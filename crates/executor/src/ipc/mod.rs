//! Transparent multi-process IPC over a Unix domain socket.
//!
//! The storage engine admits exactly one read-write process per store
//! (the first opener takes the `locks/writer` flock; every other opener is
//! refused with `unavailable.engine.persistence`). This module lets a second
//! process transparently become a client of the first instead of failing:
//!
//! - the first opener of a durable store OWNS it and may host an [`IpcServer`]
//!   on `<data_dir>/strata.sock`;
//! - a contending opener connects an [`IpcClient`] to that socket and speaks
//!   the SAME executor wire JSON every frontend already uses (`{"type":…}`
//!   requests, `{"type","data"}` / `{"error":…}` responses), so a socket
//!   client is wire-identical to an in-process [`crate::Executor`].
//!
//! One engine process per store, ever; multi-process access is purely a
//! transport concern, which is why it lives in the executor (the transport
//! adapter) and the engine needs no concurrency changes.
//!
//! Positioning: this is an embedded-database IPC mechanism, not a network
//! server — the socket is `0600`, same-unix-user only.

mod client;
mod connection;
mod dispatch;
mod protocol;
mod server;
mod wire;

pub use connection::Connection;
pub use server::IpcServer;

use std::path::{Path, PathBuf};

/// The owner's socket, in the expected location when the path fits an
/// `AF_UNIX` `sun_path` (~108 bytes).
const SOCKET_NAME: &str = "strata.sock";
/// Written in the data dir only when the socket itself had to move to the
/// runtime dir (deep data-dir paths); points a client at the real socket.
const POINTER_NAME: &str = "strata.sock.path";
/// The owner's pid, for `strata stop` and stale-owner liveness checks.
const PID_NAME: &str = "strata.pid";
/// Conservative cap: real `sun_path` is 108 bytes incl. NUL; stay well under.
const MAX_SUN_PATH: usize = 100;

/// Where a hosting owner binds its socket, and the pointer file to write when
/// the socket had to move out of the data dir.
struct SocketBinding {
    socket: PathBuf,
    /// `Some(pointer_path)` when `socket` is NOT `<data_dir>/strata.sock` — a
    /// client that can't reconstruct the runtime path reads this instead.
    pointer: Option<PathBuf>,
}

/// Resolve where an owner should bind. Prefer `<data_dir>/strata.sock`; if that
/// path overflows `sun_path`, fall back to a per-user runtime-dir socket named
/// by a hash of the data dir, and record a pointer file in the data dir.
fn resolve_binding(data_dir: &Path) -> SocketBinding {
    let direct = data_dir.join(SOCKET_NAME);
    if fits_sun_path(&direct) {
        return SocketBinding {
            socket: direct,
            pointer: None,
        };
    }
    SocketBinding {
        socket: runtime_socket(data_dir),
        pointer: Some(data_dir.join(POINTER_NAME)),
    }
}

/// Resolve where a client should connect for `data_dir`, or `None` if no owner
/// socket is discoverable. Checks the expected location first, then the pointer
/// file left by a runtime-dir fallback. Connectability is the caller's concern
/// (a stale path connects-then-fails cleanly).
fn resolve_connect(data_dir: &Path) -> Option<PathBuf> {
    let direct = data_dir.join(SOCKET_NAME);
    if direct.exists() {
        return Some(direct);
    }
    let pointer = data_dir.join(POINTER_NAME);
    std::fs::read_to_string(&pointer)
        .ok()
        .map(|contents| PathBuf::from(contents.trim()))
        .filter(|path| path.exists())
}

fn pid_path(data_dir: &Path) -> PathBuf {
    data_dir.join(PID_NAME)
}

fn fits_sun_path(path: &Path) -> bool {
    path.as_os_str().len() < MAX_SUN_PATH
}

