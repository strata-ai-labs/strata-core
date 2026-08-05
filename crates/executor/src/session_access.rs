//! The session-access vocabulary shared by the IPC hello and the `ipc_status`
//! report.
//!
//! This lives in the always-compiled part of the crate (like
//! [`crate::ipc_mode`]) rather than the unix-only `ipc` module, because the
//! `ipc_status` wire DTO reports each connected client's access on every
//! platform; the unix transport is what negotiates and enforces it.

use serde::{Deserialize, Serialize};

/// The access a session declares at hello. The owner's dispatch gate rejects
/// write-classified commands on a `Read` session; see the IPC evolution
/// design (`docs/architecture/ipc/ipc-evolution-design.md` §4.2).
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "idl-tooling", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum SessionAccess {
    /// The session intends read-class commands only.
    Read,
    /// Full command access — the pre-hello default.
    #[default]
    ReadWrite,
}
