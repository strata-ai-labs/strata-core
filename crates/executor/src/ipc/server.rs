//! The owner's IPC server: a thread-per-connection Unix-domain-socket listener
//! over the one `Executor` that holds the store's writer lock.
//!
//! Every connection's requests serialize through a shared `Mutex<Executor>` —
//! correct because the engine is single-session by construction (the
//! `Database` is non-`Clone` and every operation takes `&mut self`), so there
//! is exactly one executor per store and the mutex is the natural serialization
//! point. The socket and pid files are `0600`; on drop the server unlinks them.

use std::io::{self, BufReader, BufWriter};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::Executor;

use super::dispatch::execute_wire_request;
use super::protocol::WireRequest;
use super::{pid_path, resolve_binding, wire};

/// Concurrent handler-thread cap. Excess connections are dropped rather than
/// queued — the client's open dance retries, and an embedded store never has
/// a legitimate fan-out this wide.
const MAX_CONNECTIONS: usize = 128;
/// Handler read timeout: bounds how long a handler blocks before re-checking
/// the shutdown flag, so `shutdown()` returns promptly.
const HANDLER_READ_TIMEOUT: Duration = Duration::from_secs(2);
/// Listener idle poll when no connection is pending.
const ACCEPT_POLL: Duration = Duration::from_millis(50);

/// A running IPC server. Dropping it stops the listener and unlinks the socket
/// and pid files.
pub struct IpcServer {
    socket_path: PathBuf,
    pointer_path: Option<PathBuf>,
    pid_path: PathBuf,
    shutdown: Arc<AtomicBool>,
    listener_handle: Option<JoinHandle<()>>,
}

impl IpcServer {
    /// Bind the owner's socket for `data_dir` and start serving `executor`.
    ///
    /// The caller has already won the writer lock (it holds the live
    /// `Executor`); this only adds the transport. A stale socket from a crashed
    /// prior owner is removed first.
    ///
    /// # Errors
    ///
    /// Any bind / permission / spawn failure. Callers treat this as best-effort
    /// (the store is still usable in-process; only brokering is unavailable).
    pub fn start(data_dir: &Path, executor: Arc<Mutex<Executor>>) -> io::Result<Self> {
        // Capture the owner's baseline session scope BEFORE any connection can
        // mutate it, so a request that omits branch/space resets to the store
        // default rather than inheriting a previous request's scope.
        let baseline = {
            let executor = executor
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            Arc::new(Baseline {
                branch: executor.default_branch().to_owned(),
                space: executor.default_space().to_owned(),
            })
        };
        let binding = resolve_binding(data_dir);
        if let Some(parent) = binding.socket.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if binding.socket.exists() {
            std::fs::remove_file(&binding.socket)?;
        }

        let listener = UnixListener::bind(&binding.socket)?;
        std::fs::set_permissions(&binding.socket, std::fs::Permissions::from_mode(0o600))?;
        listener.set_nonblocking(true)?;

        if let Some(pointer) = &binding.pointer {
            write_owner_file(pointer, binding.socket.to_string_lossy().as_bytes())?;
        }
        let pid_file = pid_path(data_dir);
        write_owner_file(&pid_file, std::process::id().to_string().as_bytes())?;

        let shutdown = Arc::new(AtomicBool::new(false));
        let listener_shutdown = shutdown.clone();
        let listener_handle = thread::Builder::new()
            .name("ipc-listener".into())
            .spawn(move || listener_loop(listener, executor, baseline, listener_shutdown))?;

        Ok(Self {
            socket_path: binding.socket,
            pointer_path: binding.pointer,
            pid_path: pid_file,
            shutdown,
            listener_handle: Some(listener_handle),
        })
    }

    /// The socket path this owner is listening on.
    #[must_use]
    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    /// Stop the listener, wait for it, and unlink the socket, pointer, and pid
    /// files. Idempotent; `Drop` calls the same cleanup.
    pub fn shutdown(&mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
        if let Some(handle) = self.listener_handle.take() {
            let _ = handle.join();
        }
        self.cleanup_files();
    }

    fn cleanup_files(&self) {
        let _ = std::fs::remove_file(&self.socket_path);
        if let Some(pointer) = &self.pointer_path {
            let _ = std::fs::remove_file(pointer);
        }
        let _ = std::fs::remove_file(&self.pid_path);
    }

