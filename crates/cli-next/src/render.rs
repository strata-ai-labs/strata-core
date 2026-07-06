//! CLI response rendering.

use base64::Engine as _;
use serde::Serialize;
use serde_json::Value;
use strata_executor_next::Output;

use crate::options::Format;
use crate::CliError;

pub(crate) fn render_output(output: &Output, format: Format) -> Result<(), CliError> {
    let mut value = serde_json::to_value(output)?;
    // Human and raw formats show KV keys/values as text when possible. The
    // decode happens here — with the typed `Output` in hand — so only fields
    // the schema declares as `Bytes` are touched (see `humanize_kv_bytes`).
    // JSON and pretty formats stay wire-true (base64).
    if matches!(format, Format::Human | Format::Raw) {
        humanize_kv_bytes(output, &mut value);
    }
    render_value(&value, format)
}

pub(crate) fn render_value(value: &Value, format: Format) -> Result<(), CliError> {
    match format {
        Format::Json => println!("{}", serde_json::to_string(value)?),
        Format::Pretty => println!("{}", serde_json::to_string_pretty(value)?),
        Format::Human => render_human(value)?,
        Format::Raw => render_raw(value),
    }
    Ok(())
}

pub(crate) fn render_error(status: &impl Serialize, format: Format) {
    #[derive(Serialize)]
    struct ErrorEnvelope<'a, T: Serialize + ?Sized> {
        error: &'a T,
    }

    let envelope = ErrorEnvelope { error: status };
    match format {
        Format::Json => render_serialized_error(serde_json::to_string(&envelope)),
        Format::Pretty => render_serialized_error(serde_json::to_string_pretty(&envelope)),
        Format::Human | Format::Raw => match serde_json::to_value(status) {
            Ok(value) => eprintln!("{}", human_error_line(&value)),
            Err(error) => eprintln!("error: failed to render executor error: {error}"),
        },
    }
}

fn render_serialized_error(rendered: Result<String, serde_json::Error>) {
    match rendered {
        Ok(text) => eprintln!("{text}"),
        Err(error) => eprintln!("error: failed to render executor error: {error}"),
    }
}

fn render_human(value: &Value) -> Result<(), CliError> {
    if let Some((kind, data)) = tagged_output(value) {
        match kind {
            "pong" => println!(
                "pong {}",
                data.get("version").and_then(Value::as_str).unwrap_or("")
            ),
            "bool" | "uint" => println!("{}", scalar_summary(data)),
            "kv_versioned_value" => print_optional_data(data),
            "json_value" | "json_versioned_value" => print_maybe_json(data)?,
            _ => render_human_data(data)?,
        }
        return Ok(());
    }

    render_human_data(value)
}

fn render_human_data(data: &Value) -> Result<(), CliError> {
    if data.is_null() {
        println!("(nil)");
        return Ok(());
    }

    if let Some(items) = data.get("items").and_then(Value::as_array) {
        print_items(items);
        print_page_tail(data);
        return Ok(());
    }

    if let Some(items) = data.get("matches").and_then(Value::as_array) {
        print_vector_matches(items);
        return Ok(());
    }

    if let Some(found) = data.get("found").and_then(Value::as_bool) {
        if !found {
            println!("(nil)");
            return Ok(());
        }
        if let Some(value) = data.get("value") {
            println!("{}", scalar_summary(value));
            return Ok(());
        }
    }

    if let Some(effect) = data.get("effect") {
        println!("{}", mutation_summary(data, effect));
        return Ok(());
    }

    match data {
        Value::Bool(_) | Value::Number(_) | Value::String(_) => {
            println!("{}", scalar_summary(data));
        }
        _ => println!("{}", serde_json::to_string_pretty(data)?),
    }
    Ok(())
}

fn render_raw(value: &Value) {
    let data = tagged_output(value).map_or(value, |(_, data)| data);

    if data.is_null() {
        return;
    }

    if let Some(items) = data.get("items").and_then(Value::as_array) {
        print_items(items);
        return;
    }

    if let Some(items) = data.get("matches").and_then(Value::as_array) {
        print_vector_matches(items);
        return;
    }

    if data.get("effect").is_some() {
        return;
    }

    if let Some(found) = data.get("found").and_then(Value::as_bool) {
        if !found {
            return;
        }
        if let Some(value) = data.get("value") {
            println!("{}", raw_scalar(value));
            return;
        }
    }

    if let Some(value) = data.get("value") {
        println!("{}", raw_scalar(value));
        return;
    }

    println!("{}", raw_scalar(data));
}

