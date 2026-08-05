//! IPC transport envelope — the framed payload that wraps an executor wire
//! command with the connection's session scope, plus the protocol-revision-2
//! hello exchanged as a connection's first frame.
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
//!
//! The hello (design: `docs/architecture/ipc/ipc-evolution-design.md` §4.1) is
//! how a connection declares its protocol revision, IDL stamps, identity, and
//! intended access before its first command — the negotiation point every
//! later protocol feature (read-only enforcement, notifications, cancel) rides
//! on. A connection whose first frame is a bare [`WireRequest`] speaks the
//! implicit protocol revision 1.

use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

/// A framed request: the connection's session scope plus the wire command.
#[derive(Debug, Deserialize)]
pub(crate) struct WireRequest<'a> {
    /// Correlation id, echoed on the response frame. Required on a
    /// protocol-revision-2 connection (negotiated by hello); ignored on the
    /// implicit protocol 1, whose responses carry no frame to echo it in.
    #[serde(default)]
    pub(crate) id: Option<u64>,
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
    pub(crate) id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) branch: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) space: Option<&'a str>,
    pub(crate) command: &'a RawValue,
}

/// A protocol-revision-2 response frame: the request's correlation id plus
/// the untouched executor response envelope. The wrapper is transport-only —
/// `payload` is byte-identical to what a local execution or a protocol-1
/// response carries, so correlation never forks the executor wire format.
/// `id` is `null` only when the request's own id could not be read (the
/// malformed-envelope case).
#[derive(Debug, Serialize)]
pub(crate) struct WireResponseFrame<'a> {
    pub(crate) id: Option<u64>,
    pub(crate) payload: &'a RawValue,
}

/// The client-side reader for a protocol-revision-2 response frame.
#[derive(Debug, Deserialize)]
pub(crate) struct WireResponseFrameRef<'a> {
    #[serde(default)]
    pub(crate) id: Option<u64>,
    #[serde(borrow)]
    pub(crate) payload: &'a RawValue,
}

/// The protocol revision a hello negotiates. Connections that never send a
/// hello speak the implicit revision 1 (bare [`WireRequest`] frames, full
/// access, no identity).
pub(crate) const PROTOCOL_VERSION: u32 = 2;

/// The version-tick notification capability: a subscribed connection receives
/// coalesced, lossy `{"notify":{"event":"version",…}}` pushes when the
/// store-state watermark advances.
pub(crate) const CAPABILITY_NOTIFY_VERSION: &str = "notify.version";

/// Capability names this owner supports. A hello's grant is the intersection
/// of the client's want-list with this set — unknown names are ignored, never
/// errors, so a newer client can probe an older owner. The grant is
/// *discovery*, not authorization: a supported capability's frames are served
/// on any protocol-revision-2 connection.
pub(crate) const SUPPORTED_CAPABILITIES: &[&str] = &[CAPABILITY_NOTIFY_VERSION];

/// A hello first frame: `{"hello": {…}}`. Distinguished from a legacy
/// [`WireRequest`] first frame by its single `hello` intent key.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct HelloFrame {
    pub(crate) hello: HelloRequest,
}

/// The client's half of the hello exchange.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct HelloRequest {
    /// The protocol revision the client speaks. The owner refuses revisions it
    /// does not support rather than guessing.
    pub(crate) protocol: u32,
    /// The IDL stamps the client was built or generated against, for skew
    /// diagnostics on the owner side.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) idl: Option<IdlStamps>,
    /// Who is connecting, for status surfaces.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) client: Option<ClientIdentity>,
    /// The access this session intends. Declarative until server-side
    /// read-only enforcement lands; the owner echoes it back as granted.
    #[serde(default)]
    pub(crate) access: SessionAccess,
    /// Capability want-list; the owner grants the supported intersection.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) capabilities: Vec<String>,
}

pub use crate::session_access::SessionAccess;

/// The IDL version stamps a party was built against (the same pair the
/// generated command index carries).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IdlStamps {
    /// The authored-IDL schema version (`strata.idl.v1`).
    pub schema_version: String,
    /// The generator version (`strata-executor-idl.1`).
    pub generator_version: String,
}

/// Who is connecting — a display identity for status surfaces, not
/// authentication (the `0600` socket's same-user check is the security
/// boundary).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ClientIdentity {
    /// Short product name, e.g. `strata-cli` or `strata-vscode`.
    pub name: String,
    /// The client's own version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// The client's process id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
}