    /// Stop an owner identified by its pid file: send `SIGTERM`, wait briefly
    /// for exit, then remove the pid, socket, and pointer files. Idempotent.
    ///
    /// # Errors
    ///
    /// Filesystem errors reading the pid file; a missing pid file is success
    /// (no owner to stop).
    pub fn stop(data_dir: &Path) -> io::Result<()> {
        let pid_file = pid_path(data_dir);
        if let Ok(contents) = std::fs::read_to_string(&pid_file) {
            if let Ok(pid) = contents.trim().parse::<i32>() {
                // Send SIGTERM without an unsafe `libc::kill` — this crate is
                // `#![deny(unsafe_code)]`; `kill(1)` is a portable equivalent.
                let _ = std::process::Command::new("kill")
                    .arg("-TERM")
                    .arg(pid.to_string())
                    .status();
                for _ in 0..20 {
                    if !process_is_alive(pid) {
                        break;
                    }
                    thread::sleep(Duration::from_millis(100));
                }
            }
            let _ = std::fs::remove_file(&pid_file);
        }
        for name in [super::SOCKET_NAME, super::POINTER_NAME] {
            let _ = std::fs::remove_file(data_dir.join(name));
        }
        Ok(())
    }
}

impl Drop for IpcServer {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
        if let Some(handle) = self.listener_handle.take() {
            let _ = handle.join();
        }
        self.cleanup_files();
    }
}

fn write_owner_file(path: &Path, bytes: &[u8]) -> io::Result<()> {
    std::fs::write(path, bytes)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
}

/// `kill -0` liveness probe (no unsafe): exit status 0 means the process
/// exists (or we may lack permission, which for a same-user pid means alive).
fn process_is_alive(pid: i32) -> bool {
    std::process::Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

/// The owner's session scope at start — the reset target for requests that
/// omit branch/space.
struct Baseline {
    branch: String,
    space: String,
}

// The listener thread's entry point owns its resources for the thread's whole
// life (the listener, the shared executor/baseline handles, the shutdown flag),
// so by-value is correct even though `accept` only borrows.
#[allow(clippy::needless_pass_by_value)]
fn listener_loop(
    listener: UnixListener,
    executor: Arc<Mutex<Executor>>,
    baseline: Arc<Baseline>,
    shutdown: Arc<AtomicBool>,
) {
    let mut handlers: Vec<JoinHandle<()>> = Vec::new();
    while !shutdown.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, _addr)) => {
                handlers.retain(|handle| !handle.is_finished());
                if handlers.len() >= MAX_CONNECTIONS {
                    tracing::warn!("IPC connection limit reached ({MAX_CONNECTIONS}); dropping");
                    drop(stream);
                    continue;
                }
                stream.set_nonblocking(false).ok();
                stream.set_read_timeout(Some(HANDLER_READ_TIMEOUT)).ok();
                let executor = executor.clone();
                let baseline = baseline.clone();
                let shutdown = shutdown.clone();
                match thread::Builder::new()
                    .name("ipc-handler".into())
                    .spawn(move || handle_connection(stream, &executor, &baseline, &shutdown))
                {
                    Ok(handle) => handlers.push(handle),
                    Err(error) => tracing::warn!("failed to spawn IPC handler: {error}"),
                }
            }
            Err(ref error) if is_transient_accept_error(error.kind()) => {
                thread::sleep(ACCEPT_POLL);
            }
            Err(error) => {
                if !shutdown.load(Ordering::SeqCst) {
                    tracing::warn!("IPC accept error: {error}");
                }
                break;
            }
        }
    }
    for handle in handlers {
        let _ = handle.join();
    }
}

fn handle_connection(
    stream: UnixStream,
    executor: &Arc<Mutex<Executor>>,
    baseline: &Baseline,
    shutdown: &AtomicBool,
) {
    let Ok(read_half) = stream.try_clone() else {
        return;
    };
    let mut reader = BufReader::new(read_half);
    let mut writer = BufWriter::new(stream);

    loop {
        if shutdown.load(Ordering::SeqCst) {
            break;
        }
        let frame = match wire::read_frame(&mut reader) {
            Ok(frame) => frame,
            // A read timeout re-checks the shutdown flag and loops; anything
            // else (a clean EOF disconnect included) ends this connection.
            Err(ref e) if is_read_retry_error(e.kind()) => {
                continue;
            }
            Err(_) => break,
        };

        let response = serve_one(executor, baseline, &frame);
        if wire::write_frame(&mut writer, response.as_bytes()).is_err() {
            break;
        }
    }
}

