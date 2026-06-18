//! Arrow file format helpers.

use std::path::Path;

use crate::error::ExecutorResult;
use crate::types::ArrowFileFormat;

use super::invalid_input;

pub(crate) fn detect_format(path: &Path) -> ExecutorResult<ArrowFileFormat> {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("parquet") => Ok(ArrowFileFormat::Parquet),
        Some("csv") => Ok(ArrowFileFormat::Csv),
        Some("json" | "jsonl") => Ok(ArrowFileFormat::Jsonl),
        Some(extension) => Err(invalid_input(
            "invalid_argument.executor.arrow_format",
            format!(
                "unrecognized file extension '.{extension}'; expected .parquet, .csv, .jsonl, or .json"
            ),
        )),
        None => Err(invalid_input(
            "invalid_argument.executor.arrow_format",
            "file has no extension; specify the Arrow file format",
        )),
    }
}
