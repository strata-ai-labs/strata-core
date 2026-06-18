//! Arrow schema mapping and row coercion.

use arrow::array::{self, Array};
use arrow::datatypes::{DataType, Schema};
use arrow::record_batch::RecordBatch;
use base64::Engine;
use serde_json::Value;

use crate::error::ExecutorResult;
use crate::types::ArrowImportTarget;

use super::{internal_error, invalid_input};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ImportMapping {
    pub(crate) key_idx: usize,
    pub(crate) value_idx: Option<usize>,
    pub(crate) extra_indices: Vec<usize>,
    pub(crate) extra_names: Vec<String>,
    pub(crate) key_encoding_idx: Option<usize>,
    pub(crate) value_encoding_idx: Option<usize>,
}

pub(crate) fn resolve_mapping(
    schema: &Schema,
    target: ArrowImportTarget,
    key_column: Option<&str>,
    value_column: Option<&str>,
) -> ExecutorResult<ImportMapping> {
    let key_idx = resolve_key_column(schema, key_column)?;
    let (value_idx, extra_indices, extra_names) = match target {
        ArrowImportTarget::Kv => resolve_kv_value(schema, key_idx, value_column)?,
        ArrowImportTarget::Json => resolve_json_document(schema, key_idx, value_column)?,
        ArrowImportTarget::Vector => resolve_vector_embedding(schema, key_idx, value_column)?,
    };
    let key_encoding_idx = schema.index_of("key_encoding").ok();
    let value_encoding_idx = schema.index_of("value_encoding").ok();
    Ok(ImportMapping {
        key_idx,
        value_idx,
        extra_indices,
        extra_names,
        key_encoding_idx,
        value_encoding_idx,
    })
}

pub(crate) fn key_bytes(
    batch: &RecordBatch,
    mapping: &ImportMapping,
    row: usize,
) -> ExecutorResult<Option<Vec<u8>>> {
    let key_col = batch.column(mapping.key_idx);
    if key_col.is_null(row) {
        return Ok(None);
    }
    let encoding = encoding_at(batch, mapping.key_encoding_idx, row);
    cell_to_bytes(key_col.as_ref(), row, encoding.as_deref()).map(Some)
}

pub(crate) fn value_bytes(
    batch: &RecordBatch,
    mapping: &ImportMapping,
    row: usize,
) -> ExecutorResult<Vec<u8>> {
    if let Some(value_idx) = mapping.value_idx {
        let value_col = batch.column(value_idx);
        let encoding = encoding_at(batch, mapping.value_encoding_idx, row);
        return cell_to_bytes(value_col.as_ref(), row, encoding.as_deref());
    }
    let value = row_to_json_object(batch, row, &mapping.extra_indices, &mapping.extra_names)?;
    serde_json::to_vec(&value).map_err(|error| {
        internal_error(format!(
            "failed to serialize Arrow row as JSON bytes: {error}"
        ))
    })
}

pub(crate) fn json_document(
    batch: &RecordBatch,
    mapping: &ImportMapping,
    row: usize,
) -> ExecutorResult<Value> {
    if let Some(value_idx) = mapping.value_idx {
        return cell_to_json_document(batch.column(value_idx).as_ref(), row);
    }
    row_to_json_object(batch, row, &mapping.extra_indices, &mapping.extra_names)
}

pub(crate) fn vector_embedding(
    batch: &RecordBatch,
    mapping: &ImportMapping,
    row: usize,
) -> Option<Vec<f32>> {
    let value_idx = mapping.value_idx?;
    let column = batch.column(value_idx);
    extract_embedding(column.as_ref(), row)
}

pub(crate) fn vector_metadata(
    batch: &RecordBatch,
    mapping: &ImportMapping,
    row: usize,
) -> ExecutorResult<Option<Value>> {
    if mapping.extra_indices.is_empty() {
        return Ok(None);
    }
    Ok(Some(row_to_json_object(
        batch,
        row,
        &mapping.extra_indices,
        &mapping.extra_names,
    )?))
}

