//! Arrow-compatible file readers.

use std::fs::File;
use std::io::{BufReader, Seek};
use std::path::Path;
use std::sync::Arc;

use arrow::datatypes::Schema;
use arrow::record_batch::RecordBatch;

use crate::error::ExecutorResult;
use crate::types::ArrowFileFormat;

use super::{invalid_input, io_error};

pub(crate) fn read_file(
    path: &Path,
    format: ArrowFileFormat,
) -> ExecutorResult<(Schema, Vec<RecordBatch>)> {
    if !path.exists() {
        return Err(invalid_input(
            "invalid_argument.executor.arrow_input_missing",
            format!("file not found: '{}'", path.display()),
        ));
    }

    match format {
        ArrowFileFormat::Parquet => read_parquet(path),
        ArrowFileFormat::Csv => read_csv(path),
        ArrowFileFormat::Jsonl => read_jsonl(path),
    }
}

fn read_parquet(path: &Path) -> ExecutorResult<(Schema, Vec<RecordBatch>)> {
    let file = open_file(path)?;
    let builder = parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder::try_new(file)
        .map_err(|error| io_error(format!("failed to open Parquet file: {error}")))?;
    let schema = builder.schema().as_ref().clone();
    let reader = builder
        .build()
        .map_err(|error| io_error(format!("failed to build Parquet reader: {error}")))?;
    let batches = reader
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| io_error(format!("failed to read Parquet batch: {error}")))?;
    Ok((schema, batches))
}

fn read_csv(path: &Path) -> ExecutorResult<(Schema, Vec<RecordBatch>)> {
    let schema = arrow::csv::reader::infer_schema_from_files(
        &[path.to_string_lossy().into_owned()],
        b',',
        Some(100),
        true,
    )
    .map_err(|error| io_error(format!("failed to infer CSV schema: {error}")))?;
    let schema = Arc::new(schema);
    let file = open_file(path)?;
    let reader = arrow::csv::ReaderBuilder::new(schema.clone())
        .with_header(true)
        .build(file)
        .map_err(|error| io_error(format!("failed to build CSV reader: {error}")))?;
    let batches = reader
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| io_error(format!("failed to read CSV batch: {error}")))?;
    Ok((schema.as_ref().clone(), batches))
}

fn read_jsonl(path: &Path) -> ExecutorResult<(Schema, Vec<RecordBatch>)> {
    let file = open_file(path)?;
    let mut reader = BufReader::new(file);
    let (schema, _) = arrow::json::reader::infer_json_schema_from_seekable(&mut reader, Some(100))
        .map_err(|error| io_error(format!("failed to infer JSONL schema: {error}")))?;
    let schema = Arc::new(schema);
    reader
        .seek(std::io::SeekFrom::Start(0))
        .map_err(|error| io_error(format!("failed to seek JSONL file: {error}")))?;
    let reader = arrow::json::ReaderBuilder::new(schema.clone())
        .build(reader)
        .map_err(|error| io_error(format!("failed to build JSONL reader: {error}")))?;
    let batches = reader
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| io_error(format!("failed to read JSONL batch: {error}")))?;
    Ok((schema.as_ref().clone(), batches))
}

fn open_file(path: &Path) -> ExecutorResult<File> {
    File::open(path)
        .map_err(|error| io_error(format!("failed to open file '{}': {error}", path.display())))
}
