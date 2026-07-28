//! Live state of a hosted IPC socket, as seen by the `ipc_status` command.
//!
//! When a `Connection` opens a durable store as a host (see [`crate::IpcMode`])
//! it starts an `IpcServer` and injects an [`IpcHostState`] into the underlying
//! [`crate::Executor`], so the executor's `ipc_status` handler can report the
//! socket path, the owner pid, and the live client count uniformly — for an
//! in-process caller and for a remote client whose request is forwarded here
//! over the wire. A non-hosting executor (a client, a cache, or an
//! `IpcMode::Off` open) holds none, which the handler reports as `hosting:
//! false`.
//!
//! This lives in the always-compiled part of the crate (not the unix-only `ipc`
//! module) because the executor field that holds it is always compiled; the
//! unix transport is what populates it.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

/// A handle onto a running IPC host's reportable state.
///
/// `socket_path` and `owner_pid` are fixed at host start; `client_count` is a
/// live counter shared with the server, so a status read always reflects the
/// current number of connected clients. `stop_signal` is the server's shutdown
/// flag — setting it stops the listener (`ipc_stop`).
#[derive(Clone, Debug)]
pub struct IpcHostState {
    socket_path: PathBuf,
    owner_pid: u32,
    client_count: Arc<AtomicUsize>,
    stop_signal: Arc<AtomicBool>,
}

impl IpcHostState {
    /// Build a host-state handle from a server's fixed facts, its live
    /// connection counter, and its shutdown flag.
    #[must_use]
    pub fn new(
        socket_path: PathBuf,
        owner_pid: u32,
        client_count: Arc<AtomicUsize>,
        stop_signal: Arc<AtomicBool>,
    ) -> Self {
        Self {
            socket_path,
            owner_pid,
            client_count,
            stop_signal,
        }
    }

    /// Signal the hosting server to stop accepting connections (`ipc_stop`).
    /// The listener exits on its next poll; the socket files are unlinked when
    /// the owning `Connection` is later closed or dropped.
    pub fn request_stop(&self) {
        self.stop_signal.store(true, Ordering::SeqCst);
    }

    /// The socket this owner is hosting on.
    #[must_use]
    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    /// The hosting owner's process id.
    #[must_use]
    pub const fn owner_pid(&self) -> u32 {
        self.owner_pid
    }

    /// The number of clients currently connected to the host.
    #[must_use]
    pub fn client_count(&self) -> u64 {
        u64::try_from(self.client_count.load(Ordering::Relaxed)).unwrap_or(u64::MAX)
    }
}
