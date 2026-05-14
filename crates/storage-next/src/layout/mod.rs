//! Durable object layout and namespace ownership.

#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "object layout constructors are consumed by durable services added later"
    )
)]

use crate::object::{ObjectName, ObjectNameError, ObjectPrefix};
use std::fmt;

const MAX_TABLE_LEVEL: u32 = 9_999;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ObjectFamily {
    Manifest,
    Wal,
    Tables,
    Snapshots,
    Temporary,
    Quarantine,
    Locks,
    Meta,
}

impl ObjectFamily {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Manifest => "manifest",
            Self::Wal => "wal",
            Self::Tables => "tables",
            Self::Snapshots => "snapshots",
            Self::Temporary => "tmp",
            Self::Quarantine => "quarantine",
            Self::Locks => "locks",
            Self::Meta => "meta",
        }
    }

    pub(crate) fn from_object_name(name: &ObjectName) -> Option<Self> {
        let family = name.as_str().split('/').next()?;
        match family {
            "manifest" => Some(Self::Manifest),
            "wal" => Some(Self::Wal),
            "tables" => Some(Self::Tables),
            "snapshots" => Some(Self::Snapshots),
            "tmp" => Some(Self::Temporary),
            "quarantine" => Some(Self::Quarantine),
            "locks" => Some(Self::Locks),
            "meta" => Some(Self::Meta),
            _ => None,
        }
    }
}

impl fmt::Display for ObjectFamily {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum LayoutError {
    InvalidObjectName(ObjectNameError),
    EmptyComponent {
        role: &'static str,
    },
    ComponentContainsSeparator {
        role: &'static str,
    },
    InvalidComponent {
        role: &'static str,
        source: ObjectNameError,
    },
    LevelOutOfRange {
        level: u32,
        max: u32,
    },
}

impl fmt::Display for LayoutError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidObjectName(source) => write!(formatter, "invalid object name: {source}"),
            Self::EmptyComponent { role } => write!(formatter, "{role} component is empty"),
            Self::ComponentContainsSeparator { role } => {
                write!(formatter, "{role} component must not contain '/'")
            }
            Self::InvalidComponent { role, source } => {
                write!(formatter, "{role} component is invalid: {source}")
            }
            Self::LevelOutOfRange { level, max } => {
                write!(formatter, "table level {level} exceeds maximum {max}")
            }
        }
    }
}

impl std::error::Error for LayoutError {}

pub(crate) type LayoutResult<T> = Result<T, LayoutError>;

pub(crate) struct ObjectLayout;

impl ObjectLayout {
    pub(crate) fn family_prefix(family: ObjectFamily) -> LayoutResult<ObjectPrefix> {
        object_prefix(&[family.as_str()])
    }

    pub(crate) fn manifest_prefix() -> LayoutResult<ObjectPrefix> {
        Self::family_prefix(ObjectFamily::Manifest)
    }

    pub(crate) fn database_manifest() -> LayoutResult<ObjectName> {
        object_name(&[ObjectFamily::Manifest.as_str(), "current"])
    }

    pub(crate) fn wal_prefix() -> LayoutResult<ObjectPrefix> {
        Self::family_prefix(ObjectFamily::Wal)
    }

    pub(crate) fn wal_segment(segment_id: u64) -> LayoutResult<ObjectName> {
        object_name(&[ObjectFamily::Wal.as_str(), &fixed_u64(segment_id)])
    }

    pub(crate) fn wal_segment_metadata_prefix() -> LayoutResult<ObjectPrefix> {
        object_prefix(&[ObjectFamily::Meta.as_str(), ObjectFamily::Wal.as_str()])
    }

    pub(crate) fn wal_segment_metadata(segment_id: u64) -> LayoutResult<ObjectName> {
        object_name(&[
            ObjectFamily::Meta.as_str(),
            ObjectFamily::Wal.as_str(),
            &fixed_u64(segment_id),
        ])
    }

    pub(crate) fn table_prefix() -> LayoutResult<ObjectPrefix> {
        Self::family_prefix(ObjectFamily::Tables)
    }

    pub(crate) fn branch_table_prefix(branch_id: &str) -> LayoutResult<ObjectPrefix> {
        validate_component("branch", branch_id)?;
        object_prefix(&[ObjectFamily::Tables.as_str(), branch_id])
    }

    pub(crate) fn branch_table_manifest(branch_id: &str) -> LayoutResult<ObjectName> {
        validate_component("branch", branch_id)?;
        object_name(&[ObjectFamily::Tables.as_str(), branch_id, "manifest"])
    }

