//! Length-prefixed frame protocol for IPC communication.
//!
//! Each frame is a 4-byte big-endian `u32` length followed by `length` bytes
//! of payload. The payload is the executor wire JSON — a `Command` request or
//! a `{type,data}` / `{error}` response envelope (see [`super`]). Maximum
//! frame size is 64 MiB, which bounds a single command or result and rejects a
//! corrupt or hostile length prefix before allocating.

use std::io::{self, Read, Write};

/// Maximum frame payload size: 64 MiB.
pub(crate) const MAX_FRAME_SIZE: u32 = 64 * 1024 * 1024;

/// Write a length-prefixed frame to the stream and flush it.
///
/// # Errors
///
/// Returns [`io::ErrorKind::InvalidInput`] if the payload exceeds
/// [`MAX_FRAME_SIZE`], or any underlying write/flush error.
pub(crate) fn write_frame(stream: &mut impl Write, payload: &[u8]) -> io::Result<()> {
    let len = u32::try_from(payload.len())
        .ok()
        .filter(|len| *len <= MAX_FRAME_SIZE)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "frame payload too large: {} bytes (max {MAX_FRAME_SIZE})",
                    payload.len()
                ),
            )
        })?;
    stream.write_all(&len.to_be_bytes())?;
    stream.write_all(payload)?;
    stream.flush()
}

/// Read a length-prefixed frame from the stream, returning its payload bytes.
///
/// # Errors
///
/// Returns [`io::ErrorKind::InvalidData`] if the length prefix exceeds
/// [`MAX_FRAME_SIZE`] (a corrupt or hostile frame), or any underlying read
/// error — including [`io::ErrorKind::UnexpectedEof`] on a closed stream.
pub(crate) fn read_frame(stream: &mut impl Read) -> io::Result<Vec<u8>> {
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf)?;
    let len = u32::from_be_bytes(len_buf);

    if len > MAX_FRAME_SIZE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("frame too large: {len} bytes (max {MAX_FRAME_SIZE})"),
        ));
    }

    let mut payload = vec![0u8; len as usize];
    stream.read_exact(&mut payload)?;
    Ok(payload)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn round_trip_preserves_payload() {
        let data = b"{\"type\":\"ping\"}";
        let mut buf = Vec::new();
        write_frame(&mut buf, data).expect("write");

        let mut cursor = Cursor::new(buf);
        let result = read_frame(&mut cursor).expect("read");
        assert_eq!(result, data);
    }

    #[test]
    fn empty_frame_round_trips() {
        let mut buf = Vec::new();
        write_frame(&mut buf, b"").expect("write");

        let mut cursor = Cursor::new(buf);
        let result = read_frame(&mut cursor).expect("read");
        assert!(result.is_empty());
    }

    #[test]
    fn back_to_back_frames_read_in_order() {
        let mut buf = Vec::new();
        write_frame(&mut buf, b"first").expect("write");
        write_frame(&mut buf, b"second").expect("write");

        let mut cursor = Cursor::new(buf);
        assert_eq!(read_frame(&mut cursor).expect("read"), b"first");
        assert_eq!(read_frame(&mut cursor).expect("read"), b"second");
    }

    #[test]
    fn oversized_write_is_rejected() {
        let data = vec![0u8; (MAX_FRAME_SIZE as usize) + 1];
        let mut buf = Vec::new();
        let err = write_frame(&mut buf, &data).expect_err("oversized payload");
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
    }

    #[test]
    fn oversized_length_prefix_is_rejected_before_allocating() {
        let bad_len = (MAX_FRAME_SIZE + 1).to_be_bytes();
        let mut cursor = Cursor::new(bad_len.to_vec());
        let err = read_frame(&mut cursor).expect_err("oversized length");
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn truncated_length_prefix_is_unexpected_eof() {
        let mut cursor = Cursor::new(vec![0u8, 0u8]); // only 2 of 4 length bytes
        let err = read_frame(&mut cursor).expect_err("truncated");
        assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof);
    }
}