pub(crate) fn row_to_json_object(
    batch: &RecordBatch,
    row: usize,
    indices: &[usize],
    names: &[String],
) -> ExecutorResult<Value> {
    let mut map = serde_json::Map::new();
    for (index, name) in indices.iter().zip(names) {
        let column = batch.column(*index);
        map.insert(name.clone(), cell_to_json(column.as_ref(), row)?);
    }
    Ok(Value::Object(map))
}

fn resolve_key_column(schema: &Schema, key_column: Option<&str>) -> ExecutorResult<usize> {
    if let Some(column) = key_column {
        return schema.index_of(column).map_err(|_| {
            invalid_input(
                "invalid_argument.executor.arrow_key_column",
                format!(
                    "key column '{column}' not found; available columns: {}",
                    format_columns(schema)
                ),
            )
        });
    }
    for candidate in ["key", "_id", "id"] {
        if let Ok(index) = schema.index_of(candidate) {
            return Ok(index);
        }
    }
    Err(invalid_input(
        "invalid_argument.executor.arrow_key_column",
        format!(
            "no key column found; available columns: {}",
            format_columns(schema)
        ),
    ))
}

fn resolve_kv_value(
    schema: &Schema,
    key_idx: usize,
    value_column: Option<&str>,
) -> ExecutorResult<(Option<usize>, Vec<usize>, Vec<String>)> {
    if let Some(column) = value_column {
        let value_idx = schema.index_of(column).map_err(|_| {
            invalid_input(
                "invalid_argument.executor.arrow_value_column",
                format!(
                    "value column '{column}' not found; available columns: {}",
                    format_columns(schema)
                ),
            )
        })?;
        return Ok(resolve_extras(schema, &[key_idx, value_idx]));
    }
    if let Ok(value_idx) = schema.index_of("value") {
        return Ok(resolve_extras(schema, &[key_idx, value_idx]));
    }
    if schema.fields().len() == 2 {
        let value_idx = usize::from(key_idx == 0);
        return Ok((Some(value_idx), Vec::new(), Vec::new()));
    }
    let (extra_indices, extra_names) = collect_extras(schema, &[key_idx]);
    Ok((None, extra_indices, extra_names))
}

fn resolve_json_document(
    schema: &Schema,
    key_idx: usize,
    value_column: Option<&str>,
) -> ExecutorResult<(Option<usize>, Vec<usize>, Vec<String>)> {
    if let Some(column) = value_column {
        let value_idx = schema.index_of(column).map_err(|_| {
            invalid_input(
                "invalid_argument.executor.arrow_value_column",
                format!(
                    "document column '{column}' not found; available columns: {}",
                    format_columns(schema)
                ),
            )
        })?;
        return Ok(resolve_extras(schema, &[key_idx, value_idx]));
    }
    for candidate in ["document", "value", "doc", "body"] {
        if let Ok(value_idx) = schema.index_of(candidate) {
            return Ok(resolve_extras(schema, &[key_idx, value_idx]));
        }
    }
    let (extra_indices, extra_names) = collect_extras(schema, &[key_idx]);
    Ok((None, extra_indices, extra_names))
}

fn resolve_vector_embedding(
    schema: &Schema,
    key_idx: usize,
    value_column: Option<&str>,
) -> ExecutorResult<(Option<usize>, Vec<usize>, Vec<String>)> {
    let value_idx = if let Some(column) = value_column {
        schema.index_of(column).map_err(|_| {
            invalid_input(
                "invalid_argument.executor.arrow_value_column",
                format!(
                    "embedding column '{column}' not found; available columns: {}",
                    format_columns(schema)
                ),
            )
        })?
    } else {
        ["embedding", "vector", "embeddings", "emb"]
            .iter()
            .find_map(|candidate| schema.index_of(candidate).ok())
            .ok_or_else(|| {
                invalid_input(
                    "invalid_argument.executor.arrow_value_column",
                    format!(
                        "no embedding column found; available columns: {}",
                        format_columns(schema)
                    ),
                )
            })?
    };
    let field = schema.field(value_idx);
    match field.data_type() {
        DataType::FixedSizeList(inner, _) | DataType::List(inner) => match inner.data_type() {
            DataType::Float32 | DataType::Float64 => {}
            data_type => {
                return Err(invalid_input(
                    "invalid_argument.executor.arrow_embedding_type",
                    format!(
                        "embedding column '{}' has inner type {data_type}; expected Float32 or Float64",
                        field.name()
                    ),
                ));
            }
        },
        data_type => {
            return Err(invalid_input(
                "invalid_argument.executor.arrow_embedding_type",
                format!(
                    "embedding column '{}' has type {data_type}; expected a float list",
                    field.name()
                ),
            ));
        }
    }
    Ok(resolve_extras(schema, &[key_idx, value_idx]))
}

