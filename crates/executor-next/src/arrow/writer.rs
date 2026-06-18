//! Arrow-compatible file writers.

use std::fs::File;
use std::path::Path;

use arrow::record_batch::RecordBatch;

use crate::error::ExecutorResult;
use crate::types::ArrowFileFormat;

use super::{internal_error, invalid_input, io_error};

pub(crate) fn write_file(
    path: &Path,
    format: ArrowFileFormat,
    batches: &[RecordBatch],
) -> ExecutorResult<u64> {
    if batches.is_empty() {
        return Err(invalid_input(
            "invalid_argument.executor.arrow_empty_export",
            "no Arrow batches to write",
        ));
    }

    let schema = batches[0].schema();
    for batch in batches {
        if batch.schema() != schema {
            return Err(internal_error(
                "cannot write Arrow batches with different schemas",
            ));
        }
    }

    match format {
        ArrowFileFormat::Parquet => write_parquet(path, batches)?,
        ArrowFileFormat::Csv => write_csv(path, batches)?,
        ArrowFileFormat::Jsonl => write_jsonl(path, batches)?,
    }

    let metadata = std::fs::metadata(path)
        .map_err(|error| io_error(format!("failed to stat '{}': {error}", path.display())))?;
    Ok(metadata.len())
}

fn write_parquet(path: &Path, batches: &[RecordBatch]) -> ExecutorResult<()> {
    let file = create_file(path)?;
    let properties = parquet::file::properties::WriterProperties::builder()
        .set_compression(parquet::basic::Compression::SNAPPY)
        .build();
    let mut writer =
        parquet::arrow::ArrowWriter::try_new(file, batches[0].schema(), Some(properties))
            .map_err(|error| io_error(format!("failed to create Parquet writer: {error}")))?;
    for batch in batches {
        writer
            .write(batch)
            .map_err(|error| io_error(format!("failed to write Parquet batch: {error}")))?;
    }
    writer
        .close()
        .map_err(|error| io_error(format!("failed to finalize Parquet file: {error}")))?;
    Ok(())
}

fn write_csv(path: &Path, batches: &[RecordBatch]) -> ExecutorResult<()> {
    let file = create_file(path)?;
    let mut writer = arrow::csv::WriterBuilder::new()
        .with_header(true)
        .build(file);
    for batch in batches {
        writer
            .write(batch)
            .map_err(|error| io_error(format!("failed to write CSV batch: {error}")))?;
    }
    Ok(())
}

fn write_jsonl(path: &Path, batches: &[RecordBatch]) -> ExecutorResult<()> {
    let file = create_file(path)?;
    let mut writer = arrow::json::LineDelimitedWriter::new(file);
    for batch in batches {
        writer
            .write(batch)
            .map_err(|error| io_error(format!("failed to write JSONL batch: {error}")))?;
    }
    writer
        .finish()
        .map_err(|error| io_error(format!("failed to finalize JSONL file: {error}")))?;
    Ok(())
}

fn create_file(path: &Path) -> ExecutorResult<File> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent).map_err(|error| {
            io_error(format!(
                "failed to create output directory '{}': {error}",
                parent.display()
            ))
        })?;
    }
    File::create(path).map_err(|error| {
        io_error(format!(
            "failed to create file '{}': {error}",
            path.display()
        ))
    })
}