/// The owner's half of the hello exchange — what a connected client learns
/// about the process it attached to. Carried as the `data` of an
/// `{"type":"ipc_hello"}` response envelope.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ServerHello {
    /// The protocol revision the owner is speaking on this connection.
    pub protocol: u32,
    /// The owner's release version.
    pub release: String,
    /// The IDL stamps the owner was built against.
    pub idl: IdlStamps,
    /// The access the owner granted this session.
    pub granted_access: SessionAccess,
    /// The granted capability set (the supported subset of the want-list).
    pub capabilities: Vec<String>,
    /// The owner's process id.
    pub owner_pid: u32,
}

/// The IDL stamps this build carries. Sourced from the CLI catalog's
/// supported-source constants so the hello and the embedded command index can
/// never disagree.
pub(crate) fn build_idl_stamps() -> IdlStamps {
    IdlStamps {
        schema_version: crate::cli_metadata::SUPPORTED_SOURCE_SCHEMA_VERSION.to_owned(),
        generator_version: crate::cli_metadata::SUPPORTED_SOURCE_GENERATOR_VERSION.to_owned(),
    }
}

/// Whether a first frame is a hello (vs a legacy [`WireRequest`]). This
/// answers "which shape is this?", never "is it valid?" — a frame carrying a
/// `hello` key routes to strict hello parsing, whose errors are the real ones.
pub(crate) fn frame_is_hello(frame: &[u8]) -> bool {
    #[derive(Deserialize)]
    struct Sniff {
        hello: Option<serde::de::IgnoredAny>,
    }
    serde_json::from_slice::<Sniff>(frame)
        .map(|sniff| sniff.hello.is_some())
        .unwrap_or(false)
}

/// A subscription frame on a protocol-revision-2 connection:
/// `{"id": n, "subscribe": {"events": ["version"]}}`. Acked with an
/// `ipc_subscribed` envelope carrying the accepted event set (the supported
/// intersection — unknown names are ignored, mirroring capability grants).
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SubscribeFrame {
    #[serde(default)]
    pub(crate) id: Option<u64>,
    pub(crate) subscribe: SubscribeRequest,
}

/// The subscription want-list.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SubscribeRequest {
    #[serde(default)]
    pub(crate) events: Vec<String>,
}

/// The event name a version-tick subscription asks for.
pub(crate) const EVENT_VERSION: &str = "version";

