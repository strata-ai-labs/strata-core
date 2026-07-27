//! `Connection` — the transport-transparent handle every frontend opens.
//!
//! A connection is either `Local` (this process won the writer lock and holds
//! the one `Executor`, optionally hosting a socket for others) or `Remote`
//! (another process owns the store and we speak to it over its socket). Both
//! present the same `execute(Command) -> ExecutorResult<Output>`, so callers
//! never branch on which they got.
//!
//! `open_durable_local_brokered` is the open dance: try to win the lock; on
//! contention, connect to the owner's socket; a bounded retry rides the
//! ~250ms window where an owner is starting up or shutting down (so the lock
//! or the socket is briefly in flux) before either result becomes final.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::{
    Command, DurableLocalOpenOptions, Executor, ExecutorResult, Output, DEFAULT_BRANCH,
    DEFAULT_SPACE,
};

use super::client::IpcClient;
use super::resolve_connect;
use super::server::IpcServer;

/// The engine's lock-contention wire code — the signal that another process
/// owns the store (see the RFC trace: `local_fs` EWOULDBLOCK folds to this).
const LOCK_CONTENTION_CODE: &str = "unavailable.engine.persistence";
/// How long the open dance rides the owner start-up / shut-down window before
/// giving a final answer.
const OPEN_RETRY_BUDGET: Duration = Duration::from_millis(500);
/// Poll step while waiting on that window.
const OPEN_RETRY_STEP: Duration = Duration::from_millis(25);

/// A transport-transparent database connection.
pub struct Connection {
    inner: ConnectionInner,
    scope: Mutex<Scope>,
}

struct Scope {
    branch: String,
    space: String,
}

enum ConnectionInner {
    /// This process owns the store. The server (when present) shares this exact
    /// `Arc<Mutex<Executor>>`, so in-process and socket clients serialize
    /// through one executor; dropping the server unlinks the socket.
    Local {
        executor: Arc<Mutex<Executor>>,
        server: Option<IpcServer>,
    },
    /// Another process owns the store; we are a client of its socket.
    Remote { client: Mutex<IpcClient> },
}

impl Connection {
    /// Open a durable store with transparent multi-process brokering.
    ///
    /// - `host`: when we win the lock AND this is a long-lived owner (a REPL,
    ///   `mcp serve`, `strata start`, an SDK app), host a socket so others can
    ///   connect. One-shot commands pass `false` — they hold the lock briefly
    ///   and rely on the client fallback for the rare collision.
    /// - `ipc`: master switch. `false` opts out entirely: a raw local open with
    ///   no server and no fallback (hardened single-process deployments).
    ///
    /// # Errors
    ///
    /// The underlying open error, or the lock-contention error when the store
    /// is owned but no socket is reachable within the retry budget.
    pub fn open_durable_local_brokered(
        path: impl Into<PathBuf>,
        options: DurableLocalOpenOptions,
        host: bool,
        ipc: bool,
    ) -> ExecutorResult<Self> {
        let path = path.into();
        if !ipc {
            let executor = Executor::open_durable_local_with_options(&path, options)?;
            return Ok(Self::local(executor, None));
        }
        Self::open_brokered(&path, options, host)
    }

    fn open_brokered(
        path: &Path,
        options: DurableLocalOpenOptions,
        host: bool,
    ) -> ExecutorResult<Self> {
        match Executor::open_durable_local_with_options(path, options.clone()) {
            Ok(executor) => return Ok(Self::local_hosting(path, executor, host)),
            Err(error) if error.code() == LOCK_CONTENTION_CODE => {}
            Err(error) => return Err(error),
        }

        // The store is owned. Ride the start-up / shut-down window: an owner may
        // be binding its socket (become a client) or releasing the lock (become
        // the owner). Whichever resolves first within the budget wins.
        let deadline = Instant::now() + OPEN_RETRY_BUDGET;
        loop {
            if let Some(socket) = resolve_connect(path) {
                if let Ok(client) = IpcClient::connect(&socket) {
                    return Ok(Self::remote(client));
                }
            }
            if Instant::now() >= deadline {
                // Final attempt to win the lock; on contention this returns the
                // definitive lock error to the caller.
                return Executor::open_durable_local_with_options(path, options)
                    .map(|executor| Self::local_hosting(path, executor, host));
            }
            match Executor::open_durable_local_with_options(path, options.clone()) {
                Ok(executor) => return Ok(Self::local_hosting(path, executor, host)),
                Err(error) if error.code() == LOCK_CONTENTION_CODE => {
                    std::thread::sleep(OPEN_RETRY_STEP);
                }
                Err(error) => return Err(error),
            }
        }
    }