fn resolve_extras(schema: &Schema, exclude: &[usize]) -> (Option<usize>, Vec<usize>, Vec<String>) {
    let value_idx = exclude.last().copied();
    let (extra_indices, extra_names) = collect_extras(schema, exclude);
    (value_idx, extra_indices, extra_names)
}

fn collect_extras(schema: &Schema, exclude: &[usize]) -> (Vec<usize>, Vec<String>) {
    let mut indices = Vec::new();
    let mut names = Vec::new();
    for (index, field) in schema.fields().iter().enumerate() {
        let ignored = exclude.contains(&index)
            || matches!(
                field.name().as_str(),
                "key_encoding" | "value_encoding" | "version" | "timestamp"
            );
        if !ignored {
            indices.push(index);
            names.push(field.name().clone());
        }
    }
    (indices, names)
}

fn format_columns(schema: &Schema) -> String {
    schema
        .fields()
        .iter()
        .map(|field| format!("{} ({})", field.name(), field.data_type()))
        .collect::<Vec<_>>()
        .join(", ")
}

fn encoding_at(batch: &RecordBatch, index: Option<usize>, row: usize) -> Option<String> {
    index
        .and_then(|index| cell_to_string(batch.column(index).as_ref(), row).ok())
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
}

fn cell_to_bytes(
    column: &dyn Array,
    row: usize,
    encoding: Option<&str>,
) -> ExecutorResult<Vec<u8>> {
    if column.is_null(row) {
        return Ok(Vec::new());
    }
    if matches!(encoding, Some("base64")) {
        let encoded = cell_to_string(column, row)?;
        return base64::engine::general_purpose::STANDARD
            .decode(encoded.as_bytes())
            .map_err(|error| {
                invalid_input(
                    "invalid_argument.executor.arrow_base64",
                    format!("failed to decode base64 Arrow cell: {error}"),
                )
            });
    }
    match column.data_type() {
        DataType::Binary => {
            let array = column
                .as_any()
                .downcast_ref::<array::BinaryArray>()
                .unwrap();
            Ok(array.value(row).to_vec())
        }
        DataType::LargeBinary => {
            let array = column
                .as_any()
                .downcast_ref::<array::LargeBinaryArray>()
                .unwrap();
            Ok(array.value(row).to_vec())
        }
        _ => Ok(cell_to_string(column, row)?.into_bytes()),
    }
}

fn cell_to_json_document(column: &dyn Array, row: usize) -> ExecutorResult<Value> {
    if column.is_null(row) {
        return Ok(Value::Null);
    }
    match column.data_type() {
        DataType::Utf8 | DataType::LargeUtf8 => {
            let text = cell_to_string(column, row)?;
            Ok(serde_json::from_str(&text).unwrap_or(Value::String(text)))
        }
        _ => cell_to_json(column, row),
    }
}

