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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use arrow::array::{Int64Array, StringArray};
    use arrow::datatypes::{DataType, Field, Schema};
    use tempfile::TempDir;

    use crate::arrow::writer::write_file;
    use crate::ExecutorErrorClass;

    use super::*;

    #[test]
    fn reads_each_supported_format_with_schema_and_rows() {
        let dir = TempDir::new().expect("temp dir");
        for (format, name) in [
            (ArrowFileFormat::Parquet, "rows.parquet"),
            (ArrowFileFormat::Csv, "rows.csv"),
            (ArrowFileFormat::Jsonl, "rows.jsonl"),
        ] {
            let path = dir.path().join(name);
            write_file(&path, format, &[sample_batch()]).expect("write fixture");
            let (schema, batches) = read_file(&path, format).expect("read succeeds");
            assert_eq!(
                schema
                    .fields()
                    .iter()
                    .map(|field| field.name().as_str())
                    .collect::<Vec<_>>(),
                vec!["key", "value"]
            );
            assert_eq!(batches.iter().map(RecordBatch::num_rows).sum::<usize>(), 2);
        }
    }

    #[test]
    fn missing_and_malformed_files_have_stable_error_classes() {
        let dir = TempDir::new().expect("temp dir");
        let missing = dir.path().join("missing.parquet");
        let error = read_file(&missing, ArrowFileFormat::Parquet).expect_err("missing file fails");
        assert_eq!(error.class(), ExecutorErrorClass::InvalidInput);
        assert_eq!(
            error.code(),
            "invalid_argument.executor.arrow_input_missing"
        );

        let malformed = dir.path().join("bad.parquet");
        std::fs::write(&malformed, b"not parquet").expect("write malformed");
        let error =
            read_file(&malformed, ArrowFileFormat::Parquet).expect_err("malformed file fails");
        assert_eq!(error.class(), ExecutorErrorClass::Unavailable);
        assert_eq!(error.code(), "unavailable.executor.arrow_io");
    }

    fn sample_batch() -> RecordBatch {
        RecordBatch::try_new(
            Arc::new(Schema::new(vec![
                Field::new("key", DataType::Utf8, false),
                Field::new("value", DataType::Int64, false),
            ])),
            vec![
                Arc::new(StringArray::from(vec!["a", "b"])),
                Arc::new(Int64Array::from(vec![1, 2])),
            ],
        )
        .expect("sample batch")
    }
}