/// Whether a frame is a subscription (vs a request). Same shape-only contract
/// as [`frame_is_hello`].
pub(crate) fn frame_is_subscribe(frame: &[u8]) -> bool {
    #[derive(Deserialize)]
    struct Sniff {
        subscribe: Option<serde::de::IgnoredAny>,
    }
    serde_json::from_slice::<Sniff>(frame)
        .map(|sniff| sniff.subscribe.is_some())
        .unwrap_or(false)
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
            id: Some(7),
            branch: Some("feature"),
            space: None,
            command: &command,
        };
        let bytes = serde_json::to_vec(&owned).expect("serialize request");

        let decoded: WireRequest = serde_json::from_slice(&bytes).expect("decode request");
        assert_eq!(decoded.id, Some(7));
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
        assert_eq!(decoded.id, None, "protocol-1 requests carry no id");
        assert_eq!(decoded.branch, None);
        assert_eq!(decoded.space, None);
        assert_eq!(decoded.command.get(), "{\"type\":\"ping\"}");
    }

    #[test]
    fn a_response_frame_round_trips_id_and_raw_payload() {
        let payload = serde_json::value::RawValue::from_string(
            "{\"type\":\"pong\",\"data\":{\"version\":\"1.0.0\"}}".to_owned(),
        )
        .expect("raw payload");
        let frame = super::WireResponseFrame {
            id: Some(9),
            payload: &payload,
        };
        let bytes = serde_json::to_vec(&frame).expect("serialize frame");
        let decoded: super::WireResponseFrameRef =
            serde_json::from_slice(&bytes).expect("decode frame");
        assert_eq!(decoded.id, Some(9));
        assert_eq!(
            decoded.payload.get(),
            payload.get(),
            "the payload passes through byte-identical"
        );
    }

    #[test]
    fn the_sniff_routes_by_frame_shape_only() {
        use super::frame_is_hello;
        // A hello frame routes to hello parsing even when invalid — validity
        // is the strict parser's job, not the sniff's.
        assert!(frame_is_hello(b"{\"hello\":{\"protocol\":2}}"));
        assert!(frame_is_hello(b"{\"hello\":\"garbage\"}"));
        // A request envelope, non-envelope JSON, and non-JSON are not hellos.
        assert!(!frame_is_hello(
            b"{\"branch\":\"b\",\"command\":{\"type\":\"ping\"}}"
        ));
        assert!(!frame_is_hello(b"[1,2,3]"));
        assert!(!frame_is_hello(b"not json"));
    }

    #[test]
    fn a_minimal_hello_defaults_to_read_write_with_no_extras() {
        let decoded: super::HelloFrame =
            serde_json::from_str("{\"hello\":{\"protocol\":2}}").expect("decode");
        assert_eq!(decoded.hello.protocol, 2);
        assert_eq!(decoded.hello.access, super::SessionAccess::ReadWrite);
        assert!(decoded.hello.idl.is_none());
        assert!(decoded.hello.client.is_none());
        assert!(decoded.hello.capabilities.is_empty());
    }

    #[test]
    fn a_full_hello_round_trips() {
        let request = super::HelloRequest {
            protocol: super::PROTOCOL_VERSION,
            idl: Some(super::build_idl_stamps()),
            client: Some(super::ClientIdentity {
                name: "strata-vscode".to_owned(),
                version: Some("0.1.0".to_owned()),
                pid: Some(4242),
            }),
            access: super::SessionAccess::Read,
            capabilities: vec!["notify.version".to_owned()],
        };
        let bytes = serde_json::to_vec(&super::HelloFrame { hello: request }).expect("serialize");
        let decoded: super::HelloFrame = serde_json::from_slice(&bytes).expect("decode");
        assert_eq!(decoded.hello.access, super::SessionAccess::Read);
        assert_eq!(
            decoded.hello.client.expect("identity").name,
            "strata-vscode"
        );
        assert_eq!(
            decoded.hello.idl.expect("stamps").schema_version,
            "strata.idl.v1"
        );
        assert_eq!(decoded.hello.capabilities, ["notify.version"]);
    }

    #[test]
    fn hello_evolution_is_by_known_fields_only() {
        // Workspace wire policy: deny_unknown_fields everywhere; new hello
        // fields must be added as optional-with-default, never absorbed
        // silently. An unknown key is a strict-parse error.
        let unknown_in_hello: Result<super::HelloFrame, _> =
            serde_json::from_str("{\"hello\":{\"protocol\":2,\"surprise\":true}}");
        assert!(unknown_in_hello.is_err(), "unknown hello field rejected");
        let unknown_beside_hello: Result<super::HelloFrame, _> =
            serde_json::from_str("{\"hello\":{\"protocol\":2},\"surprise\":true}");
        assert!(unknown_beside_hello.is_err(), "unknown frame key rejected");
    }

    #[test]
    fn the_subscribe_sniff_routes_by_shape_and_the_frame_round_trips() {
        use super::frame_is_subscribe;
        assert!(frame_is_subscribe(
            b"{\"id\":1,\"subscribe\":{\"events\":[\"version\"]}}"
        ));
        assert!(!frame_is_subscribe(b"{\"command\":{\"type\":\"ping\"}}"));
        assert!(!frame_is_subscribe(b"{\"hello\":{\"protocol\":2}}"));

        let decoded: super::SubscribeFrame =
            serde_json::from_str("{\"id\":3,\"subscribe\":{\"events\":[\"version\"]}}")
                .expect("decode");
        assert_eq!(decoded.id, Some(3));
        assert_eq!(decoded.subscribe.events, ["version"]);
        let unknown: Result<super::SubscribeFrame, _> =
            serde_json::from_str("{\"id\":3,\"subscribe\":{\"events\":[],\"surprise\":1}}");
        assert!(unknown.is_err(), "subscribe evolves by known fields only");
    }

    #[test]
    fn session_access_wire_words_are_read_and_read_write() {
        // The wire vocabulary is part of the protocol contract; a rename would
        // silently break every out-of-repo client.
        let read: super::SessionAccess = serde_json::from_str("\"read\"").expect("read");
        assert_eq!(read, super::SessionAccess::Read);
        let write: super::SessionAccess =
            serde_json::from_str("\"read_write\"").expect("read_write");
        assert_eq!(write, super::SessionAccess::ReadWrite);
        assert_eq!(
            serde_json::to_string(&super::SessionAccess::Read).expect("ser"),
            "\"read\""
        );
    }
}
