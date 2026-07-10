//! The M8E2 `Engine` trait impl (slice `HB5`) — Ask 3's final glue.
//!
//! `stratahub-ingest` drives exports through its async [`Engine`] trait;
//! this adapter is pure delegation over the synchronous
//! [`StrataCoreEngine`](crate::StrataCoreEngine). The trait passes
//! `source_path` per call, so the adapter is stateless: each export
//! opens its own handle inside `spawn_blocking` (the coordination
//! doc's Q2 lean — sync internals behind an async boundary), which
//! also makes `Send + Sync` trivial.

use std::path::Path;

use async_trait::async_trait;
use bytes::Bytes;
use stratahub_ingest::engine::{
    AuxiliaryHashes, Engine, EngineError, EngineExportOptions, EngineExportOutput, EngineInfo,
    EngineObject,
};

use crate::error::BundleExportError;

/// The strata-core engine, as `stratahub-ingest` consumes it.
#[derive(Clone, Copy, Debug, Default)]
pub struct IngestEngine;

impl IngestEngine {
    /// Creates the adapter.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Engine for IngestEngine {
    async fn export_bundle(
        &self,
        source_path: &Path,
        options: &EngineExportOptions,
    ) -> Result<EngineExportOutput, EngineError> {
        let source_path = source_path.to_owned();
        let options = crate::EngineExportOptions {
            emit_schema_preview: options.emit_schema_preview,
            branches: options.branches.clone(),
        };
        let output = tokio::task::spawn_blocking(move || {
            let mut engine = crate::StrataCoreEngine::open(&source_path)?;
            engine.export_bundle(&options)
        })
        .await
        .map_err(|error| EngineError::Internal {
            detail: format!("export task failed: {error}"),
        })??;

        Ok(EngineExportOutput {
            manifest_canonical_bytes: Bytes::from(output.manifest_canonical_bytes),
            manifest: output.manifest,
            objects: Box::new(futures_util::stream::iter(output.objects.into_iter().map(
                |object| {
                    Ok(EngineObject {
                        hash: object.hash,
                        size_bytes: object.size_bytes,
                        content_type: object.content_type,
                        body: Box::new(std::io::Cursor::new(object.bytes)),
                    })
                },
            ))),
            schema_blob: output.schema_blob.map(Bytes::from),
            preview_blob: output.preview_blob.map(Bytes::from),
            auxiliary_hashes: AuxiliaryHashes {
                schema: output.auxiliary_hashes.schema,
                preview: output.auxiliary_hashes.preview,
            },
        })
    }

    fn engine_info(&self) -> EngineInfo {
        let info = crate::engine_info();
        EngineInfo {
            version: info.version,
            capability_registry_version: info.capability_registry_version,
            supported_primitives: info.supported_primitives,
        }
    }
}

impl From<BundleExportError> for EngineError {
    fn from(error: BundleExportError) -> Self {
        match error {
            BundleExportError::NotAStrataDb(path) => Self::NotAStrataDb(path),
            BundleExportError::Locked { path } => Self::Locked { path },
            BundleExportError::BranchNotFound(branch) => {
                match stratahub_protocol::BranchName::parse(&branch) {
                    Ok(branch) => Self::BranchNotFound(branch),
                    Err(_) => Self::Internal {
                        detail: format!("requested branch `{branch}` does not exist"),
                    },
                }
            }
            BundleExportError::Internal { detail } => Self::Internal { detail },
            BundleExportError::Io { source } => Self::Io { source },
        }
    }
}
