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
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::Executor;

use super::dispatch::execute_wire_request;
use super::protocol::{self, HelloFrame, ServerHello, WireRequest};
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
    /// Live count of connected clients, incremented/decremented by each handler
    /// thread and read by `ipc_status` through the injected host state.
    client_count: Arc<AtomicUsize>,
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
        let client_count = Arc::new(AtomicUsize::new(0));
        let listener_client_count = client_count.clone();
        let listener_handle =
            thread::Builder::new()
                .name("ipc-listener".into())
                .spawn(move || {
                    listener_loop(
                        listener,
                        executor,
                        baseline,
                        listener_shutdown,
                        listener_client_count,
                    );
                })?;

        Ok(Self {
            socket_path: binding.socket,
            pointer_path: binding.pointer,
            pid_path: pid_file,
            shutdown,
            client_count,
            listener_handle: Some(listener_handle),
        })
    }

    /// The socket path this owner is listening on.
    #[must_use]
    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    /// The live connected-client counter, shared with the handler threads.
    #[must_use]
    pub fn client_count(&self) -> Arc<AtomicUsize> {
        self.client_count.clone()
    }

    /// The shutdown flag; setting it stops the listener (`ipc_stop`).
    #[must_use]
    pub fn stop_signal(&self) -> Arc<AtomicBool> {
        self.shutdown.clone()
    }

    /// Whether shutdown has been signaled (via `shutdown()`/`stop_signal()`, or
    /// an `ipc_stop` that flipped the shared flag). Unlike the presence of the
    /// server itself, this reflects the *live* state: a headless `strata start`
    /// owner polls it to learn when its hosting was stopped so it can exit.
    #[must_use]
    pub fn is_stopped(&self) -> bool {
        self.shutdown.load(Ordering::SeqCst)
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
    client_count: Arc<AtomicUsize>,
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
                let client_count = client_count.clone();
                match thread::Builder::new()
                    .name("ipc-handler".into())
                    .spawn(move || {
                        handle_connection(stream, &executor, &baseline, &shutdown, client_count);
                    }) {
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

/// RAII connected-client counter: bumps the live count for this handler
/// thread's whole life and decrements on every exit path (EOF, timeout, error,
/// or panic), so `ipc_status` never leaks a stale connection.
struct ConnectionGuard(Arc<AtomicUsize>);

impl ConnectionGuard {
    fn enter(count: Arc<AtomicUsize>) -> Self {
        count.fetch_add(1, Ordering::Relaxed);
        Self(count)
    }
}

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
}

fn handle_connection(
    stream: UnixStream,
    executor: &Arc<Mutex<Executor>>,
    baseline: &Baseline,
    shutdown: &AtomicBool,
    client_count: Arc<AtomicUsize>,
) {
    let _connected = ConnectionGuard::enter(client_count);
    let Ok(read_half) = stream.try_clone() else {
        return;
    };
    let mut reader = BufReader::new(read_half);
    let mut writer = BufWriter::new(stream);

    let mut awaiting_first_frame = true;
    // Whether this connection negotiated protocol revision 2 (hello accepted):
    // responses then carry the request's correlation id in a transport frame.
    let mut correlated = false;
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

        // A hello is only meaningful as the connection's first frame: it
        // negotiates protocol revision 2 before any command. A first frame
        // that is instead a bare request keeps the connection on the implicit
        // protocol 1 — transitional acceptance, to be removed before the
        // release train once every in-family client sends a hello (#2872,
        // design doc §4.1). A hello arriving later is not sniffed and falls
        // through as a malformed request envelope.
        if std::mem::take(&mut awaiting_first_frame) && protocol::frame_is_hello(&frame) {
            match serve_hello(&frame) {
                Ok(response) => {
                    if wire::write_frame(&mut writer, response.as_bytes()).is_err() {
                        break;
                    }
                    correlated = true;
                    continue;
                }
                Err(refusal) => {
                    // Best-effort refusal: the connection is closing over a
                    // protocol violation either way; a failed write of the
                    // refusal changes nothing for the server.
                    let _ = wire::write_frame(&mut writer, refusal.as_bytes());
                    break;
                }
            }
        }

        let response = if correlated {
            serve_one_correlated(executor, baseline, &frame)
        } else {
            serve_one(executor, baseline, &frame)
        };
        if wire::write_frame(&mut writer, response.as_bytes()).is_err() {
            break;
        }
    }
}