fn tagged_output(value: &Value) -> Option<(&str, &Value)> {
    let object = value.as_object()?;
    let kind = object.get("type")?.as_str()?;
    let data = object.get("data").unwrap_or(&Value::Null);
    Some((kind, data))
}

fn print_optional_data(data: &Value) {
    if data.is_null() {
        println!("(nil)");
        return;
    }
    if let Some(value) = data.get("value") {
        println!("{}", scalar_summary(value));
        return;
    }
    println!("{}", scalar_summary(data));
}

fn print_maybe_json(data: &Value) -> Result<(), CliError> {
    let Some(found) = data.get("found").and_then(Value::as_bool) else {
        println!("{}", serde_json::to_string_pretty(data)?);
        return Ok(());
    };
    if !found {
        println!("(nil)");
        return Ok(());
    }
    if let Some(value) = data.get("value") {
        println!("{}", scalar_summary(value));
    } else {
        println!("(nil)");
    }
    Ok(())
}

fn print_items(items: &[Value]) {
    for item in items {
        println!("{}", scalar_summary(item));
    }
    if items.is_empty() {
        println!("(empty)");
    }
}

fn print_page_tail(data: &Value) {
    if data.get("has_more").and_then(Value::as_bool) == Some(true) {
        if let Some(cursor) = data.get("cursor") {
            println!("-- more: {}", scalar_summary(cursor));
        }
    }
}

fn print_vector_matches(items: &[Value]) {
    for item in items {
        let key = item
            .get("key")
            .map_or_else(|| scalar_summary(item), scalar_summary);
        let score = item
            .get("score")
            .or_else(|| item.get("distance"))
            .map(scalar_summary)
            .unwrap_or_default();
        if score.is_empty() {
            println!("{key}");
        } else {
            println!("{key}\t{score}");
        }
    }
    if items.is_empty() {
        println!("(empty)");
    }
}

fn mutation_summary(data: &Value, effect: &Value) -> String {
    let kind = effect.get("kind").and_then(Value::as_str).unwrap_or("ok");
    let applied = effect
        .get("applied")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let subject = data
        .get("key")
        .or_else(|| data.get("collection"))
        .or_else(|| data.get("space"))
        .or_else(|| data.get("graph"))
        .map(scalar_summary)
        .unwrap_or_default();
    if subject.is_empty() {
        format!("{kind} applied={applied}")
    } else {
        format!("{kind} {subject} applied={applied}")
    }
}

/// Rewrites schema-declared `Bytes` fields from base64 to readable text for
/// human/raw output.
///
/// Driven by the typed `Output` variant, never by value shape, so a genuine
/// string that merely looks like base64 is never touched — the defect that
/// retired the old integer-array heuristic. Fields whose bytes are not valid
/// UTF-8 keep their base64 form. Continuation cursors deliberately stay
/// base64: they are opaque tokens that `--cursor` accepts verbatim.
fn humanize_kv_bytes(output: &Output, value: &mut Value) {
    let Some(data) = value.get_mut("data") else {
        return;
    };
    match output {
        Output::KvVersionedValue(_) => decode_bytes_fields(data, &["value"]),
        Output::VersionHistory(_) => decode_bytes_item_fields(data, &["value"]),
        Output::Keys { .. } | Output::KeysPage { .. } => {
            if let Some(items) = data.get_mut("items").and_then(Value::as_array_mut) {
                for item in items {
                    decode_bytes_value(item);
                }
            }
        }
        Output::KvScanResult { .. } | Output::SampleResult { .. } => {
            decode_bytes_item_fields(data, &["key", "value"]);
        }
        Output::WriteResult { .. } | Output::DeleteResult { .. } => {
            decode_bytes_fields(data, &["key"]);
        }
        // Batch outputs also carry Bytes but are not reachable from any CLI
        // verb today; their base64 form is still correct if that changes.
        _ => {}
    }
}

fn decode_bytes_item_fields(data: &mut Value, fields: &[&str]) {
    if let Some(items) = data.get_mut("items").and_then(Value::as_array_mut) {
        for item in items {
            decode_bytes_fields(item, fields);
        }
    }
}

fn decode_bytes_fields(object: &mut Value, fields: &[&str]) {
    for field in fields {
        if let Some(value) = object.get_mut(*field) {
            decode_bytes_value(value);
        }
    }
}

