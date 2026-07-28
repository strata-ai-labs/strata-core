//! The socket server's per-request boundary: wire JSON in, wire JSON out.
//!
//! Identical decode→dispatch→encode semantics to the wasm binding
//! (`crates/wasm/src/lib.rs`) and the MCP `strata_command` tool
//! (`crates/cli/src/mcp.rs`) — with one difference: guard and decode failures
//! also become error envelopes rather than propagating, because a socket
//! server must always reply to a request it has read.

use serde_json::json;

use crate::{guard_json_integers, Command, Executor, ExecutorError, ExecutorResult, Output};

/// Execute one wire-JSON request against `executor`, returning the wire-JSON
/// response envelope: `{"type":<name>,"data":<payload>}` on success,
/// `{"error":<status>}` on any failure. Never propagates.
pub(crate) fn execute_wire_request(executor: &mut Executor, request_json: &str) -> String {
    let envelope = match decode_and_execute(executor, request_json) {
        Ok(output) => serde_json::to_value(&output),
        Err(error) => serde_json::to_value(json!({ "error": error })),
    };
    envelope
        .and_then(|value| serde_json::to_string(&value))
        .unwrap_or_else(|error| serialize_failure_envelope(&error))
}

/// Ingress guard + typed decode + dispatch. The guard runs first (matching
/// every other wire binding); a malformed command becomes a registered
/// `invalid_argument.executor.wire_request`, carrying serde's own field-level
/// teaching message.
fn decode_and_execute(executor: &mut Executor, request_json: &str) -> ExecutorResult<Output> {
    guard_json_integers(request_json)?;
    let command: Command = serde_json::from_str(request_json).map_err(|error| {
        ExecutorError::invalid_input(
            "invalid_argument.executor.wire_request",
            format!("malformed command request: {error}"),
        )
    })?;
    executor.execute(command)
}

/// Last-resort envelope for the (practically unreachable) case where a typed
/// `Output`/`ErrorStatus` fails to serialize. Hand-built so the client still
/// receives a well-formed `{"error":…}` frame with a registered code.
fn serialize_failure_envelope(error: &serde_json::Error) -> String {
    let status = ExecutorError::new(
        crate::error::ExecutorErrorClass::Internal,
        "internal.executor.wire_response",
        false,
        format!("wire response serialization failed: {error}"),
    );
    serde_json::to_string(&json!({ "error": status })).unwrap_or_else(|_| {
        // Both serializations failing is not reachable with plain-data types;
        // emit a minimal static envelope rather than panic inside a handler.
        "{\"error\":{\"class\":\"internal\",\"code\":\"internal.executor.wire_response\",\
         \"message\":\"wire response serialization failed\"}}"
            .to_owned()
    })
}

#[cfg(test)]
mod tests {
    use super::execute_wire_request;
    use crate::Executor;
    use serde_json::Value;

    fn envelope(executor: &mut Executor, request: &str) -> Value {
        serde_json::from_str(&execute_wire_request(executor, request)).expect("valid wire response")
    }

    #[test]
    fn success_returns_type_data_envelope() {
        let mut executor = Executor::open_cache().expect("cache executor");
        let response = envelope(&mut executor, "{\"type\":\"ping\"}");
        assert_eq!(response["type"], "pong");
        assert!(response["data"]["version"].is_string());
    }

    #[test]
    fn malformed_request_returns_wire_request_error() {
        let mut executor = Executor::open_cache().expect("cache executor");
        let response = envelope(&mut executor, "{\"type\":\"not_a_command\"}");
        assert_eq!(
            response["error"]["code"],
            "invalid_argument.executor.wire_request"
        );
        assert_eq!(response["error"]["class"], "invalid_argument");
    }

    #[test]
    fn non_json_request_returns_wire_request_error() {
        let mut executor = Executor::open_cache().expect("cache executor");
        let response = envelope(&mut executor, "this is not json");
        assert_eq!(
            response["error"]["code"],
            "invalid_argument.executor.wire_request"
        );
    }

    #[test]
    fn lossy_integer_request_is_caught_by_the_ingress_guard() {
        let mut executor = Executor::open_cache().expect("cache executor");
        // An integer past u64 that serde_json would coerce to a lossy f64.
        let response = envelope(
            &mut executor,
            "{\"type\":\"kv_count\",\"extra\":999999999999999999999}",
        );
        assert_eq!(
            response["error"]["code"],
            "invalid_argument.executor.json_number"
        );
    }

    #[test]
    fn executor_error_surfaces_as_error_envelope() {
        let mut executor = Executor::open_cache().expect("cache executor");
        // kv_get on a missing key is a structured executor result, not a crash;
        // an operation against a nonexistent branch is a clean error envelope.
        let response = envelope(
            &mut executor,
            "{\"type\":\"kv_get\",\"branch\":\"does-not-exist\",\"key\":\"aGk=\"}",
        );
        assert!(
            response.get("error").is_some(),
            "expected an error envelope: {response}"
        );
        assert!(response["error"]["code"].is_string());
    }
}
