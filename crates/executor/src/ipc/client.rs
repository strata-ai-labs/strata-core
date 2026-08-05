//! IPC client: speaks the framed wire to a store owner's socket and
//! reconstructs the same `ExecutorResult<Output>` an in-process executor
//! returns, so a remote connection is transport-transparent to callers.

use std::io::{self, BufReader, BufWriter};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

use serde_json::value::RawValue;

use crate::{Command, ErrorStatus, ExecutorError, ExecutorResult, Output};

use super::protocol::{
    self, ClientIdentity, HelloFrame, HelloRequest, ServerHello, SessionAccess, WireRequestOwned,
};
use super::wire;

/// Client read timeout: a command may run arbitrarily long on the owner, so
/// this is generous; it exists so a dead owner does not hang the client
/// forever.
const READ_TIMEOUT: Duration = Duration::from_secs(30);
/// Client write timeout: framing a request should never block for long.
const WRITE_TIMEOUT: Duration = Duration::from_secs(5);

/// A connected IPC client. One outstanding request at a time (synchronous
/// request/response), so framing plus stream ordering is the only correlation
/// needed.
pub(crate) struct IpcClient {
    reader: BufReader<UnixStream>,
    writer: BufWriter<UnixStream>,
    /// The owner's hello, when it speaks protocol revision 2. `None` against a
    /// pre-hello owner (the connection then runs the implicit protocol 1).
    server_hello: Option<ServerHello>,
}

impl IpcClient {
    /// Connect to a store owner listening at `socket_path` and introduce
    /// ourselves with a protocol-revision-2 hello.
    pub(crate) fn connect(socket_path: &Path) -> io::Result<Self> {
        let stream = UnixStream::connect(socket_path)?;
        stream.set_read_timeout(Some(READ_TIMEOUT))?;
        stream.set_write_timeout(Some(WRITE_TIMEOUT))?;
        let mut client = Self {
            reader: BufReader::new(stream.try_clone()?),
            writer: BufWriter::new(stream),
            server_hello: None,
        };
        client.server_hello = client.hello()?;
        Ok(client)
    }

    /// The hello exchange. `Ok(Some)` is a protocol-revision-2 owner;
    /// `Ok(None)` is a pre-hello owner that answered the hello frame with a
    /// malformed-envelope error and kept the connection — we downgrade to the
    /// implicit protocol 1 rather than stranding the user on a skewed pair.
    /// Any other refusal or transport failure fails the connect.
    fn hello(&mut self) -> io::Result<Option<ServerHello>> {
        let request = HelloRequest {
            protocol: protocol::PROTOCOL_VERSION,
            idl: Some(protocol::build_idl_stamps()),
            client: Some(ClientIdentity {
                name: process_name(),
                version: Some(env!("CARGO_PKG_VERSION").to_owned()),
                pid: Some(std::process::id()),
            }),
            access: SessionAccess::ReadWrite,
            capabilities: Vec::new(),
        };
        let payload =
            serde_json::to_vec(&HelloFrame { hello: request }).map_err(io::Error::other)?;
        wire::write_frame(&mut self.writer, &payload)?;
        let response = wire::read_frame(&mut self.reader)?;
        let value: serde_json::Value =
            serde_json::from_slice(&response).map_err(io::Error::other)?;

        if value.get("type").and_then(serde_json::Value::as_str) == Some("ipc_hello") {
            let hello: ServerHello =
                serde_json::from_value(value["data"].clone()).map_err(io::Error::other)?;
            return Ok(Some(hello));
        }
        if let Some(error) = value.get("error") {
            let code = error.get("code").and_then(serde_json::Value::as_str);
            if code == Some("invalid_argument.executor.wire_request") {
                return Ok(None);
            }
            return Err(io::Error::other(format!(
                "the store owner refused the IPC hello: {error}"
            )));
        }
        Err(io::Error::other(
            "unexpected response to the IPC hello (neither ipc_hello nor an error envelope)",
        ))
    }

    /// The owner's hello, when this connection negotiated protocol revision 2.
    pub(crate) fn server_hello(&self) -> Option<&ServerHello> {
        self.server_hello.as_ref()
    }