/// Decode one framed request, apply its session scope, and dispatch — with a
/// panic guard so a single misbehaving command cannot take down the owner. The
/// scope application and dispatch happen under one lock hold, so concurrent
/// connections cannot interleave between them.
fn serve_one(executor: &Arc<Mutex<Executor>>, baseline: &Baseline, frame: &[u8]) -> String {
    let request: WireRequest = match serde_json::from_slice(frame) {
        Ok(request) => request,
        Err(error) => return wire_request_error(&error.to_string()),
    };
    // Each request fully determines the scope: its own branch/space, or the
    // owner's baseline when omitted — never a previous request's leftover.
    let branch = request.branch.as_deref().unwrap_or(&baseline.branch);
    let space = request.space.as_deref().unwrap_or(&baseline.space);
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut executor = executor
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        executor.set_default_branch(branch.to_owned())?;
        executor.set_default_space(space.to_owned())?;
        Ok::<String, crate::ExecutorError>(execute_wire_request(
            &mut executor,
            request.command.get(),
        ))
    }));
    match outcome {
        Ok(Ok(response)) => response,
        // A rejected session scope (e.g. an invalid branch name) is a clean
        // error envelope, not a transport failure.
        Ok(Err(error)) => serde_json::to_string(&serde_json::json!({ "error": error }))
            .unwrap_or_else(|_| wire_request_error("session scope rejected")),
        Err(_) => internal_panic_error(),
    }
}

/// A transient `accept()` outcome (the listener's non-blocking poll expiring)
/// versus a real listener failure. Transient → re-poll; anything else ends the
/// listener. Extracted as a pure predicate so its classification is unit-tested
/// directly (the accept loop itself is OS-timing-driven).
fn is_transient_accept_error(kind: io::ErrorKind) -> bool {
    kind == io::ErrorKind::WouldBlock
}

/// A read error that should keep the connection alive (the per-request read
/// timeout firing) versus one that ends it (a clean EOF or hard error).
/// Extracted as a pure predicate for the same reason.
fn is_read_retry_error(kind: io::ErrorKind) -> bool {
    matches!(kind, io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut)
}

fn wire_request_error(detail: &str) -> String {
    let error = crate::ExecutorError::invalid_input(
        "invalid_argument.executor.wire_request",
        format!("malformed IPC request envelope: {detail}"),
    );
    serde_json::to_string(&serde_json::json!({ "error": error }))
        .unwrap_or_else(|_| static_wire_request_error())
}

fn internal_panic_error() -> String {
    let error = crate::ExecutorError::new(
        crate::error::ExecutorErrorClass::Internal,
        "internal.executor.wire_response",
        false,
        "the IPC handler panicked while serving a command",
    );
    serde_json::to_string(&serde_json::json!({ "error": error }))
        .unwrap_or_else(|_| static_wire_request_error())
}

