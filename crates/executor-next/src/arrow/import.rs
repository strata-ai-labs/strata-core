//! Arrow file import through executor commands.

use std::path::PathBuf;

use crate::error::ExecutorResult;
use crate::output::Output;
use crate::types::{
    ArrowFileFormat, ArrowImportResult, ArrowImportTarget, BatchJsonEntry, BatchKvEntry,
    BatchVectorEntry, Bytes, VectorDistanceMetric,
};
use crate::{Command, Executor};

use super::format::detect_format;
use super::reader::read_file;
use super::schema::{
    json_document, key_bytes, resolve_mapping, value_bytes, vector_embedding, vector_metadata,
};
use super::{invalid_input, required_option, unexpected_output};

#[allow(clippy::too_many_arguments)]
pub(crate) fn import_file(
    executor: &mut Executor,
    branch: Option<&str>,
    space: Option<&str>,
    file_path: String,
    format: Option<ArrowFileFormat>,
    target: ArrowImportTarget,
    key_column: Option<&str>,
    value_column: Option<&str>,
    collection: Option<&str>,
) -> ExecutorResult<Output> {
    let path = PathBuf::from(&file_path);
    let format = match format {
        Some(format) => format,
        None => detect_format(&path)?,
    };
    let (schema, batches) = read_file(&path, format)?;
    let mapping = resolve_mapping(&schema, target, key_column, value_column)?;

    let result = match target {
        ArrowImportTarget::Kv => import_kv(executor, branch, space, &batches, &mapping)?,
        ArrowImportTarget::Json => import_json(executor, branch, space, &batches, &mapping)?,
        ArrowImportTarget::Vector => {
            let collection = required_option(
                collection,
                "invalid_argument.executor.arrow_collection",
                "vector Arrow import requires a collection",
            )?;
            import_vector(executor, branch, space, collection, &batches, &mapping)?
        }
    };

    Ok(Output::ArrowImportResult(ArrowImportResult::new(
        target,
        file_path,
        result.rows_imported,
        result.rows_skipped,
        result.batches_processed,
    )))
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct ImportCounts {
    rows_imported: u64,
    rows_skipped: u64,
    batches_processed: u64,
}

fn import_kv(
    executor: &mut Executor,
    branch: Option<&str>,
    space: Option<&str>,
    batches: &[arrow::record_batch::RecordBatch],
    mapping: &super::schema::ImportMapping,
) -> ExecutorResult<ImportCounts> {
    let mut counts = ImportCounts::default();
    for batch in batches {
        let mut entries = Vec::with_capacity(batch.num_rows());
        for row in 0..batch.num_rows() {
            let Some(key) = key_bytes(batch, mapping, row)? else {
                counts.rows_skipped += 1;
                continue;
            };
            let value = value_bytes(batch, mapping, row)?;
            entries.push(BatchKvEntry::new(Bytes::from(key), Bytes::from(value)));
        }
        if !entries.is_empty() {
            let output = executor.execute(Command::KvBatchPut {
                branch: branch.map(str::to_owned),
                space: space.map(str::to_owned),
                entries,
            })?;
            let Output::BatchResults(results) = output else {
                return Err(unexpected_output("kv_batch_put"));
            };
            for item in results.items() {
                if item.error_status().is_some() || !item.applied() {
                    counts.rows_skipped += 1;
                } else {
                    counts.rows_imported += 1;
                }
            }
        }
        counts.batches_processed += 1;
    }
    Ok(counts)
}

fn import_json(
    executor: &mut Executor,
    branch: Option<&str>,
    space: Option<&str>,
    batches: &[arrow::record_batch::RecordBatch],
    mapping: &super::schema::ImportMapping,
) -> ExecutorResult<ImportCounts> {
    let mut counts = ImportCounts::default();
    for batch in batches {
        let mut entries = Vec::with_capacity(batch.num_rows());
        for row in 0..batch.num_rows() {
            let Some(key) = key_bytes(batch, mapping, row)? else {
                counts.rows_skipped += 1;
                continue;
            };
            let key = String::from_utf8(key).map_err(|error| {
                invalid_input(
                    "invalid_argument.executor.arrow_json_key",
                    format!("JSON import key is not valid UTF-8: {error}"),
                )
            })?;
            let value = json_document(batch, mapping, row)?;
            entries.push(BatchJsonEntry::new(key, "$", value));
        }
        if !entries.is_empty() {
            let output = executor.execute(Command::JsonBatchSet {
                branch: branch.map(str::to_owned),
                space: space.map(str::to_owned),
                entries,
            })?;
            let Output::JsonBatchResults(results) = output else {
                return Err(unexpected_output("json_batch_set"));
            };
            for item in results.items() {
                if item.error_status().is_some() {
                    counts.rows_skipped += 1;
                } else {
                    counts.rows_imported += 1;
                }
            }
        }
        counts.batches_processed += 1;
    }
    Ok(counts)
}

fn import_vector(
    executor: &mut Executor,
    branch: Option<&str>,
    space: Option<&str>,
    collection: &str,
    batches: &[arrow::record_batch::RecordBatch],
    mapping: &super::schema::ImportMapping,
) -> ExecutorResult<ImportCounts> {
    let mut counts = ImportCounts::default();
    let mut collection_ready = collection_exists(executor, branch, space, collection)?;
    for batch in batches {
        let mut entries = Vec::with_capacity(batch.num_rows());
        for row in 0..batch.num_rows() {
            let Some(key) = key_bytes(batch, mapping, row)? else {
                counts.rows_skipped += 1;
                continue;
            };
            let key = String::from_utf8(key).map_err(|error| {
                invalid_input(
                    "invalid_argument.executor.arrow_vector_key",
                    format!("vector import key is not valid UTF-8: {error}"),
                )
            })?;
            let Some(vector) = vector_embedding(batch, mapping, row) else {
                counts.rows_skipped += 1;
                continue;
            };
            if !collection_ready {
                create_vector_collection(executor, branch, space, collection, vector.len())?;
                collection_ready = true;
            }
            let metadata = vector_metadata(batch, mapping, row)?;
            entries.push(BatchVectorEntry::new(key, vector, metadata));
        }
        if !entries.is_empty() {
            let output = executor.execute(Command::VectorBatchUpsert {
                branch: branch.map(str::to_owned),
                space: space.map(str::to_owned),
                collection: collection.to_owned(),
                entries,
            })?;
            let Output::VectorBatchUpsertResults(results) = output else {
                return Err(unexpected_output("vector_batch_upsert"));
            };
            for item in results.items() {
                if item.error_status().is_some() || !item.applied() {
                    counts.rows_skipped += 1;
                } else {
                    counts.rows_imported += 1;
                }
            }
        }
        counts.batches_processed += 1;
    }
    Ok(counts)
}

fn collection_exists(
    executor: &mut Executor,
    branch: Option<&str>,
    space: Option<&str>,
    collection: &str,
) -> ExecutorResult<bool> {
    let output = executor.execute(Command::VectorListCollections {
        branch: branch.map(str::to_owned),
        space: space.map(str::to_owned),
    })?;
    let Output::VectorCollectionList {
        items: collections, ..
    } = output
    else {
        return Err(unexpected_output("vector_list_collections"));
    };
    Ok(collections.iter().any(|entry| entry.name() == collection))
}

fn create_vector_collection(
    executor: &mut Executor,
    branch: Option<&str>,
    space: Option<&str>,
    collection: &str,
    dimension: usize,
) -> ExecutorResult<()> {
    let dimension = u64::try_from(dimension).map_err(|_| {
        invalid_input(
            "invalid_argument.executor.arrow_vector_dimension",
            "vector dimension does not fit u64",
        )
    })?;
    let output = executor.execute(Command::VectorCreateCollection {
        branch: branch.map(str::to_owned),
        space: space.map(str::to_owned),
        collection: collection.to_owned(),
        dimension,
        metric: VectorDistanceMetric::Cosine,
    })?;
    let Output::VectorCollectionList { .. } = output else {
        return Err(unexpected_output("vector_create_collection"));
    };
    Ok(())
}