    /// Send `command` (with the caller's session scope) and reconstruct the
    /// typed result. A transport failure surfaces as a registered
    /// `unavailable.executor.ipc_transport` error (the command's fate is
    /// in-doubt).
    pub(crate) fn execute(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        command: &Command,
    ) -> ExecutorResult<Output> {
        let command_json = serde_json::to_string(command).map_err(|error| {
            ExecutorError::new(
                crate::error::ExecutorErrorClass::Internal,
                "internal.executor.wire_response",
                false,
                format!("could not serialize command for IPC: {error}"),
            )
        })?;
        let raw = RawValue::from_string(command_json).map_err(transport_error)?;
        let request = WireRequestOwned {
            branch,
            space,
            command: &raw,
        };
        let payload = serde_json::to_vec(&request).map_err(transport_error)?;

        wire::write_frame(&mut self.writer, &payload).map_err(transport_error)?;
        let response = wire::read_frame(&mut self.reader).map_err(transport_error)?;
        decode_wire_response(&response)
    }
}

/// Turn a response envelope (`{"type","data"}` or `{"error":…}`) back into a
/// typed `ExecutorResult<Output>` — the inverse of the server's encode step, so
/// the remote result is indistinguishable from a local one.
fn decode_wire_response(bytes: &[u8]) -> ExecutorResult<Output> {
    let value: serde_json::Value = serde_json::from_slice(bytes).map_err(transport_error)?;
    if let Some(error) = value.get("error") {
        let status: ErrorStatus = serde_json::from_value(error.clone()).map_err(transport_error)?;
        return Err(ExecutorError::from_status(status));
    }
    serde_json::from_value::<Output>(value).map_err(transport_error)
}

fn transport_error(error: impl std::fmt::Display) -> ExecutorError {
    ExecutorError::new(
        crate::error::ExecutorErrorClass::Unavailable,
        "unavailable.executor.ipc_transport",
        false,
        format!("IPC transport failure: {error}"),
    )
}

/// The connecting process's display name for the hello identity — the
/// executable stem (`strata` for the CLI). Display metadata only; the socket's
/// same-user permission check is the security boundary.
fn process_name() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|path| {
            path.file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| "unknown".to_owned())
}

#[cfg(test)]
mod tests {
    use super::IpcClient;
    use crate::ipc::wire;
    use std::io::{BufReader, BufWriter};
    use std::os::unix::net::UnixListener;

    /// A hand-rolled pre-hello owner: answers every frame the way the
    /// protocol-1 server did — a `WireRequest` with `ping` gets a pong
    /// envelope; anything else (including a hello frame it has never heard
    /// of) gets the malformed-envelope error — and keeps the connection open.
    fn spawn_legacy_owner(dir: &std::path::Path) -> std::path::PathBuf {
        let sock = dir.join("strata.sock");
        let listener = UnixListener::bind(&sock).expect("bind fake owner");
        std::thread::spawn(move || {
            let Ok((stream, _)) = listener.accept() else {
                return;
            };
            let mut reader = BufReader::new(stream.try_clone().expect("clone"));
            let mut writer = BufWriter::new(stream);
            while let Ok(frame) = wire::read_frame(&mut reader) {
                let value: Result<serde_json::Value, _> = serde_json::from_slice(&frame);
                let is_ping = value
                    .as_ref()
                    .ok()
                    .and_then(|v| v.get("command"))
                    .and_then(|c| c.get("type"))
                    .and_then(|t| t.as_str())
                    == Some("ping");
                let response = if is_ping {
                    "{\"type\":\"pong\",\"data\":{\"version\":\"0.0.0-legacy\"}}".to_owned()
                } else {
                    "{\"error\":{\"class\":\"invalid_argument\",\
                     \"code\":\"invalid_argument.executor.wire_request\",\
                     \"message\":\"malformed IPC request envelope\"}}"
                        .to_owned()
                };
                if wire::write_frame(&mut writer, response.as_bytes()).is_err() {
                    break;
                }
            }
        });
        sock
    }

    #[test]
    fn a_pre_hello_owner_downgrades_the_client_to_protocol_1() {
        // Skew tolerance: a stale long-lived owner (old `strata start`) that
        // predates the hello answers it with a malformed-envelope error and
        // keeps serving. The new client must downgrade and keep working — a
        // hard failure would strand every database owned by an old process.
        let dir = tempfile::tempdir().expect("tmp");
        let sock = spawn_legacy_owner(dir.path());

        let mut client = IpcClient::connect(&sock).expect("connect despite the old owner");
        assert!(
            client.server_hello().is_none(),
            "no hello info on a downgraded (protocol 1) connection"
        );

        let command: crate::Command =
            serde_json::from_value(serde_json::json!({ "type": "ping" })).expect("ping");
        let output = client
            .execute(None, None, &command)
            .expect("commands still run on protocol 1");
        assert_eq!(
            serde_json::to_value(&output).expect("json")["type"],
            "pong",
            "the downgraded connection serves commands"
        );
    }
}