    pub(crate) fn table_object(
        branch_id: &str,
        level: u32,
        table_id: &str,
    ) -> LayoutResult<ObjectName> {
        validate_component("branch", branch_id)?;
        validate_component("table", table_id)?;
        let level = level_component(level)?;
        object_name(&[ObjectFamily::Tables.as_str(), branch_id, &level, table_id])
    }

    pub(crate) fn snapshot_prefix() -> LayoutResult<ObjectPrefix> {
        Self::family_prefix(ObjectFamily::Snapshots)
    }

    pub(crate) fn snapshot(snapshot_id: u64) -> LayoutResult<ObjectName> {
        object_name(&[ObjectFamily::Snapshots.as_str(), &fixed_u64(snapshot_id)])
    }

    pub(crate) fn temporary_prefix() -> LayoutResult<ObjectPrefix> {
        Self::family_prefix(ObjectFamily::Temporary)
    }

    pub(crate) fn operation_temporary_prefix(operation_id: &str) -> LayoutResult<ObjectPrefix> {
        validate_component("operation", operation_id)?;
        object_prefix(&[ObjectFamily::Temporary.as_str(), operation_id])
    }

    pub(crate) fn temporary_object(
        operation_id: &str,
        object_id: &str,
    ) -> LayoutResult<ObjectName> {
        validate_component("operation", operation_id)?;
        validate_component("temporary object", object_id)?;
        object_name(&[ObjectFamily::Temporary.as_str(), operation_id, object_id])
    }

    pub(crate) fn quarantine_prefix() -> LayoutResult<ObjectPrefix> {
        Self::family_prefix(ObjectFamily::Quarantine)
    }

    pub(crate) fn branch_quarantine_prefix(branch_id: &str) -> LayoutResult<ObjectPrefix> {
        validate_component("branch", branch_id)?;
        object_prefix(&[ObjectFamily::Quarantine.as_str(), branch_id])
    }

    pub(crate) fn quarantine_manifest(branch_id: &str) -> LayoutResult<ObjectName> {
        validate_component("branch", branch_id)?;
        object_name(&[ObjectFamily::Quarantine.as_str(), branch_id, "manifest"])
    }

    pub(crate) fn quarantine_object(branch_id: &str, object_id: &str) -> LayoutResult<ObjectName> {
        validate_component("branch", branch_id)?;
        validate_component("quarantine object", object_id)?;
        object_name(&[ObjectFamily::Quarantine.as_str(), branch_id, object_id])
    }

    pub(crate) fn locks_prefix() -> LayoutResult<ObjectPrefix> {
        Self::family_prefix(ObjectFamily::Locks)
    }

    pub(crate) fn writer_lock() -> LayoutResult<ObjectName> {
        object_name(&[ObjectFamily::Locks.as_str(), "writer"])
    }

    pub(crate) fn meta_prefix() -> LayoutResult<ObjectPrefix> {
        Self::family_prefix(ObjectFamily::Meta)
    }

    pub(crate) fn database_meta() -> LayoutResult<ObjectName> {
        object_name(&[ObjectFamily::Meta.as_str(), "database"])
    }
}

fn fixed_u64(value: u64) -> String {
    // Fixed-width lowercase hex preserves numeric ordering in ordinary
    // lexicographic object listings.
    format!("{value:016x}")
}

fn level_component(level: u32) -> LayoutResult<String> {
    if level > MAX_TABLE_LEVEL {
        return Err(LayoutError::LevelOutOfRange {
            level,
            max: MAX_TABLE_LEVEL,
        });
    }

    Ok(format!("l{level:04}"))
}

fn object_name(components: &[&str]) -> LayoutResult<ObjectName> {
    for component in components {
        validate_component("object-name", component)?;
    }
    ObjectName::new(components.join("/")).map_err(LayoutError::InvalidObjectName)
}

fn object_prefix(components: &[&str]) -> LayoutResult<ObjectPrefix> {
    for component in components {
        validate_component("object-prefix", component)?;
    }
    ObjectPrefix::new(format!("{}/", components.join("/"))).map_err(LayoutError::InvalidObjectName)
}

fn validate_component(role: &'static str, component: &str) -> LayoutResult<()> {
    // Components are validated before joining so a caller cannot smuggle an
    // extra path segment into a branch, table, or quarantine object name.
    if component.is_empty() {
        return Err(LayoutError::EmptyComponent { role });
    }
    if component.contains('/') {
        return Err(LayoutError::ComponentContainsSeparator { role });
    }

    ObjectName::new(component)
        .map(|_| ())
        .map_err(|source| LayoutError::InvalidComponent { role, source })
}

#[cfg(test)]
mod tests;