fn decode_bytes_value(value: &mut Value) {
    let Value::String(encoded) = value else {
        return;
    };
    if let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(encoded.as_str()) {
        if let Ok(text) = String::from_utf8(decoded) {
            *value = Value::String(text);
        }
    }
}

// `Bytes` fields arrive as canonical base64 strings (DSGN-5/DTO-2). The typed
// pre-pass above (`humanize_kv_bytes`) decodes the fields the schema declares
// as bytes; by the time values reach these untyped helpers there is nothing
// left to decode, and a schema-blind decode here would corrupt genuine strings
// that merely look like base64. Arrays are always genuine JSON arrays and
// render as JSON.
fn scalar_summary(value: &Value) -> String {
    match value {
        Value::Null => "(nil)".to_owned(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        Value::String(value) => value.clone(),
        Value::Array(_) => serde_json::to_string(value).unwrap_or_else(|_| "<array>".to_owned()),
        Value::Object(_) => serde_json::to_string(value).unwrap_or_else(|_| "<object>".to_owned()),
    }
}

fn raw_scalar(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        Value::Array(_) | Value::Object(_) => serde_json::to_string(value).unwrap_or_default(),
        Value::Null => String::new(),
        Value::Bool(_) | Value::Number(_) => value.to_string(),
    }
}

fn human_error_line(value: &Value) -> String {
    let code = value.get("code").and_then(Value::as_str).unwrap_or("error");
    let message = value
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("command failed");
    let reference = value
        .get("reference_id")
        .and_then(Value::as_str)
        .unwrap_or("");
    if reference.is_empty() {
        format!("{code}: {message}")
    } else {
        format!("{code}: {message} ({reference})")
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use strata_executor_next::{
        Bytes, CommitReceipt, MutationEffect, Output, PageInfo, ScanItem, VersionedValue,
    };

    use super::humanize_kv_bytes;

    fn bytes(text: &str) -> Bytes {
        Bytes::new(text.as_bytes().to_vec())
    }

    #[test]
    fn kv_value_decodes_to_text_for_human_output() {
        let output = Output::KvVersionedValue(Some(VersionedValue::new(bytes("one"), 1, 10)));
        let mut value = serde_json::to_value(&output).expect("output serializes");
        assert_eq!(value["data"]["value"], json!("b25l"));
        humanize_kv_bytes(&output, &mut value);
        assert_eq!(value["data"]["value"], json!("one"));
    }

    #[test]
    fn key_items_decode_but_the_continuation_cursor_stays_base64() {
        let output = Output::KeysPage {
            items: vec![bytes("a")],
            page: PageInfo::new(true, Some(bytes("b"))),
        };
        let mut value = serde_json::to_value(&output).expect("output serializes");
        humanize_kv_bytes(&output, &mut value);
        assert_eq!(value["data"]["items"][0], json!("a"));
        assert_eq!(value["data"]["cursor"], json!("Yg=="));
    }

    #[test]
    fn scan_items_decode_key_and_value() {
        let output = Output::KvScanResult {
            items: vec![ScanItem::new(bytes("a"), bytes("one"), 1, 10)],
            page: PageInfo::terminal(),
        };
        let mut value = serde_json::to_value(&output).expect("output serializes");
        humanize_kv_bytes(&output, &mut value);
        assert_eq!(value["data"]["items"][0]["key"], json!("a"));
        assert_eq!(value["data"]["items"][0]["value"], json!("one"));
    }

    #[test]
    fn non_utf8_bytes_keep_their_base64_form() {
        let output = Output::KvVersionedValue(Some(VersionedValue::new(
            Bytes::new(vec![0xff, 0xfe]),
            1,
            10,
        )));
        let mut value = serde_json::to_value(&output).expect("output serializes");
        humanize_kv_bytes(&output, &mut value);
        assert_eq!(value["data"]["value"], json!("//4="));
    }

    #[test]
    fn write_result_subject_key_decodes() {
        let output = Output::WriteResult {
            key: bytes("user"),
            effect: MutationEffect::created(),
            commit: CommitReceipt::new(1, 10, true, 1, 0),
        };
        let mut value = serde_json::to_value(&output).expect("output serializes");
        humanize_kv_bytes(&output, &mut value);
        assert_eq!(value["data"]["key"], json!("user"));
    }

    #[test]
    fn missing_reads_are_untouched() {
        let output = Output::KvVersionedValue(None);
        let mut value = serde_json::to_value(&output).expect("output serializes");
        humanize_kv_bytes(&output, &mut value);
        assert_eq!(value["data"], serde_json::Value::Null);
    }
}