/// `$XDG_RUNTIME_DIR/strata/<hash>.sock`, or `/tmp/strata-<uid>/<hash>.sock`
/// when the runtime dir is unset. The hash of the (canonicalized) data dir
/// keeps distinct stores from colliding.
fn runtime_socket(data_dir: &Path) -> PathBuf {
    let base = std::env::var_os("XDG_RUNTIME_DIR").map_or_else(
        || {
            // `$XDG_RUNTIME_DIR` is the per-user path; this temp fallback only
            // runs when it is unset. Discriminate by `$USER` (no unsafe getuid)
            // so distinct users on a shared `/tmp` do not collide.
            let user = std::env::var("USER").unwrap_or_else(|_| "default".to_owned());
            std::env::temp_dir().join(format!("strata-{user}"))
        },
        PathBuf::from,
    );
    let canonical = std::fs::canonicalize(data_dir).unwrap_or_else(|_| data_dir.to_path_buf());
    let hash = fnv1a(canonical.as_os_str().as_encoded_bytes());
    base.join("strata").join(format!("{hash:016x}.sock"))
}

fn fnv1a(bytes: &[u8]) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = OFFSET;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}

#[cfg(test)]
mod path_tests {
    use super::*;

    #[test]
    fn short_data_dir_binds_in_place_without_a_pointer() {
        let dir = std::path::Path::new("/tmp/db");
        let binding = resolve_binding(dir);
        assert_eq!(binding.socket, dir.join("strata.sock"));
        assert!(binding.pointer.is_none());
    }

    #[test]
    fn overlong_data_dir_moves_to_runtime_and_records_a_pointer() {
        let deep = std::path::PathBuf::from(format!("/tmp/{}", "d".repeat(120)));
        let binding = resolve_binding(&deep);
        assert_ne!(binding.socket, deep.join("strata.sock"));
        assert_eq!(binding.pointer, Some(deep.join("strata.sock.path")));
        assert!(
            fits_sun_path(&binding.socket),
            "runtime socket must itself fit sun_path"
        );
    }

    #[test]
    fn connect_finds_the_in_place_socket() {
        let tmp = tempfile::tempdir().expect("tmp");
        assert!(resolve_connect(tmp.path()).is_none(), "no socket yet");
        std::fs::write(tmp.path().join("strata.sock"), b"").expect("touch");
        assert_eq!(
            resolve_connect(tmp.path()),
            Some(tmp.path().join("strata.sock"))
        );
    }

    #[test]
    fn distinct_data_dirs_get_distinct_runtime_sockets() {
        // The hash must discriminate stores — a constant would collide two
        // different data dirs onto one socket.
        let a = runtime_socket(std::path::Path::new("/tmp/store-a"));
        let b = runtime_socket(std::path::Path::new("/tmp/store-b"));
        assert_ne!(a, b, "different stores must not share a runtime socket");
    }

    #[test]
    fn fnv1a_matches_the_reference_hash() {
        // Golden FNV-1a values pin the exact mix (an OR-based or otherwise
        // altered accumulator would diverge here).
        assert_eq!(fnv1a(b""), 0xcbf2_9ce4_8422_2325, "offset basis");
        assert_eq!(fnv1a(b"x"), 0xaf63_f54c_8602_1707);
        assert_eq!(fnv1a(b"strata"), 0x2b58_aad8_d3bb_901c);
        assert_ne!(fnv1a(b"ab"), fnv1a(b"ba"), "order-sensitive");
    }

    #[test]
    fn sun_path_fit_boundary() {
        let just_fits = std::path::PathBuf::from("x".repeat(MAX_SUN_PATH - 1));
        let too_long = std::path::PathBuf::from("x".repeat(MAX_SUN_PATH));
        assert!(fits_sun_path(&just_fits));
        assert!(!fits_sun_path(&too_long));
    }

    #[test]
    fn connect_follows_the_pointer_file() {
        let tmp = tempfile::tempdir().expect("tmp");
        let real = tmp.path().join("elsewhere.sock");
        std::fs::write(&real, b"").expect("touch real");
        std::fs::write(
            tmp.path().join("strata.sock.path"),
            real.to_string_lossy().as_bytes(),
        )
        .expect("write pointer");
        assert_eq!(resolve_connect(tmp.path()), Some(real));
    }
}