fn cell_to_json(column: &dyn Array, row: usize) -> ExecutorResult<Value> {
    if column.is_null(row) {
        return Ok(Value::Null);
    }
    match column.data_type() {
        DataType::Int8 => {
            let array = column.as_any().downcast_ref::<array::Int8Array>().unwrap();
            Ok(serde_json::json!(array.value(row)))
        }
        DataType::Int16 => {
            let array = column.as_any().downcast_ref::<array::Int16Array>().unwrap();
            Ok(serde_json::json!(array.value(row)))
        }
        DataType::Int32 => {
            let array = column.as_any().downcast_ref::<array::Int32Array>().unwrap();
            Ok(serde_json::json!(array.value(row)))
        }
        DataType::Int64 => {
            let array = column.as_any().downcast_ref::<array::Int64Array>().unwrap();
            Ok(serde_json::json!(array.value(row)))
        }
        DataType::UInt8 => {
            let array = column.as_any().downcast_ref::<array::UInt8Array>().unwrap();
            Ok(serde_json::json!(array.value(row)))
        }
        DataType::UInt16 => {
            let array = column
                .as_any()
                .downcast_ref::<array::UInt16Array>()
                .unwrap();
            Ok(serde_json::json!(array.value(row)))
        }
        DataType::UInt32 => {
            let array = column
                .as_any()
                .downcast_ref::<array::UInt32Array>()
                .unwrap();
            Ok(serde_json::json!(array.value(row)))
        }
        DataType::UInt64 => {
            let array = column
                .as_any()
                .downcast_ref::<array::UInt64Array>()
                .unwrap();
            Ok(serde_json::json!(array.value(row)))
        }
        DataType::Float32 => {
            let array = column
                .as_any()
                .downcast_ref::<array::Float32Array>()
                .unwrap();
            Ok(serde_json::json!(array.value(row)))
        }
        DataType::Float64 => {
            let array = column
                .as_any()
                .downcast_ref::<array::Float64Array>()
                .unwrap();
            Ok(serde_json::json!(array.value(row)))
        }
        DataType::Boolean => {
            let array = column
                .as_any()
                .downcast_ref::<array::BooleanArray>()
                .unwrap();
            Ok(Value::Bool(array.value(row)))
        }
        DataType::Binary => {
            let array = column
                .as_any()
                .downcast_ref::<array::BinaryArray>()
                .unwrap();
            Ok(Value::String(
                base64::engine::general_purpose::STANDARD.encode(array.value(row)),
            ))
        }
        DataType::LargeBinary => {
            let array = column
                .as_any()
                .downcast_ref::<array::LargeBinaryArray>()
                .unwrap();
            Ok(Value::String(
                base64::engine::general_purpose::STANDARD.encode(array.value(row)),
            ))
        }
        _ => Ok(Value::String(cell_to_string(column, row)?)),
    }
}

fn cell_to_string(column: &dyn Array, row: usize) -> ExecutorResult<String> {
    if column.is_null(row) {
        return Ok(String::new());
    }
    match column.data_type() {
        DataType::Utf8 => {
            let array = column
                .as_any()
                .downcast_ref::<array::StringArray>()
                .unwrap();
            Ok(array.value(row).to_owned())
        }
        DataType::LargeUtf8 => {
            let array = column
                .as_any()
                .downcast_ref::<array::LargeStringArray>()
                .unwrap();
            Ok(array.value(row).to_owned())
        }
        _ => {
            let formatter = arrow::util::display::ArrayFormatter::try_new(
                column,
                &arrow::util::display::FormatOptions::default(),
            )
            .map_err(|error| {
                internal_error(format!("failed to format Arrow cell as string: {error}"))
            })?;
            Ok(formatter.value(row).to_string())
        }
    }
}

fn extract_embedding(column: &dyn Array, row: usize) -> Option<Vec<f32>> {
    if column.is_null(row) {
        return None;
    }
    let values = match column.data_type() {
        DataType::FixedSizeList(_, _) => {
            let array = column
                .as_any()
                .downcast_ref::<array::FixedSizeListArray>()?;
            array.value(row)
        }
        DataType::List(_) => {
            let array = column.as_any().downcast_ref::<array::ListArray>()?;
            array.value(row)
        }
        _ => return None,
    };
    if let Some(array) = values.as_any().downcast_ref::<array::Float32Array>() {
        if (0..array.len()).any(|index| array.is_null(index)) {
            return None;
        }
        return Some(array.values().to_vec());
    }
    values
        .as_any()
        .downcast_ref::<array::Float64Array>()
        .and_then(|array| {
            if (0..array.len()).any(|index| array.is_null(index)) {
                return None;
            }
            array
                .values()
                .iter()
                .copied()
                .map(f64_embedding_value_to_f32)
                .collect()
        })
}

#[allow(clippy::cast_possible_truncation)]
fn f64_embedding_value_to_f32(value: f64) -> Option<f32> {
    if value.is_finite() && value >= f64::from(f32::MIN) && value <= f64::from(f32::MAX) {
        Some(value as f32)
    } else {
        None
    }
}
