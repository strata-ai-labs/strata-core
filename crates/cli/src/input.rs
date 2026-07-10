//! CLI input helpers.

use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use base64::Engine as _;
use serde_json::Value;
use strata_executor::{Bytes, VectorMetadataFilter};

use crate::CliError;

pub(crate) fn bytes_argument(
    value: Option<&str>,
    file: Option<&PathBuf>,
) -> Result<Bytes, CliError> {
    Ok(Bytes::new(read_bytes_argument(value, file)?))
}

/// Decodes a `--cursor` continuation token.
///
/// KV cursors are opaque base64 tokens — the exact string printed by the
/// previous page (and the `Bytes` wire encoding). Decoding here restores the
/// underlying key bytes, so continuation works for non-UTF-8 keys too.
pub(crate) fn cursor_argument(value: &str) -> Result<Bytes, CliError> {
    base64::engine::general_purpose::STANDARD
        .decode(value)
        .map(Bytes::new)
        .map_err(|error| {
            CliError::usage(format!(
                "invalid --cursor `{value}`: expected the base64 token printed by the previous page ({error})"
            ))
        })
}

pub(crate) fn text_argument(
    value: Option<&str>,
    file: Option<&PathBuf>,
    label: &str,
) -> Result<String, CliError> {
    let bytes = read_bytes_argument(value, file)?;
    String::from_utf8(bytes).map_err(|error| {
        CliError::usage(format!(
            "{label} must be valid UTF-8 when read as text: {error}"
        ))
    })
}

pub(crate) fn parse_relaxed_json_argument(
    value: Option<&str>,
    file: Option<&PathBuf>,
    label: &str,
) -> Result<Value, CliError> {
    let text = text_argument(value, file, label)?;
    Ok(serde_json::from_str(&text).unwrap_or(Value::String(text)))
}

pub(crate) fn parse_json_argument(
    value: Option<&str>,
    file: Option<&PathBuf>,
    label: &str,
) -> Result<Value, CliError> {
    let text = text_argument(value, file, label)?;
    serde_json::from_str(&text).map_err(CliError::from)
}

pub(crate) fn parse_optional_json_argument(
    value: Option<&str>,
    file: Option<&PathBuf>,
    label: &str,
) -> Result<Option<Value>, CliError> {
    if value.is_none() && file.is_none() {
        return Ok(None);
    }
    parse_json_argument(value, file, label).map(Some)
}

pub(crate) fn parse_vector_argument(
    value: Option<&str>,
    file: Option<&PathBuf>,
    label: &str,
) -> Result<Vec<f32>, CliError> {
    let text = text_argument(value, file, label)?;
    parse_vector_text(&text)
}

pub(crate) fn parse_filter_argument(
    value: Option<&str>,
    file: Option<&PathBuf>,
) -> Result<VectorMetadataFilter, CliError> {
    let text = text_argument(value, file, "vector metadata filter")?;
    serde_json::from_str(&text).map_err(CliError::from)
}

pub(crate) fn parse_optional_filter_argument(
    value: Option<&str>,
    file: Option<&PathBuf>,
) -> Result<Option<VectorMetadataFilter>, CliError> {
    if value.is_none() && file.is_none() {
        return Ok(None);
    }
    parse_filter_argument(value, file).map(Some)
}

pub(crate) fn read_text_file(path: &Path) -> Result<String, CliError> {
    fs::read_to_string(path).map_err(CliError::from)
}

fn read_bytes_argument(value: Option<&str>, file: Option<&PathBuf>) -> Result<Vec<u8>, CliError> {
    match (value, file) {
        (Some(_), Some(_)) => Err(CliError::usage(
            "provide either an inline value or --file, not both",
        )),
        (Some(value), None) => read_inline_or_file_shorthand(value),
        (None, Some(path)) => fs::read(path).map_err(CliError::from),
        (None, None) => Err(CliError::usage(
            "missing required value; pass a value or --file",
        )),
    }
}

fn read_inline_or_file_shorthand(value: &str) -> Result<Vec<u8>, CliError> {
    if value == "-" {
        let mut bytes = Vec::new();
        io::stdin().read_to_end(&mut bytes)?;
        return Ok(bytes);
    }
    if let Some(path) = value.strip_prefix('@') {
        if path.is_empty() {
            return Err(CliError::usage("@ file shorthand requires a path"));
        }
        return fs::read(path).map_err(CliError::from);
    }
    Ok(value.as_bytes().to_vec())
}

fn parse_vector_text(value: &str) -> Result<Vec<f32>, CliError> {
    if value.trim_start().starts_with('[') {
        return serde_json::from_str(value).map_err(CliError::from);
    }
    value
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(|part| {
            part.parse::<f32>().map_err(|error| {
                CliError::usage(format!("invalid vector element `{part}`: {error}"))
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_comma_vector() {
        assert_eq!(
            parse_vector_text("1, 2.5,3").expect("parse vector"),
            vec![1.0, 2.5, 3.0]
        );
    }

    #[test]
    fn parses_json_vector() {
        assert_eq!(
            parse_vector_text("[1,2,3]").expect("parse vector"),
            vec![1.0, 2.0, 3.0]
        );
    }

    #[test]
    fn cursor_argument_decodes_the_printed_base64_token() {
        assert_eq!(
            cursor_argument("YQ==").expect("valid cursor"),
            Bytes::new(b"a".to_vec())
        );
    }

    #[test]
    fn cursor_argument_rejects_non_base64_input() {
        let error = cursor_argument("not base64!").expect_err("invalid cursor");
        assert!(matches!(error, CliError::Usage(_)));
    }
}
