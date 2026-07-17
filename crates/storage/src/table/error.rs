//! Table-runtime error vocabulary.

use crate::format::FormatError;
use std::error::Error;
use std::fmt;
use std::sync::Arc;

const MAX_DISPLAY_KEY_BYTES: usize = 16;

pub(crate) type TableRuntimeResult<T> = Result<T, TableRuntimeError>;

#[derive(Clone, Debug)]
pub(crate) enum TableRuntimeError {
    /// A caller-provided compaction output sink refused an output (W1.2c).
    /// The REAL error lives with the sink's owner (e.g. the lifecycle publish
    /// error captured alongside); this variant only unwinds the build.
    OutputSinkFailed,
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
    /// Stable code for this failure (TCP3.2c, #2632), carried across the
    /// storage API boundary as its `inner_code()` so a test can distinguish
    /// two table failures without reading display text (Hard Rule 29).
    ///
    /// Exhaustive with no catch-all: a new variant is a compile error until
    /// it is given a code.
    pub(crate) const fn code(&self) -> &'static str {
        match self {
            Self::OutputSinkFailed => "failed_precondition.table.output_sink",
            Self::InvalidConfig { .. } => "invalid_argument.table.config",
            Self::InvalidRowOrder { .. } => "invalid_argument.table.row_order",
            Self::DuplicateInternalKey { .. } => "invalid_argument.table.duplicate_internal_key",
            Self::InvalidRange { .. } => "invalid_argument.table.range",
            Self::BuildFormat { .. } => "serialization.table.build_format",
            Self::DecodeFormat { .. } => "serialization.table.decode_format",
            Self::SourceRead { .. } => "io.table.source_read",
            Self::Cache { .. } => "failed_precondition.table.cache",
            Self::CompactionPolicy { .. } => "failed_precondition.table.compaction_policy",
            Self::LazyMaterializationDenied { .. } => {
                "failed_precondition.table.lazy_materialization_denied"
            }
        }
    }
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
            Self::OutputSinkFailed => {
                write!(formatter, "compaction output sink refused an output")
            }
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
            Self::OutputSinkFailed
            | Self::InvalidConfig { .. }
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

#[cfg(test)]
mod tests {
    use super::TableRuntimeError;
    use crate::format::FormatError;

    /// TCP3.2c reachability: construct every `TableRuntimeError` variant and
    /// assert its code. This both pins the codes and makes the workspace
    /// error-code guard see each as asserted — a variant that stops being
    /// constructible, or a code that drifts, fails here. The `code()` match
    /// is exhaustive with no catch-all, so a new variant is a compile error
    /// until it is added to both `code()` and this table.
    #[test]
    fn every_variant_has_a_unique_well_formed_code() {
        const CLASSES: &[&str] = &[
            "invalid_argument",
            "failed_precondition",
            "serialization",
            "io",
        ];
        let format = || FormatError::InvalidLength {
            field: "table_test",
        };
        let cases: [(TableRuntimeError, &str); 11] = [
            (
                TableRuntimeError::OutputSinkFailed,
                "failed_precondition.table.output_sink",
            ),
            (
                TableRuntimeError::InvalidConfig {
                    field: "f",
                    reason: "r",
                },
                "invalid_argument.table.config",
            ),
            (
                TableRuntimeError::InvalidRowOrder {
                    previous: vec![1],
                    current: vec![0],
                },
                "invalid_argument.table.row_order",
            ),
            (
                TableRuntimeError::DuplicateInternalKey { key: vec![1] },
                "invalid_argument.table.duplicate_internal_key",
            ),
            (
                TableRuntimeError::InvalidRange { field: "f" },
                "invalid_argument.table.range",
            ),
            (
                TableRuntimeError::BuildFormat { source: format() },
                "serialization.table.build_format",
            ),
            (
                TableRuntimeError::DecodeFormat { source: format() },
                "serialization.table.decode_format",
            ),
            (TableRuntimeError::source_read("r"), "io.table.source_read"),
            (
                TableRuntimeError::Cache { reason: "r" },
                "failed_precondition.table.cache",
            ),
            (
                TableRuntimeError::CompactionPolicy { reason: "r" },
                "failed_precondition.table.compaction_policy",
            ),
            (
                TableRuntimeError::LazyMaterializationDenied { reason: "r" },
                "failed_precondition.table.lazy_materialization_denied",
            ),
        ];

        let mut seen = std::collections::BTreeSet::new();
        for (error, expected) in &cases {
            let code = error.code();
            assert_eq!(&code, expected, "code drifted for {error:?}");
            let parts: Vec<&str> = code.split('.').collect();
            assert_eq!(
                parts.len(),
                3,
                "code must be <class>.<area>.<detail>: {code}"
            );
            assert!(CLASSES.contains(&parts[0]), "unexpected class in {code}");
            assert_eq!(parts[1], "table", "area must be `table`: {code}");
            assert!(
                seen.insert(code),
                "two table variants share the code {code}"
            );
        }
    }
}
