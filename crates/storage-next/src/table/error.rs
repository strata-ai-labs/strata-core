//! Table-runtime error vocabulary.

use crate::format::FormatError;
use std::fmt;

const MAX_DISPLAY_KEY_BYTES: usize = 16;

pub(crate) type TableRuntimeResult<T> = Result<T, TableRuntimeError>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum TableRuntimeError {
    InvalidConfig {
        field: &'static str,
        reason: &'static str,
    },
    InvalidRowOrder {
        previous: Vec<u8>,
        current: Vec<u8>,
    },
    DuplicateInternalKey {
        key: Vec<u8>,
    },
    InvalidRange {
        field: &'static str,
    },
    BuildFormat {
        source: FormatError,
    },
    DecodeFormat {
        source: FormatError,
    },
    SourceRead {
        reason: &'static str,
    },
    Cache {
        reason: &'static str,
    },
    CompactionPolicy {
        reason: &'static str,
    },
}

impl fmt::Display for TableRuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfig { field, reason } => {
                write!(
                    formatter,
                    "table configuration field {field} is invalid: {reason}"
                )
            }
            Self::InvalidRowOrder { previous, current } => write!(
                formatter,
                "table rows are out of order: previous key {} sorts after current key {}",
                bounded_hex_bytes(previous),
                bounded_hex_bytes(current)
            ),
            Self::DuplicateInternalKey { key } => {
                write!(
                    formatter,
                    "table row key {} is duplicated",
                    bounded_hex_bytes(key)
                )
            }
            Self::InvalidRange { field } => {
                write!(formatter, "table range field {field} is invalid")
            }
            Self::BuildFormat { source } => {
                write!(formatter, "failed to build immutable table bytes: {source}")
            }
            Self::DecodeFormat { source } => {
                write!(
                    formatter,
                    "failed to decode immutable table bytes: {source}"
                )
            }
            Self::SourceRead { reason } => {
                write!(formatter, "failed to read table source: {reason}")
            }
            Self::Cache { reason } => {
                write!(formatter, "table cache operation failed: {reason}")
            }
            Self::CompactionPolicy { reason } => {
                write!(formatter, "table compaction policy failed: {reason}")
            }
        }
    }
}

impl std::error::Error for TableRuntimeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::BuildFormat { source } | Self::DecodeFormat { source } => Some(source),
            Self::InvalidConfig { .. }
            | Self::InvalidRowOrder { .. }
            | Self::DuplicateInternalKey { .. }
            | Self::InvalidRange { .. }
            | Self::SourceRead { .. }
            | Self::Cache { .. }
            | Self::CompactionPolicy { .. } => None,
        }
    }
}

fn bounded_hex_bytes(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let shown = bytes.len().min(MAX_DISPLAY_KEY_BYTES);
    let mut output = String::with_capacity(shown.saturating_mul(2).saturating_add(24));
    for byte in &bytes[..shown] {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    if bytes.len() > shown {
        output.push_str("...(");
        output.push_str(&bytes.len().to_string());
        output.push_str(" bytes)");
    }
    output
}
