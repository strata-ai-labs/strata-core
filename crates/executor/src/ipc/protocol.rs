//! IPC transport envelope — the framed payload that wraps an executor wire
//! command with the connection's session scope.
//!
//! The socket carries the executor's own wire JSON, but a command like
//! `kv_get` resolves its branch/space from session defaults the local
//! `Executor` holds — state a remote client cannot set directly. So each
//! request wraps the command with the client's current `branch`/`space`; the
//! server applies them to the shared executor under the lock, immediately
//! before dispatch, and the command's own explicit `branch`/`space` (if any)
//! still override — exact parity with local execution.
//!
//! The inner `command` is kept as a [`RawValue`] so its original bytes reach
//! the lossy-integer ingress guard unchanged (parsing to a `Value` first would
//! let `serde_json` coerce an out-of-range integer to a lossy `f64` before the
//! guard could see it).

use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

/// A framed request: the connection's session scope plus the wire command.
#[derive(Debug, Deserialize)]
pub(crate) struct WireRequest<'a> {
    /// Session default branch for this request (the client's `--branch`),
    /// applied before dispatch. `None` leaves the owner's default in place.
    #[serde(default)]
    pub(crate) branch: Option<String>,
    /// Session default space for this request (the client's `--space`).
    #[serde(default)]
    pub(crate) space: Option<String>,
    /// The executor wire command, raw bytes preserved for the ingress guard.
    #[serde(borrow)]
    pub(crate) command: &'a RawValue,
}

/// The client-side builder for a framed request. Kept separate from
/// [`WireRequest`] because the client owns the command bytes.
#[derive(Debug, Serialize)]
pub(crate) struct WireRequestOwned<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) branch: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) space: Option<&'a str>,
    pub(crate) command: &'a RawValue,
}

#[cfg(test)]
mod tests {
    use super::{WireRequest, WireRequestOwned};

    #[test]
    fn request_round_trips_scope_and_preserves_raw_command() {
        let command = serde_json::value::RawValue::from_string(
            "{\"type\":\"kv_count\",\"n\":999999999999999999999}".to_owned(),
        )
        .expect("raw command");
        let owned = WireRequestOwned {
            branch: Some("feature"),
            space: None,
            command: &command,
        };
        let bytes = serde_json::to_vec(&owned).expect("serialize request");

        let decoded: WireRequest = serde_json::from_slice(&bytes).expect("decode request");
        assert_eq!(decoded.branch.as_deref(), Some("feature"));
        assert_eq!(decoded.space, None);
        // The out-of-range integer is preserved verbatim (not coerced to f64),
        // so the ingress guard downstream still sees it.
        assert!(decoded.command.get().contains("999999999999999999999"));
    }

    #[test]
    fn absent_scope_decodes_to_none() {
        let decoded: WireRequest =
            serde_json::from_str("{\"command\":{\"type\":\"ping\"}}").expect("decode");
        assert_eq!(decoded.branch, None);
        assert_eq!(decoded.space, None);
        assert_eq!(decoded.command.get(), "{\"type\":\"ping\"}");
    }
}