    fn local_hosting(path: &Path, executor: Executor, host: bool) -> Self {
        let scope = Scope {
            branch: executor.default_branch().to_owned(),
            space: executor.default_space().to_owned(),
        };
        let executor = Arc::new(Mutex::new(executor));
        // Hosting is best-effort: a bind failure leaves the store fully usable
        // in-process, only unable to broker (mirrors the fork-manifest arm's
        // health-debt posture).
        let server = if host {
            match IpcServer::start(path, executor.clone()) {
                Ok(server) => Some(server),
                Err(error) => {
                    tracing::warn!(
                        "IPC server not started (store usable in-process only): {error}"
                    );
                    None
                }
            }
        } else {
            None
        };
        Self {
            inner: ConnectionInner::Local { executor, server },
            scope: Mutex::new(scope),
        }
    }

    fn local(executor: Executor, server: Option<IpcServer>) -> Self {
        let scope = Scope {
            branch: executor.default_branch().to_owned(),
            space: executor.default_space().to_owned(),
        };
        Self {
            inner: ConnectionInner::Local {
                executor: Arc::new(Mutex::new(executor)),
                server,
            },
            scope: Mutex::new(scope),
        }
    }

    fn remote(client: IpcClient) -> Self {
        Self {
            inner: ConnectionInner::Remote {
                client: Mutex::new(client),
            },
            scope: Mutex::new(Scope {
                branch: DEFAULT_BRANCH.to_owned(),
                space: DEFAULT_SPACE.to_owned(),
            }),
        }
    }

    /// Whether this process owns the store (vs is a client of another owner).
    #[must_use]
    pub fn is_local(&self) -> bool {
        matches!(self.inner, ConnectionInner::Local { .. })
    }

    /// Whether this connection is hosting a socket for other processes.
    #[must_use]
    pub fn is_hosting(&self) -> bool {
        matches!(
            &self.inner,
            ConnectionInner::Local {
                server: Some(_),
                ..
            }
        )
    }

    /// Set this connection's default branch. Applied to every subsequent
    /// command (locally by setting the executor default under the lock,
    /// remotely by sending it in the request envelope).
    pub fn set_default_branch(&self, branch: impl Into<String>) {
        self.scope
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .branch = branch.into();
    }

    /// Set this connection's default space.
    pub fn set_default_space(&self, space: impl Into<String>) {
        self.scope
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .space = space.into();
    }

    /// Execute a command, returning the same typed result whether the store is
    /// local or remote.
    ///
    /// # Errors
    ///
    /// The command's own error, or a transport error if a remote owner's
    /// connection fails.
    pub fn execute(&self, command: Command) -> ExecutorResult<Output> {
        let (branch, space) = {
            let scope = self
                .scope
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            (scope.branch.clone(), scope.space.clone())
        };
        match &self.inner {
            ConnectionInner::Local { executor, .. } => {
                let mut executor = executor
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                executor.set_default_branch(branch)?;
                executor.set_default_space(space)?;
                executor.execute(command)
            }
            ConnectionInner::Remote { client } => client
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .execute(Some(&branch), Some(&space), &command),
        }
    }