/// Serve a hello first frame: strict parse, protocol check, capability grant.
/// `Ok` is the `ipc_hello` response envelope and the connection continues on
/// protocol revision 2; `Err` is a refusal envelope and the connection closes
/// (a client that cannot hello correctly has nothing safe to say next).
fn serve_hello(frame: &[u8]) -> Result<String, String> {
    let request: HelloFrame = match serde_json::from_slice(frame) {
        Ok(request) => request,
        Err(error) => return Err(ipc_hello_error(&format!("malformed hello frame: {error}"))),
    };
    let hello = request.hello;
    if hello.protocol != protocol::PROTOCOL_VERSION {
        return Err(ipc_hello_error(&format!(
            "unsupported protocol revision {}; this owner speaks revision {}",
            hello.protocol,
            protocol::PROTOCOL_VERSION
        )));
    }
    // Identity and declared access are observability-only in this revision:
    // status reporting and server-side read-only enforcement build on them.
    tracing::debug!(client = ?hello.client, access = ?hello.access, "IPC hello");
    let capabilities: Vec<String> = hello
        .capabilities
        .iter()
        .filter(|name| protocol::SUPPORTED_CAPABILITIES.contains(&name.as_str()))
        .cloned()
        .collect();
    let response = ServerHello {
        protocol: protocol::PROTOCOL_VERSION,
        release: env!("CARGO_PKG_VERSION").to_owned(),
        idl: protocol::build_idl_stamps(),
        granted_access: hello.access,
        capabilities,
        owner_pid: std::process::id(),
    };
    Ok(
        serde_json::to_string(&serde_json::json!({ "type": "ipc_hello", "data": response }))
            .unwrap_or_else(|error| serialize_hello_failure(&error)),
    )
}

/// Last-resort envelope for a hello response that fails to serialize —
/// practically unreachable with plain-data fields, mirrored on the dispatch
/// path's serialize fallback.
fn serialize_hello_failure(error: &serde_json::Error) -> String {
    let status = crate::ExecutorError::new(
        crate::error::ExecutorErrorClass::Internal,
        "internal.executor.wire_response",
        false,
        format!("hello response serialization failed: {error}"),
    );
    serde_json::to_string(&serde_json::json!({ "error": status }))
        .unwrap_or_else(|_| static_wire_request_error())
}

fn ipc_hello_error(detail: &str) -> String {
    let error = crate::ExecutorError::invalid_input(
        "invalid_argument.executor.ipc_hello",
        format!("IPC hello refused: {detail}"),
    );
    serde_json::to_string(&serde_json::json!({ "error": error }))
        .unwrap_or_else(|_| static_wire_request_error())
}

/// Serve one frame on the implicit protocol 1: decode and dispatch, answering
/// with the bare executor envelope. A correlation id on a protocol-1 request
/// is ignored rather than rejected — there is no response frame to echo it in,
/// and a pre-hello owner never knew the field existed.
fn serve_one(executor: &Arc<Mutex<Executor>>, baseline: &Baseline, frame: &[u8]) -> String {
    let request: WireRequest = match serde_json::from_slice(frame) {
        Ok(request) => request,
        Err(error) => return wire_request_error(&error.to_string()),
    };
    dispatch_request(executor, baseline, &request)
}

/// Serve one frame on protocol revision 2: the request must carry a
/// correlation id, and the response is `{"id", "payload"}` with the untouched
/// executor envelope as the payload. `id` is `null` only when the request's
/// own id could not be read.
fn serve_one_correlated(
    executor: &Arc<Mutex<Executor>>,
    baseline: &Baseline,
    frame: &[u8],
) -> String {
    let request: WireRequest = match serde_json::from_slice(frame) {
        Ok(request) => request,
        Err(error) => return correlate(None, &wire_request_error(&error.to_string())),
    };
    let Some(id) = request.id else {
        return correlate(
            None,
            &wire_request_error("a protocol-revision-2 request requires a correlation id"),
        );
    };
    correlate(Some(id), &dispatch_request(executor, baseline, &request))
}

/// Wrap an executor response envelope in a protocol-revision-2 response frame.
fn correlate(id: Option<u64>, payload: &str) -> String {
    match serde_json::value::RawValue::from_string(payload.to_owned()) {
        Ok(raw) => serde_json::to_string(&protocol::WireResponseFrame { id, payload: &raw })
            .unwrap_or_else(|error| {
                serialize_failure_frame(
                    id,
                    &format!("response frame serialization failed: {error}"),
                )
            }),
        // Unreachable with the well-formed JSON every response path produces;
        // kept total so a correlated connection always receives a frame.
        Err(error) => {
            serialize_failure_frame(id, &format!("response payload was not JSON: {error}"))
        }
    }
}

