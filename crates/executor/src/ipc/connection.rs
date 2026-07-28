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
use std::time::Duration;

use crate::{
    Command, DurableLocalOpenOptions, Executor, ExecutorResult, IpcMode, Output, DEFAULT_BRANCH,
    DEFAULT_SPACE,
};

use super::client::IpcClient;
use super::resolve_connect;
use super::server::IpcServer;

/// The engine's lock-contention wire code — the signal that another process
/// owns the store (see the RFC trace: `local_fs` EWOULDBLOCK folds to this).
const LOCK_CONTENTION_CODE: &str = "unavailable.engine.persistence";
/// Poll step while riding the owner start-up / shut-down window.
const OPEN_RETRY_STEP: Duration = Duration::from_millis(25);
/// Number of poll steps (≈ the ~250ms owner-close window, doubled for headroom).
const OPEN_RETRY_STEPS: u32 = 20;

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
    /// Open a durable store with transparent multi-process brokering, driven by
    /// `ipc` (a transport policy the executor owns; the engine `options` stay
    /// IPC-agnostic):
    ///
    /// - [`IpcMode::Host`]: win the lock and host a socket so other processes
    ///   can broker to us (a REPL, `mcp serve`, `strata start`, a long-lived SDK
    ///   app). On contention, broker to the existing owner instead.
    /// - [`IpcMode::Client`]: win the lock but do not host (a one-shot command
    ///   that holds the lock briefly); on contention, broker as a client.
    /// - [`IpcMode::Off`]: opt out entirely — a raw local open with no server
    ///   and no fallback (hardened single-process deployments).
    ///
    /// # Errors
    ///
    /// The underlying open error, or the lock-contention error when the store
    /// is owned but no socket is reachable within the retry budget.
    pub fn open_durable_local_brokered(
        path: impl Into<PathBuf>,
        options: DurableLocalOpenOptions,
        ipc: IpcMode,
    ) -> ExecutorResult<Self> {
        let path = path.into();
        match ipc {
            // Opt out: a raw single-process open, no socket and no fallback.
            IpcMode::Off => {
                let executor = Executor::open_durable_local_with_options(&path, options)?;
                Ok(Self::local(executor, None))
            }
            // Host: win the lock and host a socket others can broker to.
            IpcMode::Host => Self::open_brokered(&path, options, true),
            // Client: win the lock but do not host; broker only on contention.
            IpcMode::Client => Self::open_brokered(&path, options, false),
        }
    }

    fn open_brokered(
        path: &Path,
        options: DurableLocalOpenOptions,
        host: bool,
    ) -> ExecutorResult<Self> {
        match Executor::open_durable_local_with_options(path, options.clone()) {
            Ok(executor) => Ok(Self::local_hosting(path, executor, host)),
            // Contention: the store is already owned. Ride the owner start-up /
            // shut-down window, brokering to its socket if one appears.
            Err(error) if error.code() == LOCK_CONTENTION_CODE => {
                Self::broker_to_owner(path, options, host)
            }
            // A non-contention open error never brokers — propagate it as-is.
            Err(error) => Err(error),
        }
    }

    /// The store was owned when we tried to open it. Ride the short, fixed owner
    /// start-up / shut-down window (bounded poll iterations, not a wall clock):
    /// an owner may be binding its socket (we become a client) or releasing the
    /// lock (we become the owner). Whichever resolves first wins; if neither does
    /// within the window, a final open returns the definitive lock error.
    ///
    /// This is a deliberately racy retry loop — its per-iteration branches are
    /// timing-dependent and convergent, so it is carved out of the mutation gate.
    fn broker_to_owner(
        path: &Path,
        options: DurableLocalOpenOptions,
        host: bool,
    ) -> ExecutorResult<Self> {
        for _ in 0..OPEN_RETRY_STEPS {
            if let Some(socket) = resolve_connect(path) {
                if let Ok(client) = IpcClient::connect(&socket) {
                    return Ok(Self::remote(client));
                }
            }
            match Executor::open_durable_local_with_options(path, options.clone()) {
                Ok(executor) => return Ok(Self::local_hosting(path, executor, host)),
                Err(error) if error.code() == LOCK_CONTENTION_CODE => {
                    std::thread::sleep(OPEN_RETRY_STEP);
                }
                Err(error) => return Err(error),
            }
        }
        Executor::open_durable_local_with_options(path, options)
            .map(|executor| Self::local_hosting(path, executor, host))
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
                Ok(server) => {
                    // Inject the live host state so `ipc_status` (in-process or
                    // forwarded from a client) can report the socket, pid, and
                    // connected-client count.
                    let state = crate::IpcHostState::new(
                        server.socket_path().to_path_buf(),
                        std::process::id(),
                        server.client_count(),
                        server.stop_signal(),
                    );
                    executor
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .set_ipc_host_state(state);
                    Some(server)
                }
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

    /// Wrap a cache (non-durable, single-process) executor as a local
    /// connection. Cache mode has no writer lock and no socket, so there is
    /// nothing to broker — this is always a plain local handle, letting every
    /// frontend hold one `Connection` type regardless of the open target.
    #[must_use]
    pub fn cache(executor: Executor) -> Self {
        Self::local(executor, None)
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

    /// This connection's current default branch (the store's default at open,
    /// or the last value set via [`Self::set_default_branch`]).
    #[must_use]
    pub fn default_branch(&self) -> String {
        self.scope
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .branch
            .clone()
    }

    /// This connection's current default space.
    #[must_use]
    pub fn default_space(&self) -> String {
        self.scope
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .space
            .clone()
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
            ConnectionInner::Remote { client } => {
                let output = client
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .execute(Some(&branch), Some(&space), &command)?;
                // The owner answered `ipc_status` as an owner would; from a
                // remote client's vantage it is not the owner. Every other
                // command passes through untouched.
                Ok(match output {
                    Output::IpcStatus(mut status) => {
                        status.is_owner = false;
                        Output::IpcStatus(status)
                    }
                    other => other,
                })
            }
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
    use crate::{Command, DurableLocalOpenOptions, Executor, IpcMode, Output};

    fn ipc_status(conn: &Connection) -> crate::types::AdminIpcStatus {
        match conn
            .execute(Command::IpcStatus {})
            .expect("ipc_status runs")
        {
            Output::IpcStatus(status) => status,
            other => panic!("expected ipc_status output, got {other:?}"),
        }
    }

    fn ipc_stop(conn: &Connection) -> crate::types::AdminIpcStop {
        match conn.execute(Command::IpcStop {}).expect("ipc_stop runs") {
            Output::IpcStop(result) => result,
            other => panic!("expected ipc_stop output, got {other:?}"),
        }
    }

    #[test]
    fn ipc_stop_halts_hosting_and_is_idempotent() {
        let dir = tempfile::tempdir().expect("tmp");
        let owner = Connection::open_durable_local_brokered(
            dir.path(),
            DurableLocalOpenOptions::new(),
            IpcMode::Host,
        )
        .expect("owner open");
        assert!(ipc_status(&owner).hosting, "hosting before stop");

        assert!(ipc_stop(&owner).stopped, "the running host is stopped");
        assert!(
            !ipc_status(&owner).hosting,
            "the host reports it is no longer hosting after stop"
        );
        assert!(!ipc_stop(&owner).stopped, "stopping again is a no-op");

        owner.close().expect("owner close");
    }

    #[test]
    fn a_client_ipc_stop_stops_the_owner_hosting() {
        let dir = tempfile::tempdir().expect("tmp");
        let owner = Connection::open_durable_local_brokered(
            dir.path(),
            DurableLocalOpenOptions::new(),
            IpcMode::Host,
        )
        .expect("owner open");
        let client = Connection::open_durable_local_brokered(
            dir.path(),
            DurableLocalOpenOptions::new(),
            IpcMode::Client,
        )
        .expect("client open");
        assert!(!client.is_local(), "second opener brokered as a client");

        // The client's `ipc_stop` forwards to the owner, which stops hosting.
        assert!(ipc_stop(&client).stopped, "the client stopped the owner");
        assert!(
            !ipc_status(&owner).hosting,
            "the owner stopped hosting at the client's request"
        );

        // Stopping actually signals the server (not just clears the state): the
        // owner's handler sees the shutdown flag and drops this client, so the
        // client's next command loses its transport.
        let mut kicked = false;
        for _ in 0..200 {
            if client.execute(Command::IpcStatus {}).is_err() {
                kicked = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(kicked, "the client is dropped once the owner stops hosting");

        owner.close().expect("owner close");
    }

    #[test]
    fn ipc_status_reports_hosting_and_flips_is_owner_for_a_client() {
        let dir = tempfile::tempdir().expect("tmp");
        let owner = Connection::open_durable_local_brokered(
            dir.path(),
            DurableLocalOpenOptions::new(),
            IpcMode::Host,
        )
        .expect("owner open");

        let owner_status = ipc_status(&owner);
        assert!(owner_status.is_owner, "the host owns the store");
        assert!(owner_status.hosting, "the host is hosting a socket");
        assert!(
            owner_status.socket_path.is_some(),
            "the host reports its socket"
        );
        assert_eq!(owner_status.owner_pid, Some(u64::from(std::process::id())));
        assert_eq!(owner_status.client_count, 0, "no clients connected yet");

        // A client brokers in; its status forwards to the owner but reports the
        // client's own ownership (false), and the live client count now sees it.
        let client = Connection::open_durable_local_brokered(
            dir.path(),
            DurableLocalOpenOptions::new(),
            IpcMode::Client,
        )
        .expect("client open");
        assert!(!client.is_local(), "second opener brokered as a client");

        let client_status = ipc_status(&client);
        assert!(!client_status.is_owner, "a client is not the owner");
        assert!(client_status.hosting, "the owner it reached is hosting");
        assert_eq!(
            client_status.owner_pid,
            Some(u64::from(std::process::id())),
            "reports the same-process owner's pid"
        );
        assert_eq!(client_status.client_count, 1, "the live client is counted");
        assert_eq!(
            ipc_status(&owner).client_count,
            1,
            "the owner sees its client"
        );

        // When the client leaves, its handler thread exits and the live count
        // returns to zero (the decrement half of the connection guard).
        client.close().expect("client close");
        let mut count = u64::MAX;
        for _ in 0..200 {
            count = ipc_status(&owner).client_count;
            if count == 0 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(
            count, 0,
            "the client count returns to zero after the client leaves"
        );
        owner.close().expect("owner close");
    }

    #[test]
    fn ipc_status_on_a_cache_connection_is_not_hosting() {
        let conn = Connection::cache(Executor::open_cache().expect("cache"));
        let status = ipc_status(&conn);
        assert!(status.is_owner, "a cache handle trivially owns its store");
        assert!(!status.hosting, "cache mode hosts nothing");
        assert!(status.socket_path.is_none());
        assert_eq!(status.owner_pid, None);
        assert_eq!(status.client_count, 0);
        conn.close().expect("close");
    }

    #[test]
    fn cache_connection_exposes_and_updates_its_default_scope() {
        // The cache constructor yields a plain local handle, and the scope
        // getters read the store default then track set_default_* — the CLI
        // relies on this for its prompt and one-shot scope resolution.
        let conn = Connection::cache(Executor::open_cache().expect("cache"));
        assert!(conn.is_local() && !conn.is_hosting(), "cache never hosts");
        assert_eq!(conn.default_branch(), crate::DEFAULT_BRANCH);
        assert_eq!(conn.default_space(), crate::DEFAULT_SPACE);
        conn.set_default_branch("feature");
        conn.set_default_space("docs");
        assert_eq!(conn.default_branch(), "feature");
        assert_eq!(conn.default_space(), "docs");
        conn.close().expect("close");
    }

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
            IpcMode::Off,
        )
        .expect("open");
        assert!(conn.is_local());
        assert!(!conn.is_hosting(), "opt-out never hosts");
        conn.close().expect("close");
    }

    #[test]
    fn client_mode_winning_the_lock_opens_local_without_hosting() {
        // Client on an uncontended store wins the lock and opens Local — but,
        // unlike Host, it must NOT host a socket (a one-shot has no business
        // spinning up a listener). This pins the host=false in the Client arm.
        let dir = tempfile::tempdir().expect("tmp");
        let conn = Connection::open_durable_local_brokered(
            dir.path(),
            DurableLocalOpenOptions::new(),
            IpcMode::Client,
        )
        .expect("open");
        assert!(conn.is_local(), "an uncontended client wins the lock");
        assert!(!conn.is_hosting(), "client never hosts a socket");
        conn.close().expect("close");
    }

    #[test]
    fn first_opener_hosts_and_second_opener_brokers_through_it() {
        let dir = tempfile::tempdir().expect("tmp");

        // Owner: a long-lived host.
        let owner = Connection::open_durable_local_brokered(
            dir.path(),
            DurableLocalOpenOptions::new(),
            IpcMode::Host,
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
            IpcMode::Client,
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
            IpcMode::Host,
        )
        .expect("owner open");

        let client = Connection::open_durable_local_brokered(
            dir.path(),
            DurableLocalOpenOptions::new(),
            IpcMode::Client,
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
    fn set_default_branch_scopes_subsequent_commands() {
        // A local connection's default branch/space must actually apply to
        // later commands (a no-op setter would silently target the wrong
        // branch).
        let dir = tempfile::tempdir().expect("tmp");
        let conn = Connection::open_durable_local_brokered(
            dir.path(),
            DurableLocalOpenOptions::new(),
            IpcMode::Off,
        )
        .expect("open");

        let created: serde_json::Value = serde_json::to_value(
            conn.execute(
                serde_json::from_value(serde_json::json!({
                    "type": "branch_create", "branch": "feature",
                }))
                .expect("cmd"),
            )
            .expect("create branch"),
        )
        .expect("json");
        assert_eq!(created["type"], "branch");

        conn.set_default_branch("feature");
        conn.execute(kv_put("aw==", "dg=="))
            .expect("put on default scope");
        // The write must be visible on `feature` (the connection default),
        // and absent on the store default.
        let on_feature = read_branch(&conn, Some("feature"), "aw==");
        assert_eq!(on_feature["data"]["value"]["value"], "dg==");
        let on_default = read_branch(&conn, Some(crate::DEFAULT_BRANCH), "aw==");
        assert_eq!(on_default["data"]["found"], false);

        conn.close().expect("close");
    }

    #[test]
    fn set_default_space_applies_to_subsequent_commands() {
        // A no-op space setter would silently leave commands on the store
        // default. Observe the setter took effect via validation: an over-long
        // default space (rejected by ProductSpace) must surface on the NEXT
        // command, proving `execute` applied the connection's space.
        let dir = tempfile::tempdir().expect("tmp");
        let conn = Connection::open_durable_local_brokered(
            dir.path(),
            DurableLocalOpenOptions::new(),
            IpcMode::Off,
        )
        .expect("open");

        conn.set_default_space("s".repeat(usize::from(u16::MAX) + 1));
        let Err(error) = conn.execute(kv_put("aw==", "dg==")) else {
            panic!("an over-long default space must be rejected when applied");
        };
        assert_eq!(error.code(), "invalid_argument.engine.product_space");
        conn.close().expect("close");
    }

    /// Read a key on an explicit branch (bypasses the connection scope).
    fn read_branch(conn: &Connection, branch: Option<&str>, key: &str) -> serde_json::Value {
        let command = serde_json::from_value(serde_json::json!({
            "type": "kv_get", "branch": branch, "key": key,
        }))
        .expect("kv_get");
        serde_json::to_value(conn.execute(command).expect("get")).expect("json")
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
            // Brokering ON (Client) — the fallback path, which must NOT swallow
            // a non-contention error as if it were a busy owner.
            DurableLocalOpenOptions::new(),
            IpcMode::Client,
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
            IpcMode::Host,
        )
        .expect("owner open");

        // IpcMode::Off must not fall back to a client — it takes the raw lock
        // path and fails with the lock-contention error.
        let refused = Connection::open_durable_local_brokered(
            dir.path(),
            DurableLocalOpenOptions::new(),
            IpcMode::Off,
        );
        let Err(error) = refused else {
            panic!("opt-out should refuse a held store, not broker");
        };
        assert_eq!(error.code(), super::LOCK_CONTENTION_CODE);

        owner.close().expect("owner close");
    }
}
