//! Optional durable sidecar services.

#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "sidecar services are consumed by lifecycle and recovery work added later"
    )
)]

use crate::backend::{
    Backend, BackendError, BackendErrorKind, BackendHandle, PublishError, PublishOutcome,
};
use crate::format::{
    decode_segment_metadata, encode_segment_metadata, FormatError, SegmentMetadata,
};
use crate::layout::{LayoutError, ObjectLayout};
use crate::object::ObjectName;
use crate::service::ObjectPublisher;
use std::fmt;

pub(crate) type WalSegmentMetadataSidecarResult<T> = Result<T, WalSegmentMetadataSidecarError>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum WalSegmentMetadataSidecarError {
    InvalidSegmentId {
        segment_id: u64,
    },
    Layout {
        source: LayoutError,
    },
    Read {
        object: ObjectName,
        source: BackendError,
    },
    Publish {
        object: ObjectName,
        source: Box<PublishError>,
    },
    InvalidPublishMetadata {
        object: ObjectName,
        field: &'static str,
    },
}

impl fmt::Display for WalSegmentMetadataSidecarError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSegmentId { segment_id } => {
                write!(
                    formatter,
                    "segment metadata sidecar id {segment_id} is invalid"
                )
            }
            Self::Layout { source } => {
                write!(
                    formatter,
                    "failed to build segment metadata sidecar object name: {source}"
                )
            }
            Self::Read { object, source } => {
                write!(
                    formatter,
                    "failed to read segment metadata sidecar {object}: {source}"
                )
            }
            Self::Publish { object, source } => {
                write!(
                    formatter,
                    "failed to publish segment metadata sidecar {object}: {source}"
                )
            }
            Self::InvalidPublishMetadata { object, field } => write!(
                formatter,
                "segment metadata sidecar {object} has invalid publish metadata {field}"
            ),
        }
    }
}

impl std::error::Error for WalSegmentMetadataSidecarError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Layout { source } => Some(source),
            Self::Read { source, .. } => Some(source),
            Self::Publish { source, .. } => Some(source.as_ref()),
            Self::InvalidSegmentId { .. } | Self::InvalidPublishMetadata { .. } => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct WalSegmentMetadataSidecar {
    segment_id: u64,
    object: ObjectName,
    metadata: SegmentMetadata,
}

impl WalSegmentMetadataSidecar {
    fn new(segment_id: u64, object: ObjectName, metadata: SegmentMetadata) -> Self {
        Self {
            segment_id,
            object,
            metadata,
        }
    }

    pub(crate) const fn segment_id(&self) -> u64 {
        self.segment_id
    }

    pub(crate) const fn object(&self) -> &ObjectName {
        &self.object
    }

