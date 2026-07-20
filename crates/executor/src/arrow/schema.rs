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
    pub(crate) metadata_idx: Option<usize>,
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
    let (value_idx, mut extra_indices, mut extra_names) = match target {
        ArrowImportTarget::Kv => resolve_kv_value(schema, key_idx, value_column)?,
        ArrowImportTarget::Json => resolve_json_document(schema, key_idx, value_column)?,
        ArrowImportTarget::Vector => resolve_vector_embedding(schema, key_idx, value_column)?,
    };
    // A vector export writes a designated JSON `metadata` column plus an internal
    // `vector_revision`. Treat metadata as a document (parsed by `vector_metadata`,
    // not re-wrapped as a string) and drop both it and the revision from the
    // generic extras bundle so neither leaks into reconstructed metadata.
    let metadata_idx = if matches!(target, ArrowImportTarget::Vector) {
        let metadata_idx = schema.index_of("metadata").ok();
        remove_extra(&mut extra_indices, &mut extra_names, metadata_idx);
        remove_extra(
            &mut extra_indices,
            &mut extra_names,
            schema.index_of("vector_revision").ok(),
        );
        metadata_idx
    } else {
        None
    };
    // `key_encoding` / `value_encoding` are optional metadata columns; a
    // missing column (`index_of` returns Err) means "no encoding override",
    // not a schema error, so the lookup collapses to `None`.
    let key_encoding_idx = schema.index_of("key_encoding").ok();
    let value_encoding_idx = schema.index_of("value_encoding").ok();
    Ok(ImportMapping {
        key_idx,
        value_idx,
        metadata_idx,
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
    // A designated `metadata` column (as written by `arrow export vector`) is a
    // JSON document: parse it and return it unwrapped, matching what was stored.
    if let Some(metadata_idx) = mapping.metadata_idx {
        let column = batch.column(metadata_idx);
        if column.is_null(row) {
            return Ok(None);
        }
        return Ok(Some(cell_to_json_document(column.as_ref(), row)?));
    }
    // Hand-authored files without a `metadata` column: bundle any stray columns.
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
            // Probe known embedding column aliases; `.ok()` skips names that
            // are absent so the first present alias wins.
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

/// Removes a column from the parallel `(indices, names)` extras vectors, keeping
/// them in sync. Used to pull designated/internal vector columns out of the
/// generic extras bundle.
fn remove_extra(indices: &mut Vec<usize>, names: &mut Vec<String>, target: Option<usize>) {
    let Some(target) = target else {
        return;
    };
    if let Some(position) = indices.iter().position(|&index| index == target) {
        indices.remove(position);
        names.remove(position);
    }
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
        // A cell that cannot be read as a string means "no per-row encoding".
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
        DataType::Null => Ok(Value::Null),
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use arrow::array::{
        BinaryArray, BooleanArray, FixedSizeListBuilder, Float32Array, Float32Builder,
        Float64Array, Float64Builder, Int16Array, Int32Array, Int64Array, Int8Array,
        LargeBinaryArray, LargeStringArray, ListBuilder, NullArray, StringArray, UInt16Array,
        UInt32Array, UInt64Array, UInt8Array,
    };
    use arrow::datatypes::Field;
    use base64::Engine;
    use serde_json::json;

    use crate::ExecutorErrorClass;

    use super::*;

    #[test]
    fn key_value_and_document_columns_follow_old_detection_order() {
        let schema = Schema::new(vec![
            utf8_field("_id"),
            utf8_field("id"),
            utf8_field("key"),
            utf8_field("value"),
        ]);
        let mapping =
            resolve_mapping(&schema, ArrowImportTarget::Kv, None, None).expect("mapping resolves");
        assert_eq!(mapping.key_idx, 2);
        assert_eq!(mapping.value_idx, Some(3));

        let schema = Schema::new(vec![
            utf8_field("_id"),
            utf8_field("id"),
            utf8_field("payload"),
        ]);
        let mapping = resolve_mapping(&schema, ArrowImportTarget::Kv, Some("id"), Some("payload"))
            .expect("explicit mapping resolves");
        assert_eq!(mapping.key_idx, 1);
        assert_eq!(mapping.value_idx, Some(2));

        for candidate in ["document", "value", "doc", "body"] {
            let schema = Schema::new(vec![utf8_field("key"), utf8_field(candidate)]);
            let mapping = resolve_mapping(&schema, ArrowImportTarget::Json, None, None)
                .expect("json mapping resolves");
            assert_eq!(mapping.value_idx, Some(1), "{candidate}");
        }
    }

    #[test]
    fn mapping_errors_include_stable_codes_and_available_columns() {
        let schema = Schema::new(vec![utf8_field("name"), utf8_field("payload")]);
        let error = resolve_mapping(&schema, ArrowImportTarget::Kv, None, None)
            .expect_err("missing key fails");
        assert_eq!(error.class(), ExecutorErrorClass::InvalidInput);
        assert_eq!(error.code(), "invalid_argument.executor.arrow_key_column");
        assert!(error.message().contains("name (Utf8)"));

        let error = resolve_mapping(&schema, ArrowImportTarget::Json, Some("name"), Some("doc"))
            .expect_err("missing document fails");
        assert_eq!(error.code(), "invalid_argument.executor.arrow_value_column");
        assert!(error.message().contains("payload (Utf8)"));
    }

    #[test]
    fn kv_mapping_uses_two_column_shortcut_and_extra_object_fallback() {
        let schema = Schema::new(vec![utf8_field("id"), utf8_field("payload")]);
        let mapping =
            resolve_mapping(&schema, ArrowImportTarget::Kv, None, None).expect("mapping resolves");
        assert_eq!(mapping.key_idx, 0);
        assert_eq!(mapping.value_idx, Some(1));
        assert!(mapping.extra_indices.is_empty());

        let schema = Schema::new(vec![
            utf8_field("id"),
            utf8_field("name"),
            Field::new("active", DataType::Boolean, false),
        ]);
        let mapping =
            resolve_mapping(&schema, ArrowImportTarget::Kv, None, None).expect("mapping resolves");
        assert_eq!(mapping.value_idx, None);
        assert_eq!(mapping.extra_names, vec!["name", "active"]);
    }

    #[test]
    fn bytes_respect_binary_columns_and_exported_base64_encodings() {
        let encoded_key = base64::engine::general_purpose::STANDARD.encode([0, 255]);
        let encoded_value = base64::engine::general_purpose::STANDARD.encode([255, 254]);
        let batch = RecordBatch::try_new(
            Arc::new(Schema::new(vec![
                utf8_field("key"),
                utf8_field("key_encoding"),
                utf8_field("value"),
                utf8_field("value_encoding"),
            ])),
            vec![
                Arc::new(StringArray::from(vec![encoded_key])),
                Arc::new(StringArray::from(vec!["base64"])),
                Arc::new(StringArray::from(vec![encoded_value])),
                Arc::new(StringArray::from(vec!["base64"])),
            ],
        )
        .expect("batch");
        let mapping = resolve_mapping(batch.schema().as_ref(), ArrowImportTarget::Kv, None, None)
            .expect("mapping resolves");
        assert_eq!(
            key_bytes(&batch, &mapping, 0).expect("key"),
            Some(vec![0, 255])
        );
        assert_eq!(
            value_bytes(&batch, &mapping, 0).expect("value"),
            vec![255, 254]
        );

        let batch = RecordBatch::try_new(
            Arc::new(Schema::new(vec![
                Field::new("key", DataType::Binary, false),
                Field::new("value", DataType::LargeBinary, false),
            ])),
            vec![
                Arc::new(BinaryArray::from(vec![b"k".as_slice()])),
                Arc::new(LargeBinaryArray::from(vec![b"raw".as_slice()])),
            ],
        )
        .expect("binary batch");
        let mapping = resolve_mapping(batch.schema().as_ref(), ArrowImportTarget::Kv, None, None)
            .expect("mapping resolves");
        assert_eq!(
            key_bytes(&batch, &mapping, 0).expect("key"),
            Some(b"k".to_vec())
        );
        assert_eq!(
            value_bytes(&batch, &mapping, 0).expect("value"),
            b"raw".to_vec()
        );
    }

    #[test]
    fn json_document_and_object_conversion_cover_arrow_scalars() {
        let batch = scalar_batch();
        let names = batch
            .schema()
            .fields()
            .iter()
            .map(|field| field.name().clone())
            .collect::<Vec<_>>();
        let indices = (0..batch.num_columns()).collect::<Vec<_>>();
        let value = row_to_json_object(&batch, 0, &indices, &names).expect("json object");
        assert_eq!(
            value,
            json!({
                "utf8": "text",
                "large_utf8": "large",
                "binary": "Ymlu",
                "large_binary": "bGFyZ2U=",
                "int8": -8,
                "int16": -16,
                "int32": -32,
                "int64": -64,
                "uint8": 8,
                "uint16": 16,
                "uint32": 32,
                "uint64": 64,
                "float32": 1.5,
                "float64": 2.5,
                "bool": true,
                "null": null,
            })
        );

        let document_batch = RecordBatch::try_new(
            Arc::new(Schema::new(vec![utf8_field("key"), utf8_field("document")])),
            vec![
                Arc::new(StringArray::from(vec!["doc-a", "doc-b"])),
                Arc::new(StringArray::from(vec!["{\"ok\":true}", "not-json"])),
            ],
        )
        .expect("document batch");
        let mapping = resolve_mapping(
            document_batch.schema().as_ref(),
            ArrowImportTarget::Json,
            None,
            None,
        )
        .expect("mapping resolves");
        assert_eq!(
            json_document(&document_batch, &mapping, 0).expect("json document"),
            json!({"ok": true})
        );
        assert_eq!(
            json_document(&document_batch, &mapping, 1).expect("json document"),
            json!("not-json")
        );
    }

    #[test]
    fn vector_mapping_accepts_float_lists_and_rejects_other_embedding_shapes() {
        for candidate in ["embedding", "vector", "embeddings", "emb"] {
            let schema = Schema::new(vec![
                utf8_field("key"),
                Field::new(
                    candidate,
                    DataType::FixedSizeList(
                        Arc::new(Field::new("item", DataType::Float32, true)),
                        2,
                    ),
                    false,
                ),
            ]);
            let mapping = resolve_mapping(&schema, ArrowImportTarget::Vector, None, None)
                .expect("vector mapping resolves");
            assert_eq!(mapping.value_idx, Some(1), "{candidate}");
        }

        let non_list = Schema::new(vec![utf8_field("key"), utf8_field("embedding")]);
        let error = resolve_mapping(&non_list, ArrowImportTarget::Vector, None, None)
            .expect_err("non-list embedding fails");
        assert_eq!(
            error.code(),
            "invalid_argument.executor.arrow_embedding_type"
        );

        let non_float_list = Schema::new(vec![
            utf8_field("key"),
            Field::new(
                "embedding",
                DataType::List(Arc::new(Field::new("item", DataType::Utf8, true))),
                false,
            ),
        ]);
        let error = resolve_mapping(&non_float_list, ArrowImportTarget::Vector, None, None)
            .expect_err("non-float embedding fails");
        assert_eq!(
            error.code(),
            "invalid_argument.executor.arrow_embedding_type"
        );
    }

    #[test]
    fn vector_embeddings_accept_supported_lists_and_skip_invalid_rows() {
        let fixed_float64 = fixed_float64_embedding_batch(&[1.0, 2.0, f64::INFINITY, 4.0]);
        let mapping = resolve_mapping(
            fixed_float64.schema().as_ref(),
            ArrowImportTarget::Vector,
            None,
            None,
        )
        .expect("mapping resolves");
        assert_eq!(
            vector_embedding(&fixed_float64, &mapping, 0).expect("embedding"),
            vec![1.0, 2.0]
        );
        assert!(vector_embedding(&fixed_float64, &mapping, 1).is_none());

        let list_float32 = list_float32_embedding_batch();
        let mapping = resolve_mapping(
            list_float32.schema().as_ref(),
            ArrowImportTarget::Vector,
            None,
            None,
        )
        .expect("mapping resolves");
        assert_eq!(
            vector_embedding(&list_float32, &mapping, 0).expect("embedding"),
            vec![3.0, 4.0]
        );
    }

    #[test]
    fn vector_metadata_parses_the_designated_column_without_leaking_internal_fields() {
        // Mirrors the schema `arrow export vector` writes: a JSON-string
        // `metadata` column plus internal version/timestamp/vector_revision
        // columns. Import must parse `metadata` back into its object and never
        // surface `vector_revision`.
        // Row 0 carries metadata; row 1 has none (export writes a null cell),
        // which must round-trip to `None`, not spurious metadata.
        let mut embedding = FixedSizeListBuilder::new(Float32Builder::new(), 2);
        for value in [1.0, 0.0, 0.0, 1.0] {
            embedding.values().append_value(value);
        }
        embedding.append(true);
        embedding.append(true);
        let batch = RecordBatch::try_new(
            Arc::new(Schema::new(vec![
                utf8_field("key"),
                Field::new(
                    "embedding",
                    DataType::FixedSizeList(
                        Arc::new(Field::new("item", DataType::Float32, true)),
                        2,
                    ),
                    false,
                ),
                Field::new("metadata", DataType::Utf8, true),
                Field::new("version", DataType::UInt64, false),
                Field::new("timestamp", DataType::UInt64, false),
                Field::new("vector_revision", DataType::UInt64, false),
            ])),
            vec![
                Arc::new(StringArray::from(vec!["doc-a", "doc-b"])),
                Arc::new(embedding.finish()),
                Arc::new(StringArray::from(vec![
                    Some("{\"kind\":\"note\",\"rank\":1}"),
                    None,
                ])),
                Arc::new(UInt64Array::from(vec![7_u64, 8])),
                Arc::new(UInt64Array::from(vec![9_u64, 10])),
                Arc::new(UInt64Array::from(vec![1_u64, 2])),
            ],
        )
        .expect("vector batch");
        let mapping = resolve_mapping(
            batch.schema().as_ref(),
            ArrowImportTarget::Vector,
            None,
            None,
        )
        .expect("mapping resolves");
        let metadata = vector_metadata(&batch, &mapping, 0)
            .expect("metadata")
            .expect("metadata present");
        assert_eq!(metadata, json!({"kind": "note", "rank": 1}));
        assert_eq!(
            vector_metadata(&batch, &mapping, 1).expect("metadata"),
            None,
            "a null metadata cell must round-trip to no metadata"
        );
    }

    #[test]
    fn vector_metadata_bundles_user_columns_but_drops_the_internal_revision() {
        // A hand-authored vector file with no designated `metadata` column: the
        // genuine user column is bundled, but the internal `vector_revision` is
        // stripped rather than leaked.
        let mut embedding = FixedSizeListBuilder::new(Float32Builder::new(), 2);
        embedding.values().append_value(1.0);
        embedding.values().append_value(0.0);
        embedding.append(true);
        let batch = RecordBatch::try_new(
            Arc::new(Schema::new(vec![
                utf8_field("key"),
                Field::new(
                    "embedding",
                    DataType::FixedSizeList(
                        Arc::new(Field::new("item", DataType::Float32, true)),
                        2,
                    ),
                    false,
                ),
                utf8_field("note"),
                Field::new("vector_revision", DataType::UInt64, false),
            ])),
            vec![
                Arc::new(StringArray::from(vec!["doc-a"])),
                Arc::new(embedding.finish()),
                Arc::new(StringArray::from(vec!["hello"])),
                Arc::new(UInt64Array::from(vec![1_u64])),
            ],
        )
        .expect("vector batch");
        let mapping = resolve_mapping(
            batch.schema().as_ref(),
            ArrowImportTarget::Vector,
            None,
            None,
        )
        .expect("mapping resolves");
        assert_eq!(
            vector_metadata(&batch, &mapping, 0)
                .expect("metadata")
                .expect("metadata present"),
            json!({"note": "hello"})
        );
    }

    fn utf8_field(name: &str) -> Field {
        Field::new(name, DataType::Utf8, false)
    }

    fn scalar_batch() -> RecordBatch {
        RecordBatch::try_new(
            Arc::new(Schema::new(vec![
                utf8_field("utf8"),
                Field::new("large_utf8", DataType::LargeUtf8, false),
                Field::new("binary", DataType::Binary, false),
                Field::new("large_binary", DataType::LargeBinary, false),
                Field::new("int8", DataType::Int8, false),
                Field::new("int16", DataType::Int16, false),
                Field::new("int32", DataType::Int32, false),
                Field::new("int64", DataType::Int64, false),
                Field::new("uint8", DataType::UInt8, false),
                Field::new("uint16", DataType::UInt16, false),
                Field::new("uint32", DataType::UInt32, false),
                Field::new("uint64", DataType::UInt64, false),
                Field::new("float32", DataType::Float32, false),
                Field::new("float64", DataType::Float64, false),
                Field::new("bool", DataType::Boolean, false),
                Field::new("null", DataType::Null, true),
            ])),
            vec![
                Arc::new(StringArray::from(vec!["text"])),
                Arc::new(LargeStringArray::from(vec!["large"])),
                Arc::new(BinaryArray::from(vec![b"bin".as_slice()])),
                Arc::new(LargeBinaryArray::from(vec![b"large".as_slice()])),
                Arc::new(Int8Array::from(vec![-8])),
                Arc::new(Int16Array::from(vec![-16])),
                Arc::new(Int32Array::from(vec![-32])),
                Arc::new(Int64Array::from(vec![-64])),
                Arc::new(UInt8Array::from(vec![8])),
                Arc::new(UInt16Array::from(vec![16])),
                Arc::new(UInt32Array::from(vec![32])),
                Arc::new(UInt64Array::from(vec![64])),
                Arc::new(Float32Array::from(vec![1.5])),
                Arc::new(Float64Array::from(vec![2.5])),
                Arc::new(BooleanArray::from(vec![true])),
                Arc::new(NullArray::new(1)),
            ],
        )
        .expect("scalar batch")
    }

    fn fixed_float64_embedding_batch(values: &[f64; 4]) -> RecordBatch {
        let mut embedding_builder = FixedSizeListBuilder::new(Float64Builder::new(), 2);
        for value in values {
            embedding_builder.values().append_value(*value);
        }
        embedding_builder.append(true);
        embedding_builder.append(true);
        RecordBatch::try_new(
            Arc::new(Schema::new(vec![
                utf8_field("key"),
                Field::new(
                    "embedding",
                    DataType::FixedSizeList(
                        Arc::new(Field::new("item", DataType::Float64, true)),
                        2,
                    ),
                    false,
                ),
            ])),
            vec![
                Arc::new(StringArray::from(vec!["a", "b"])),
                Arc::new(embedding_builder.finish()),
            ],
        )
        .expect("embedding batch")
    }

    fn list_float32_embedding_batch() -> RecordBatch {
        let mut embedding_builder = ListBuilder::new(Float32Builder::new());
        embedding_builder.values().append_value(3.0);
        embedding_builder.values().append_value(4.0);
        embedding_builder.append(true);
        RecordBatch::try_new(
            Arc::new(Schema::new(vec![
                utf8_field("key"),
                Field::new(
                    "embedding",
                    DataType::List(Arc::new(Field::new("item", DataType::Float32, true))),
                    false,
                ),
            ])),
            vec![
                Arc::new(StringArray::from(vec!["a"])),
                Arc::new(embedding_builder.finish()),
            ],
        )
        .expect("embedding batch")
    }
}
