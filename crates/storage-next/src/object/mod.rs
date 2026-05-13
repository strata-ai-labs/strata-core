//! Validated object names and prefixes.

#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the contract is consumed by concrete backends added in later work"
    )
)]

use std::fmt;

pub(crate) const MAX_OBJECT_NAME_BYTES: usize = 1024;

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct ObjectName(String);

impl ObjectName {
    pub(crate) fn new(value: impl Into<String>) -> Result<Self, ObjectNameError> {
        let value = value.into();
        validate_object_text(&value, EmptyPolicy::Reject, TrailingSlashPolicy::Reject)?;
        Ok(Self(value))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ObjectName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct ObjectPrefix(String);

impl ObjectPrefix {
    pub(crate) fn new(value: impl Into<String>) -> Result<Self, ObjectNameError> {
        let value = value.into();
        validate_object_text(&value, EmptyPolicy::Allow, TrailingSlashPolicy::Allow)?;
        Ok(Self(value))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ObjectPrefix {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ObjectNameError {
    Empty,
    TooLong { actual: usize, max: usize },
    Absolute,
    EmptyComponent,
    TraversalComponent,
    TrailingSlash,
    InvalidByte(u8),
}

impl fmt::Display for ObjectNameError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("object name is empty"),
            Self::TooLong { actual, max } => {
                write!(
                    formatter,
                    "object name is {actual} bytes; maximum is {max} bytes"
                )
            }
            Self::Absolute => formatter.write_str("object name must be database-relative"),
            Self::EmptyComponent => formatter.write_str("object name contains an empty component"),
            Self::TraversalComponent => {
                formatter.write_str("object name contains a traversal component")
            }
            Self::TrailingSlash => formatter.write_str("object name must not end with '/'"),
            Self::InvalidByte(byte) => {
                write!(formatter, "object name contains invalid byte 0x{byte:02x}")
            }
        }
    }
}

impl std::error::Error for ObjectNameError {}

#[derive(Clone, Copy)]
enum EmptyPolicy {
    Allow,
    Reject,
}

#[derive(Clone, Copy)]
enum TrailingSlashPolicy {
    Allow,
    Reject,
}

fn validate_object_text(
    value: &str,
    empty_policy: EmptyPolicy,
    trailing_slash_policy: TrailingSlashPolicy,
) -> Result<(), ObjectNameError> {
    if value.is_empty() {
        return match empty_policy {
            EmptyPolicy::Allow => Ok(()),
            EmptyPolicy::Reject => Err(ObjectNameError::Empty),
        };
    }

    if value.len() > MAX_OBJECT_NAME_BYTES {
        return Err(ObjectNameError::TooLong {
            actual: value.len(),
            max: MAX_OBJECT_NAME_BYTES,
        });
    }

    if value.starts_with('/') {
        return Err(ObjectNameError::Absolute);
    }

    if value.ends_with('/') && matches!(trailing_slash_policy, TrailingSlashPolicy::Reject) {
        return Err(ObjectNameError::TrailingSlash);
    }

    for byte in value.bytes() {
        if !is_valid_object_byte(byte) {
            return Err(ObjectNameError::InvalidByte(byte));
        }
    }

    let mut components = value.split('/').peekable();
    while let Some(component) = components.next() {
        if component.is_empty() {
            let final_component = components.peek().is_none();
            if final_component && matches!(trailing_slash_policy, TrailingSlashPolicy::Allow) {
                continue;
            }
            return Err(ObjectNameError::EmptyComponent);
        }

        if component == "." || component == ".." {
            return Err(ObjectNameError::TraversalComponent);
        }
    }

    Ok(())
}

fn is_valid_object_byte(byte: u8) -> bool {
    matches!(
        byte,
        b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'/' | b'.' | b'-' | b'_'
    )
}

#[cfg(test)]
mod tests {
    use super::{ObjectName, ObjectNameError, ObjectPrefix, MAX_OBJECT_NAME_BYTES};

    #[test]
    fn object_name_accepts_database_relative_components() {
        let name = ObjectName::new("manifest/current").expect("valid object name");

        assert_eq!(name.as_str(), "manifest/current");
        assert_eq!(name.to_string(), "manifest/current");
    }

    #[test]
    fn object_name_rejects_ambiguous_or_host_paths() {
        assert_eq!(ObjectName::new(""), Err(ObjectNameError::Empty));
        assert_eq!(ObjectName::new("/manifest"), Err(ObjectNameError::Absolute));
        assert_eq!(
            ObjectName::new("manifest//current"),
            Err(ObjectNameError::EmptyComponent)
        );
        assert_eq!(
            ObjectName::new("manifest/../current"),
            Err(ObjectNameError::TraversalComponent)
        );
        assert_eq!(
            ObjectName::new("manifest/current/"),
            Err(ObjectNameError::TrailingSlash)
        );
        assert_eq!(
            ObjectName::new("manifest\\current"),
            Err(ObjectNameError::InvalidByte(b'\\'))
        );
        assert_eq!(
            ObjectName::new("s3:bucket/key"),
            Err(ObjectNameError::InvalidByte(b':'))
        );
        assert_eq!(
            ObjectName::new("C:/tmp"),
            Err(ObjectNameError::InvalidByte(b':'))
        );
        assert_eq!(
            ObjectName::new("tables/name with space"),
            Err(ObjectNameError::InvalidByte(b' '))
        );
        assert_eq!(
            ObjectName::new("tables/\u{00e9}"),
            Err(ObjectNameError::InvalidByte(0xc3))
        );
    }

    #[test]
    fn object_name_rejects_portability_limit_overflow() {
        let oversized = "a".repeat(MAX_OBJECT_NAME_BYTES + 1);

        assert_eq!(
            ObjectName::new(oversized),
            Err(ObjectNameError::TooLong {
                actual: MAX_OBJECT_NAME_BYTES + 1,
                max: MAX_OBJECT_NAME_BYTES,
            })
        );
    }

    #[test]
    fn object_prefix_allows_empty_and_trailing_separator() {
        let all = ObjectPrefix::new("").expect("empty prefix lists all objects");
        let family = ObjectPrefix::new("tables/").expect("family prefix");

        assert_eq!(all.as_str(), "");
        assert_eq!(family.as_str(), "tables/");
    }

    #[test]
    fn object_prefix_still_rejects_host_paths() {
        assert_eq!(ObjectPrefix::new("/tables"), Err(ObjectNameError::Absolute));
        assert_eq!(
            ObjectPrefix::new("tables//branch"),
            Err(ObjectNameError::EmptyComponent)
        );
        assert_eq!(
            ObjectPrefix::new("tables//"),
            Err(ObjectNameError::EmptyComponent)
        );
        assert_eq!(
            ObjectPrefix::new("tables/../branch"),
            Err(ObjectNameError::TraversalComponent)
        );
    }
}