fn static_wire_request_error() -> String {
    "{\"error\":{\"class\":\"invalid_argument\",\"code\":\"invalid_argument.executor.wire_request\",\
     \"message\":\"malformed IPC request envelope\"}}"
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::IpcServer;
    use crate::ipc::protocol::WireRequestOwned;
    use crate::ipc::wire;
    use crate::Executor;
    use std::io::{BufReader, BufWriter};
    use std::os::unix::net::UnixStream;
    use std::sync::{Arc, Mutex};

    /// Send a wire command with optional scope and read the response envelope.
    fn round_trip(
        sock: &std::path::Path,
        branch: Option<&str>,
        command: &str,
    ) -> serde_json::Value {
        let stream = UnixStream::connect(sock).expect("connect");
        let mut writer = BufWriter::new(stream.try_clone().expect("clone"));
        let mut reader = BufReader::new(stream);
        let raw = serde_json::value::RawValue::from_string(command.to_owned()).expect("raw");
        let request = WireRequestOwned {
            branch,
            space: None,
            command: &raw,
        };
        let payload = serde_json::to_vec(&request).expect("serialize");
        wire::write_frame(&mut writer, &payload).expect("write");
        let response = wire::read_frame(&mut reader).expect("read");
        serde_json::from_slice(&response).expect("decode response")
    }

    #[test]
    fn process_is_alive_distinguishes_a_live_pid_from_a_dead_one() {
        // Our own process is alive; i32::MAX is not a running pid.
        let own = i32::try_from(std::process::id()).expect("pid fits i32");
        assert!(super::process_is_alive(own), "this process is alive");
        assert!(
            !super::process_is_alive(i32::MAX),
            "i32::MAX is not a running process"
        );
    }

    #[test]
    fn transient_accept_errors_are_exactly_would_block() {
        use std::io::ErrorKind;
        // Only a poll-timeout (WouldBlock) re-polls; every other kind ends the
        // listener. A wrong predicate would either spin on real failures or drop
        // the listener on a benign timeout.
        assert!(super::is_transient_accept_error(ErrorKind::WouldBlock));
        assert!(!super::is_transient_accept_error(ErrorKind::TimedOut));
        assert!(!super::is_transient_accept_error(
            ErrorKind::ConnectionAborted
        ));
        assert!(!super::is_transient_accept_error(ErrorKind::Other));
    }

    #[test]
    fn read_retry_errors_are_the_timeout_kinds_only() {
        use std::io::ErrorKind;
        // A read timeout (WouldBlock OR TimedOut, platform-dependent) keeps the
        // connection alive; a clean EOF or hard error ends it. `&&` instead of
        // `||`, or dropping either kind, would strand paused clients.
        assert!(super::is_read_retry_error(ErrorKind::WouldBlock));
        assert!(super::is_read_retry_error(ErrorKind::TimedOut));
        assert!(!super::is_read_retry_error(ErrorKind::UnexpectedEof));
        assert!(!super::is_read_retry_error(ErrorKind::BrokenPipe));
    }

    #[test]
    fn dropping_the_server_without_an_explicit_shutdown_still_cleans_up() {
        // RAII: an owner dropped without an explicit shutdown() — e.g. a panic
        // unwinds its scope — must still unlink its socket and pid file, or a
        // crashed owner would strand a dead socket that blocks reconnection.
        let dir = tempfile::tempdir().expect("tmp");
        let executor = Arc::new(Mutex::new(Executor::open_cache().expect("cache")));
        let sock;
        let pid = dir.path().join("strata.pid");
        {
            let server = IpcServer::start(dir.path(), executor).expect("start");
            sock = server.socket_path().to_path_buf();
            assert!(sock.exists(), "socket present while the owner is alive");
            assert!(pid.exists(), "pid file present while the owner is alive");
            // `server` drops here — no explicit shutdown().
        }
        assert!(!sock.exists(), "socket unlinked on drop");
        assert!(!pid.exists(), "pid file unlinked on drop");
    }

    #[test]
    fn start_writes_an_owner_pid_file_and_shutdown_removes_it() {
        let dir = tempfile::tempdir().expect("tmp");
        let executor = Arc::new(Mutex::new(Executor::open_cache().expect("cache")));
        let mut server = IpcServer::start(dir.path(), executor).expect("start");
        let pid_file = dir.path().join("strata.pid");
        let recorded = std::fs::read_to_string(&pid_file).expect("pid file written");
        assert_eq!(
            recorded.trim(),
            std::process::id().to_string(),
            "pid file records our pid"
        );
        server.shutdown();
        assert!(!pid_file.exists(), "pid file removed on shutdown");
    }

    #[test]
    fn stop_removes_the_socket_and_pid_files_for_a_dead_owner() {
        // A crashed owner leaves stale files with a dead pid; stop() clears them.
        let dir = tempfile::tempdir().expect("tmp");
        std::fs::write(dir.path().join("strata.pid"), i32::MAX.to_string()).expect("pid");
        std::fs::write(dir.path().join("strata.sock"), b"").expect("sock");
        IpcServer::stop(dir.path()).expect("stop");
        assert!(!dir.path().join("strata.pid").exists(), "pid removed");
        assert!(!dir.path().join("strata.sock").exists(), "socket removed");
    }

    #[test]
    fn a_well_formed_json_frame_that_is_not_a_request_returns_a_wire_request_error() {
        let dir = tempfile::tempdir().expect("tmp");
        let executor = Arc::new(Mutex::new(Executor::open_cache().expect("cache")));
        let mut server = IpcServer::start(dir.path(), executor).expect("start");
        let sock = server.socket_path().to_path_buf();

        // Valid JSON, but not a `{branch,space,command}` envelope.
        let stream = UnixStream::connect(&sock).expect("connect");
        let mut writer = BufWriter::new(stream.try_clone().expect("clone"));
        let mut reader = BufReader::new(stream);
        wire::write_frame(&mut writer, b"[1,2,3]").expect("write");
        let response = wire::read_frame(&mut reader).expect("read");
        let response: serde_json::Value = serde_json::from_slice(&response).expect("decode");
        assert_eq!(
            response["error"]["code"], "invalid_argument.executor.wire_request",
            "a non-envelope frame is a wire_request error: {response}"
        );

        server.shutdown();
    }

    #[test]
    fn a_connection_idle_past_the_read_timeout_is_still_served() {
        // The handler's read timeout must CONTINUE (re-check shutdown) rather
        // than drop the connection — a client that pauses longer than
        // HANDLER_READ_TIMEOUT then sends a command must still get a response.
        let dir = tempfile::tempdir().expect("tmp");
        let executor = Arc::new(Mutex::new(Executor::open_cache().expect("cache")));
        let mut server = IpcServer::start(dir.path(), executor).expect("start");
        let sock = server.socket_path().to_path_buf();

        let stream = UnixStream::connect(&sock).expect("connect");
        let mut writer = BufWriter::new(stream.try_clone().expect("clone"));
        let mut reader = BufReader::new(stream);
        std::thread::sleep(super::HANDLER_READ_TIMEOUT + std::time::Duration::from_millis(500));

        let raw = serde_json::value::RawValue::from_string("{\"type\":\"ping\"}".to_owned())
            .expect("raw");
        let request = WireRequestOwned {
            branch: None,
            space: None,
            command: &raw,
        };
        wire::write_frame(&mut writer, &serde_json::to_vec(&request).expect("ser")).expect("write");
        let response = wire::read_frame(&mut reader).expect("handler continued past the timeout");
        let response: serde_json::Value = serde_json::from_slice(&response).expect("decode");
        assert_eq!(response["type"], "pong", "served after an idle pause");

        server.shutdown();
    }

    #[test]
    fn serves_commands_and_error_envelopes_over_the_socket() {
        let dir = tempfile::tempdir().expect("tmp");
        let executor = Arc::new(Mutex::new(Executor::open_cache().expect("cache")));
        let mut server = IpcServer::start(dir.path(), executor).expect("start");
        let sock = server.socket_path().to_path_buf();

        // A put then a get round-trip the same value.
        let put = round_trip(
            &sock,
            None,
            "{\"type\":\"kv_put\",\"key\":\"aGk=\",\"value\":\"dGhlcmU=\"}",
        );
        assert_eq!(put["type"], "write_result", "put succeeded: {put}");
        let get = round_trip(&sock, None, "{\"type\":\"kv_get\",\"key\":\"aGk=\"}");
        assert_eq!(get["type"], "kv_versioned_value");
        assert_eq!(get["data"]["value"]["value"], "dGhlcmU=");

        // A malformed command comes back as a registered error envelope.
        let bad = round_trip(&sock, None, "{\"type\":\"nope\"}");
        assert_eq!(
            bad["error"]["code"],
            "invalid_argument.executor.wire_request"
        );

        server.shutdown();
        assert!(!sock.exists(), "socket removed on shutdown");
    }

    #[test]
    fn per_request_branch_scope_is_applied() {
        let dir = tempfile::tempdir().expect("tmp");
        let executor = Arc::new(Mutex::new(Executor::open_cache().expect("cache")));
        let mut server = IpcServer::start(dir.path(), executor).expect("start");
        let sock = server.socket_path().to_path_buf();

        // Create a branch, then a put/get scoped to it via the request envelope
        // (no explicit branch on the command) must target that branch.
        let created = round_trip(
            &sock,
            None,
            "{\"type\":\"branch_create\",\"branch\":\"feature\"}",
        );
        assert_eq!(created["type"], "branch", "branch created: {created}");
        round_trip(
            &sock,
            Some("feature"),
            "{\"type\":\"kv_put\",\"key\":\"aw==\",\"value\":\"dg==\"}",
        );
        let on_feature = round_trip(
            &sock,
            Some("feature"),
            "{\"type\":\"kv_get\",\"key\":\"aw==\"}",
        );
        assert_eq!(
            on_feature["data"]["value"]["value"], "dg==",
            "value on feature branch"
        );
        let on_main = round_trip(&sock, None, "{\"type\":\"kv_get\",\"key\":\"aw==\"}");
        assert_eq!(
            on_main["data"]["found"], false,
            "key absent on the default branch"
        );

        server.shutdown();
    }
}