    pub(crate) const fn metadata(&self) -> &SegmentMetadata {
        &self.metadata
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum WalSegmentMetadataSidecarLoad {
    Present(WalSegmentMetadataSidecar),
    Missing {
        segment_id: u64,
        object: ObjectName,
    },
    Corrupt {
        segment_id: u64,
        object: ObjectName,
        source: FormatError,
    },
}

impl WalSegmentMetadataSidecarLoad {
    pub(crate) const fn is_present(&self) -> bool {
        matches!(self, Self::Present(_))
    }

    pub(crate) const fn is_missing(&self) -> bool {
        matches!(self, Self::Missing { .. })
    }

    pub(crate) const fn is_corrupt(&self) -> bool {
        matches!(self, Self::Corrupt { .. })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct WalSegmentMetadataSidecarWrite {
    segment_id: u64,
    object: ObjectName,
    byte_count: u64,
    outcome: PublishOutcome,
}

impl WalSegmentMetadataSidecarWrite {
    fn new(segment_id: u64, object: ObjectName, byte_count: u64, outcome: PublishOutcome) -> Self {
        Self {
            segment_id,
            object,
            byte_count,
            outcome,
        }
    }

    pub(crate) const fn segment_id(&self) -> u64 {
        self.segment_id
    }

    pub(crate) const fn object(&self) -> &ObjectName {
        &self.object
    }

    pub(crate) const fn byte_count(&self) -> u64 {
        self.byte_count
    }

    pub(crate) const fn outcome(&self) -> &PublishOutcome {
        &self.outcome
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct WalSegmentMetadataSidecarDelete {
    segment_id: u64,
    object: ObjectName,
    deleted: bool,
    failure: Option<BackendError>,
}

impl WalSegmentMetadataSidecarDelete {
    fn new(
        segment_id: u64,
        object: ObjectName,
        deleted: bool,
        failure: Option<BackendError>,
    ) -> Self {
        Self {
            segment_id,
            object,
            deleted,
            failure,
        }
    }

    pub(crate) const fn segment_id(&self) -> u64 {
        self.segment_id
    }

    pub(crate) const fn object(&self) -> &ObjectName {
        &self.object
    }

    pub(crate) const fn deleted(&self) -> bool {
        self.deleted
    }

    pub(crate) const fn failure(&self) -> Option<&BackendError> {
        self.failure.as_ref()
    }
}

pub(crate) struct WalSegmentMetadataSidecarService<'a> {
    backend: BackendHandle<'a>,
    publisher: ObjectPublisher<'a>,
}

impl<'a> WalSegmentMetadataSidecarService<'a> {
    pub(crate) fn new(backend: impl Into<BackendHandle<'a>>) -> Self {
        let backend = backend.into();
        Self {
            publisher: ObjectPublisher::new(backend.clone()),
            backend,
        }
    }

    pub(crate) fn publish_replace(
        &self,
        metadata: &SegmentMetadata,
    ) -> WalSegmentMetadataSidecarResult<WalSegmentMetadataSidecarWrite> {
        let object = sidecar_object(metadata.segment_id())?;
        let bytes = encode_segment_metadata(metadata);
        let outcome = self
            .publisher
            .publish_durable_replace(&object, &bytes)
            .map_err(|source| WalSegmentMetadataSidecarError::Publish {
                object: object.clone(),
                source: Box::new(source),
            })?;
        validate_publish_outcome(&object, bytes.len() as u64, &outcome)?;

        Ok(WalSegmentMetadataSidecarWrite::new(
            metadata.segment_id(),
            object,
            bytes.len() as u64,
            outcome,
        ))
    }

    pub(crate) fn load(
        &self,
        segment_id: u64,
    ) -> WalSegmentMetadataSidecarResult<WalSegmentMetadataSidecarLoad> {
        let object = sidecar_object(segment_id)?;
        let bytes = match self.backend.read_object(&object) {
            Ok(bytes) => bytes,
            Err(source) if source.kind() == BackendErrorKind::NotFound => {
                return Ok(WalSegmentMetadataSidecarLoad::Missing { segment_id, object });
            }
            Err(source) => {
                return Err(WalSegmentMetadataSidecarError::Read { object, source });
            }
        };

        // Sidecar bytes are an acceleration cache. Decode failures stay
        // observable, but callers can still recover by scanning WAL bytes.
        let metadata = match decode_segment_metadata(&bytes) {
            Ok(metadata) if metadata.segment_id() == segment_id => metadata,
            Ok(_) => {
                return Ok(WalSegmentMetadataSidecarLoad::Corrupt {
                    segment_id,
                    object,
                    source: FormatError::InvalidValue {
                        field: "segment_id",
                    },
                });
            }
            Err(source) => {
                return Ok(WalSegmentMetadataSidecarLoad::Corrupt {
                    segment_id,
                    object,
                    source,
                });
            }
        };

        Ok(WalSegmentMetadataSidecarLoad::Present(
            WalSegmentMetadataSidecar::new(segment_id, object, metadata),
        ))
    }

    pub(crate) fn delete(
        &self,
        segment_id: u64,
    ) -> WalSegmentMetadataSidecarResult<WalSegmentMetadataSidecarDelete> {
        let object = sidecar_object(segment_id)?;
        // Deleting an optional sidecar cannot be allowed to fail authoritative
        // WAL cleanup, so backend failures are returned as report facts.
        match self.backend.delete_object(&object) {
            Ok(()) => Ok(WalSegmentMetadataSidecarDelete::new(
                segment_id, object, true, None,
            )),
            Err(source) if source.kind() == BackendErrorKind::NotFound => Ok(
                WalSegmentMetadataSidecarDelete::new(segment_id, object, false, None),
            ),
            Err(source) => Ok(WalSegmentMetadataSidecarDelete::new(
                segment_id,
                object,
                false,
                Some(source),
            )),
        }
    }
}

fn sidecar_object(segment_id: u64) -> WalSegmentMetadataSidecarResult<ObjectName> {
    if segment_id == 0 {
        return Err(WalSegmentMetadataSidecarError::InvalidSegmentId { segment_id });
    }

    ObjectLayout::wal_segment_metadata(segment_id)
        .map_err(|source| WalSegmentMetadataSidecarError::Layout { source })
}

fn validate_publish_outcome(
    object: &ObjectName,
    byte_count: u64,
    outcome: &PublishOutcome,
) -> WalSegmentMetadataSidecarResult<()> {
    crate::service::validate_publish_outcome(object, byte_count, outcome).map_err(|mismatch| {
        WalSegmentMetadataSidecarError::InvalidPublishMetadata {
            object: mismatch.object().clone(),
            field: mismatch.field(),
        }
    })
}

#[cfg(test)]
mod tests;
