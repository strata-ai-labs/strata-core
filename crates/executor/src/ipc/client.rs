//! IPC client: speaks the framed wire to a store owner's socket and
//! reconstructs the same `ExecutorResult<Output>` an in-process executor
//! returns, so a remote connection is transport-transparent to callers.

use std::io::{self, BufReader, BufWriter};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

use serde_json::value::RawValue;

use crate::{Command, ErrorStatus, ExecutorError, ExecutorResult, Output};

use super::protocol::WireRequestOwned;
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
}

impl IpcClient {
    /// Connect to a store owner listening at `socket_path`.
    pub(crate) fn connect(socket_path: &Path) -> io::Result<Self> {
        let stream = UnixStream::connect(socket_path)?;
        stream.set_read_timeout(Some(READ_TIMEOUT))?;
        stream.set_write_timeout(Some(WRITE_TIMEOUT))?;
        Ok(Self {
            reader: BufReader::new(stream.try_clone()?),
            writer: BufWriter::new(stream),
        })
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
