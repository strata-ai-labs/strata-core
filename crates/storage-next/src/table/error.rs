//! Table-runtime error vocabulary.

use crate::format::FormatError;
use std::error::Error;
use std::fmt;
use std::sync::Arc;

const MAX_DISPLAY_KEY_BYTES: usize = 16;

pub(crate) type TableRuntimeResult<T> = Result<T, TableRuntimeError>;

#[derive(Clone, Debug)]
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
        source: Option<Arc<dyn Error + Send + Sync + 'static>>,
    },
    Cache {
        reason: &'static str,
    },
    CompactionPolicy {
        reason: &'static str,
    },
    LazyMaterializationDenied {
        reason: &'static str,
    },
}

impl TableRuntimeError {
    pub(crate) fn source_read(reason: &'static str) -> Self {
        Self::SourceRead {
            reason,
            source: None,
        }
    }

    pub(crate) fn source_read_with(
        reason: &'static str,
        source: impl Error + Send + Sync + 'static,
    ) -> Self {
        Self::SourceRead {
            reason,
            source: Some(Arc::new(source)),
        }
    }
}

impl PartialEq for TableRuntimeError {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                Self::InvalidConfig {
                    field: left_field,
                    reason: left_reason,
                },
                Self::InvalidConfig {
                    field: right_field,
                    reason: right_reason,
                },
            ) => left_field == right_field && left_reason == right_reason,
            (
                Self::InvalidRowOrder {
                    previous: left_previous,
                    current: left_current,
                },
                Self::InvalidRowOrder {
                    previous: right_previous,
                    current: right_current,
                },
            ) => left_previous == right_previous && left_current == right_current,
            (
                Self::DuplicateInternalKey { key: left },
                Self::DuplicateInternalKey { key: right },
            ) => left == right,
            (Self::InvalidRange { field: left }, Self::InvalidRange { field: right })
            | (Self::Cache { reason: left }, Self::Cache { reason: right })
            | (Self::CompactionPolicy { reason: left }, Self::CompactionPolicy { reason: right })
            | (
                Self::LazyMaterializationDenied { reason: left },
                Self::LazyMaterializationDenied { reason: right },
            ) => left == right,
            (Self::BuildFormat { source: left }, Self::BuildFormat { source: right })
            | (Self::DecodeFormat { source: left }, Self::DecodeFormat { source: right }) => {
                left == right
            }
            (
                Self::SourceRead {
                    reason: left_reason,
                    ..
                },
                Self::SourceRead {
                    reason: right_reason,
                    ..
                },
            ) => left_reason == right_reason,
            _ => false,
        }
    }
}

impl Eq for TableRuntimeError {}

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
            Self::SourceRead { reason, .. } => {
                write!(formatter, "failed to read table source: {reason}")
            }
            Self::Cache { reason } => {
                write!(formatter, "table cache operation failed: {reason}")
            }
            Self::CompactionPolicy { reason } => {
                write!(formatter, "table compaction policy failed: {reason}")
            }
            Self::LazyMaterializationDenied { reason } => {
                write!(
                    formatter,
                    "lazy table full materialization denied: {reason}"
                )
            }
        }
    }
}

impl std::error::Error for TableRuntimeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::BuildFormat { source } | Self::DecodeFormat { source } => Some(source),
            Self::SourceRead {
                source: Some(source),
                ..
            } => Some(source.as_ref()),
            Self::InvalidConfig { .. }
            | Self::InvalidRowOrder { .. }
            | Self::DuplicateInternalKey { .. }
            | Self::InvalidRange { .. }
            | Self::SourceRead { source: None, .. }
            | Self::Cache { .. }
            | Self::CompactionPolicy { .. }
            | Self::LazyMaterializationDenied { .. } => None,
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