/// Last-resort correlated frame, hand-built so even the serialize-failure
/// path answers in the shape a protocol-revision-2 client is parsing.
fn serialize_failure_frame(id: Option<u64>, detail: &str) -> String {
    let id = id.map_or("null".to_owned(), |id| id.to_string());
    let message = detail.replace(['"', '\\'], "'");
    format!(
        "{{\"id\":{id},\"payload\":{{\"error\":{{\"class\":\"internal\",\
         \"code\":\"internal.executor.wire_response\",\"message\":\"{message}\"}}}}}}"
    )
}

/// Decode one parsed request's scope and dispatch it — with a panic guard so a
/// single misbehaving command cannot take down the owner. The scope
/// application and dispatch happen under one lock hold, so concurrent
/// connections cannot interleave between them.
fn dispatch_request(
    executor: &Arc<Mutex<Executor>>,
    baseline: &Baseline,
    request: &WireRequest,
) -> String {
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

    /// Send a legacy (protocol 1) wire command with optional scope and read
    /// the bare response envelope.
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
            id: None,
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
    fn is_stopped_tracks_the_shutdown_signal() {
        // A live host reports not-stopped; setting the stop signal (the exact
        // flag `ipc_stop` flips) makes it report stopped, without dropping the
        // server. `strata start` polls this to learn when to exit.
        let dir = tempfile::tempdir().expect("tmp");
        let executor = Arc::new(Mutex::new(Executor::open_cache().expect("cache")));
        let mut server = IpcServer::start(dir.path(), executor).expect("start");
        assert!(!server.is_stopped(), "a running host is not stopped");
        server
            .stop_signal()
            .store(true, std::sync::atomic::Ordering::SeqCst);
        assert!(
            server.is_stopped(),
            "flipping the shared stop signal is observed as stopped"
        );
        server.shutdown();
        assert!(server.is_stopped(), "an explicit shutdown reports stopped");
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
            id: None,
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

    /// One raw connection with reader/writer halves, for multi-frame tests.
    struct RawConn {
        writer: BufWriter<UnixStream>,
        reader: BufReader<UnixStream>,
    }

    impl RawConn {
        fn connect(sock: &std::path::Path) -> Self {
            let stream = UnixStream::connect(sock).expect("connect");
            Self {
                writer: BufWriter::new(stream.try_clone().expect("clone")),
                reader: BufReader::new(stream),
            }
        }

        fn send(&mut self, payload: &[u8]) -> serde_json::Value {
            wire::write_frame(&mut self.writer, payload).expect("write");
            let response = wire::read_frame(&mut self.reader).expect("read");
            serde_json::from_slice(&response).expect("decode response")
        }

        /// A legacy (protocol 1) request: no correlation id.
        fn send_command(&mut self, command: &str) -> serde_json::Value {
            let raw = serde_json::value::RawValue::from_string(command.to_owned()).expect("raw");
            let request = WireRequestOwned {
                id: None,
                branch: None,
                space: None,
                command: &raw,
            };
            self.send(&serde_json::to_vec(&request).expect("serialize"))
        }

        /// A protocol-revision-2 request carrying a correlation id.
        fn send_correlated(&mut self, id: u64, command: &str) -> serde_json::Value {
            let raw = serde_json::value::RawValue::from_string(command.to_owned()).expect("raw");
            let request = WireRequestOwned {
                id: Some(id),
                branch: None,
                space: None,
                command: &raw,
            };
            self.send(&serde_json::to_vec(&request).expect("serialize"))
        }
    }

    #[test]
    fn a_hello_first_frame_negotiates_protocol_2_and_commands_follow() {
        let dir = tempfile::tempdir().expect("tmp");
        let executor = Arc::new(Mutex::new(Executor::open_cache().expect("cache")));
        let mut server = IpcServer::start(dir.path(), executor).expect("start");
        let mut conn = RawConn::connect(server.socket_path());

        let hello = conn.send(b"{\"hello\":{\"protocol\":2}}");
        assert_eq!(hello["type"], "ipc_hello", "hello answered: {hello}");
        assert_eq!(hello["data"]["protocol"], 2);
        assert_eq!(hello["data"]["release"], env!("CARGO_PKG_VERSION"));
        assert_eq!(hello["data"]["idl"]["schema_version"], "strata.idl.v1");
        assert_eq!(
            hello["data"]["idl"]["generator_version"],
            "strata-executor-idl.1"
        );
        assert_eq!(hello["data"]["granted_access"], "read_write");
        assert_eq!(
            hello["data"]["owner_pid"],
            u64::from(std::process::id()),
            "the owner reports its own pid"
        );
        assert_eq!(
            hello["data"]["capabilities"],
            serde_json::json!([]),
            "nothing granted from an empty want-list"
        );

        // The same connection then serves commands, correlated: the response
        // is an {"id","payload"} frame echoing the request id, with the
        // classic executor envelope untouched inside.
        let pong = conn.send_correlated(1, "{\"type\":\"ping\"}");
        assert_eq!(pong["id"], 1, "the response echoes the request id");
        assert_eq!(
            pong["payload"]["type"], "pong",
            "the payload is the classic envelope: {pong}"
        );

        server.shutdown();
    }

    #[test]
    fn an_unsupported_protocol_revision_is_refused_and_the_connection_closes() {
        let dir = tempfile::tempdir().expect("tmp");
        let executor = Arc::new(Mutex::new(Executor::open_cache().expect("cache")));
        let mut server = IpcServer::start(dir.path(), executor).expect("start");
        let mut conn = RawConn::connect(server.socket_path());

        let refusal = conn.send(b"{\"hello\":{\"protocol\":99}}");
        assert_eq!(
            refusal["error"]["code"], "invalid_argument.executor.ipc_hello",
            "refused with the registered hello code: {refusal}"
        );
        assert_eq!(refusal["error"]["class"], "invalid_argument");

        // The refusal closes the connection: a further exchange fails — the
        // write may already see a broken pipe, or the read reaches EOF —
        // rather than hanging or being served.
        let followup = wire::write_frame(&mut conn.writer, b"{\"command\":{\"type\":\"ping\"}}")
            .and_then(|()| wire::read_frame(&mut conn.reader));
        assert!(
            followup.is_err(),
            "a refused connection serves nothing further"
        );

        server.shutdown();
    }

    #[test]
    fn a_malformed_hello_is_refused_not_guessed() {
        // deny_unknown_fields is the hello's evolution contract: a field this
        // owner does not know cannot be silently absorbed, because the client
        // chose protocol semantics the owner would then be faking.
        let dir = tempfile::tempdir().expect("tmp");
        let executor = Arc::new(Mutex::new(Executor::open_cache().expect("cache")));
        let mut server = IpcServer::start(dir.path(), executor).expect("start");
        let mut conn = RawConn::connect(server.socket_path());

        let refusal = conn.send(b"{\"hello\":{\"protocol\":2,\"surprise\":true}}");
        assert_eq!(
            refusal["error"]["code"],
            "invalid_argument.executor.ipc_hello"
        );

        server.shutdown();
    }

    #[test]
    fn unsupported_capabilities_are_ignored_not_granted_and_not_errors() {
        let dir = tempfile::tempdir().expect("tmp");
        let executor = Arc::new(Mutex::new(Executor::open_cache().expect("cache")));
        let mut server = IpcServer::start(dir.path(), executor).expect("start");
        let mut conn = RawConn::connect(server.socket_path());

        let hello = conn
            .send(b"{\"hello\":{\"protocol\":2,\"capabilities\":[\"notify.version\",\"bogus\"]}}");
        assert_eq!(hello["type"], "ipc_hello", "probing is not an error");
        assert_eq!(
            hello["data"]["capabilities"],
            serde_json::json!([]),
            "nothing is granted until a capability is supported"
        );

        server.shutdown();
    }

    #[test]
    fn a_read_access_declaration_is_echoed_but_unenforced_in_this_revision() {
        // Pins the slice boundary explicitly: the hello CARRIES the access
        // declaration; REJECTING writes on a read session is the follow-up
        // enforcement slice, which will flip the second half of this test.
        let dir = tempfile::tempdir().expect("tmp");
        let executor = Arc::new(Mutex::new(Executor::open_cache().expect("cache")));
        let mut server = IpcServer::start(dir.path(), executor).expect("start");
        let mut conn = RawConn::connect(server.socket_path());

        let hello = conn.send(b"{\"hello\":{\"protocol\":2,\"access\":\"read\"}}");
        assert_eq!(hello["data"]["granted_access"], "read");

        let put = conn.send_correlated(
            1,
            "{\"type\":\"kv_put\",\"key\":\"aGk=\",\"value\":\"dg==\"}",
        );
        assert_eq!(
            put["payload"]["type"], "write_result",
            "declarative only: a write still succeeds until enforcement lands"
        );

        server.shutdown();
    }

    #[test]
    fn a_correlated_request_without_an_id_is_refused_with_a_null_id_frame() {
        // Protocol 2 requires the id: without one the client cannot correlate
        // the answer, so the refusal itself says which request it refuses the
        // only way it can — id null, in a frame the client is parsing.
        let dir = tempfile::tempdir().expect("tmp");
        let executor = Arc::new(Mutex::new(Executor::open_cache().expect("cache")));
        let mut server = IpcServer::start(dir.path(), executor).expect("start");
        let mut conn = RawConn::connect(server.socket_path());

        assert_eq!(
            conn.send(b"{\"hello\":{\"protocol\":2}}")["type"],
            "ipc_hello"
        );
        let refused = conn.send_command("{\"type\":\"ping\"}");
        assert_eq!(refused["id"], serde_json::Value::Null);
        assert_eq!(
            refused["payload"]["error"]["code"],
            "invalid_argument.executor.wire_request"
        );
        assert_eq!(
            conn.send_correlated(1, "{\"type\":\"ping\"}")["payload"]["type"],
            "pong",
            "the connection survives the refusal"
        );

        server.shutdown();
    }

    #[test]
    fn pipelined_requests_are_answered_in_order_with_their_own_ids() {
        // The protocol permits writing ahead: two frames sent back-to-back are
        // answered in request order, each response carrying its request's id.
        // (One-in-flight is client discipline, not a server rule.)
        let dir = tempfile::tempdir().expect("tmp");
        let executor = Arc::new(Mutex::new(Executor::open_cache().expect("cache")));
        let mut server = IpcServer::start(dir.path(), executor).expect("start");
        let mut conn = RawConn::connect(server.socket_path());
        assert_eq!(
            conn.send(b"{\"hello\":{\"protocol\":2}}")["type"],
            "ipc_hello"
        );

        let first = frame_for(7, "{\"type\":\"ping\"}");
        let second = frame_for(8, "{\"type\":\"kv_count\"}");
        wire::write_frame(&mut conn.writer, &first).expect("write first");
        wire::write_frame(&mut conn.writer, &second).expect("write second");

        let responses: Vec<serde_json::Value> = (0..2)
            .map(|_| {
                let bytes = wire::read_frame(&mut conn.reader).expect("read");
                serde_json::from_slice(&bytes).expect("decode")
            })
            .collect();
        assert_eq!(responses[0]["id"], 7, "first in, first answered");
        assert_eq!(responses[0]["payload"]["type"], "pong");
        assert_eq!(responses[1]["id"], 8);
        assert_eq!(responses[1]["payload"]["type"], "uint");

        server.shutdown();
    }

    /// Serialize one correlated request frame without sending it.
    fn frame_for(id: u64, command: &str) -> Vec<u8> {
        let raw = serde_json::value::RawValue::from_string(command.to_owned()).expect("raw");
        serde_json::to_vec(&WireRequestOwned {
            id: Some(id),
            branch: None,
            space: None,
            command: &raw,
        })
        .expect("serialize")
    }

    #[test]
    fn a_stray_id_on_a_legacy_connection_is_ignored_not_echoed() {
        // Pins protocol-1 semantics exactly: no hello means no response frame,
        // even when the request smuggles an id — a legacy connection's reply
        // is the bare envelope, byte-compatible with every pre-hello client.
        let dir = tempfile::tempdir().expect("tmp");
        let executor = Arc::new(Mutex::new(Executor::open_cache().expect("cache")));
        let mut server = IpcServer::start(dir.path(), executor).expect("start");
        let mut conn = RawConn::connect(server.socket_path());

        let response = conn.send_correlated(42, "{\"type\":\"ping\"}");
        assert_eq!(response["type"], "pong", "bare envelope: {response}");
        assert!(
            response.get("id").is_none(),
            "no frame wrapper on protocol 1"
        );

        server.shutdown();
    }

    #[test]
    fn a_hello_after_the_first_frame_is_a_malformed_request_envelope() {
        // The hello negotiates a connection, not a request — mid-connection it
        // is just a frame with no `command`, and the connection survives it.
        let dir = tempfile::tempdir().expect("tmp");
        let executor = Arc::new(Mutex::new(Executor::open_cache().expect("cache")));
        let mut server = IpcServer::start(dir.path(), executor).expect("start");
        let mut conn = RawConn::connect(server.socket_path());

        assert_eq!(conn.send_command("{\"type\":\"ping\"}")["type"], "pong");
        let late_hello = conn.send(b"{\"hello\":{\"protocol\":2}}");
        assert_eq!(
            late_hello["error"]["code"],
            "invalid_argument.executor.wire_request"
        );
        assert_eq!(
            conn.send_command("{\"type\":\"ping\"}")["type"],
            "pong",
            "the connection continues after the stray hello"
        );

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