    /// Close the connection: for a local owner, drop the server (unlink the
    /// socket) and close the executor; for a remote client, drop the socket.
    ///
    /// # Errors
    ///
    /// The executor close error, for a local owner.
    pub fn close(self) -> ExecutorResult<()> {
        match self.inner {
            ConnectionInner::Local { executor, server } => {
                drop(server); // stop the listener and unlink the socket first
                if let Ok(mutex) = Arc::try_unwrap(executor) {
                    let mut executor = mutex
                        .into_inner()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    executor.close()?;
                }
                Ok(())
            }
            ConnectionInner::Remote { .. } => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Connection;
    use crate::{Command, DurableLocalOpenOptions};

    fn kv_put(key: &str, value: &str) -> Command {
        serde_json::from_value(serde_json::json!({
            "type": "kv_put", "key": key, "value": value,
        }))
        .expect("kv_put command")
    }

    fn kv_get(key: &str) -> Command {
        serde_json::from_value(serde_json::json!({ "type": "kv_get", "key": key }))
            .expect("kv_get command")
    }

    #[test]
    fn opt_out_opens_a_plain_local_connection() {
        let dir = tempfile::tempdir().expect("tmp");
        let conn = Connection::open_durable_local_brokered(
            dir.path(),
            DurableLocalOpenOptions::new(),
            false,
            false, // ipc off
        )
        .expect("open");
        assert!(conn.is_local());
        assert!(!conn.is_hosting(), "opt-out never hosts");
        conn.close().expect("close");
    }

    #[test]
    fn first_opener_hosts_and_second_opener_brokers_through_it() {
        let dir = tempfile::tempdir().expect("tmp");

        // Owner: a long-lived host.
        let owner = Connection::open_durable_local_brokered(
            dir.path(),
            DurableLocalOpenOptions::new(),
            true, // host
            true, // ipc on
        )
        .expect("owner open");
        assert!(
            owner.is_local() && owner.is_hosting(),
            "owner hosts a socket"
        );

        // Owner writes a value.
        let put = owner
            .execute(kv_put("aGk=", "b3duZXI="))
            .expect("owner put");
        assert_eq!(
            serde_json::to_value(&put).expect("json")["type"],
            "write_result"
        );

        // Second opener finds the lock held and becomes a client.
        let client = Connection::open_durable_local_brokered(
            dir.path(),
            DurableLocalOpenOptions::new(),
            false, // a one-shot: no hosting, fallback only
            true,
        )
        .expect("client open");
        assert!(
            !client.is_local(),
            "second opener brokered as a remote client"
        );

        // The client reads the owner's write, and its own write is visible to
        // the owner — one store, two processes.
        let got = client.execute(kv_get("aGk=")).expect("client get");
        let got = serde_json::to_value(&got).expect("json");
        assert_eq!(
            got["data"]["value"]["value"], "b3duZXI=",
            "client sees owner's write"
        );

        client
            .execute(kv_put("Ynll", "Y2xpZW50"))
            .expect("client put");
        let owner_sees = owner.execute(kv_get("Ynll")).expect("owner get");
        let owner_sees = serde_json::to_value(&owner_sees).expect("json");
        assert_eq!(
            owner_sees["data"]["value"]["value"], "Y2xpZW50",
            "owner sees client's write"
        );

        client.close().expect("client close");
        owner.close().expect("owner close");
    }

    #[test]
    fn a_remote_command_after_the_owner_is_gone_is_a_transport_error() {
        let dir = tempfile::tempdir().expect("tmp");
        let owner = Connection::open_durable_local_brokered(
            dir.path(),
            DurableLocalOpenOptions::new(),
            true,
            true,
        )
        .expect("owner open");

        let client = Connection::open_durable_local_brokered(
            dir.path(),
            DurableLocalOpenOptions::new(),
            false,
            true,
        )
        .expect("client open");
        assert!(!client.is_local(), "brokered as a client");

        // The owner exits; the client's next command loses its transport and
        // surfaces the registered, in-doubt transport error.
        owner.close().expect("owner close");
        let error = client
            .execute(kv_get("aGk="))
            .expect_err("a command with no owner fails");
        assert_eq!(error.code(), "unavailable.executor.ipc_transport");
        assert_eq!(error.class(), crate::ExecutorErrorClass::Unavailable);
    }

    #[test]
    fn a_non_contention_open_error_propagates_without_brokering() {
        // A regular file (not a database directory) fails with a permanent,
        // non-lock error. The brokered open must surface THAT error, not treat
        // it as contention and try to connect to a nonexistent owner.
        let dir = tempfile::tempdir().expect("tmp");
        let path = dir.path().join("not-a-db");
        std::fs::write(&path, b"plain file").expect("write file");
        let refused = Connection::open_durable_local_brokered(
            &path,
            DurableLocalOpenOptions::new(),
            false,
            true, // ipc ON — the fallback path, which must NOT swallow this
        );
        let Err(error) = refused else {
            panic!("a regular file is not a database");
        };
        assert_eq!(error.code(), "invalid_argument.engine.persistence");
        assert_ne!(error.code(), super::LOCK_CONTENTION_CODE);
    }

    #[test]
    fn opt_out_client_refuses_a_held_store_instead_of_brokering() {
        let dir = tempfile::tempdir().expect("tmp");
        let owner = Connection::open_durable_local_brokered(
            dir.path(),
            DurableLocalOpenOptions::new(),
            true,
            true,
        )
        .expect("owner open");

        // ipc=false must not fall back to a client — it takes the raw lock path
        // and fails with the lock-contention error.
        let refused = Connection::open_durable_local_brokered(
            dir.path(),
            DurableLocalOpenOptions::new(),
            false,
            false, // ipc off
        );
        let Err(error) = refused else {
            panic!("opt-out should refuse a held store, not broker");
        };
        assert_eq!(error.code(), super::LOCK_CONTENTION_CODE);

        owner.close().expect("owner close");
    }
}
