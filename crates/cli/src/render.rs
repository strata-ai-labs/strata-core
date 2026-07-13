//! CLI response rendering.

use base64::Engine as _;
use serde::Serialize;
use serde_json::Value;
use strata_executor::Output;

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
            "event_count" => print_count(data),
            "kv_versioned_value" => print_optional_data(data),
            "vector_data" | "event_record" | "graph_node_result" | "graph_edge_result" => {
                print_optional_record(data)?;
            }
            "json_value" | "json_versioned_value" => print_maybe_json(kind, data)?,
            "json_version_history" => print_bare_items(data),
            "vector_matches" => print_matches_data(data),
            "inference_generation" => print_inference_generation(data, true),
            "inference_text" => println!("{}", data.as_str().unwrap_or_default()),
            "inference_token_ids" => print_token_ids(data),
            "inference_embeddings" => print_embeddings_summary(data),
            "inference_ranking" => print_ranking(data),
            "inference_models" => print_inference_models(data),
            "inference_model_pulled" => print_model_pulled(data),
            "inference_unload_result" => println!(
                "{}",
                if data.get("unloaded").and_then(Value::as_bool) == Some(true) {
                    "unloaded"
                } else {
                    "no cached entry"
                }
            ),
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
    let (kind, data) = tagged_output(value).unwrap_or(("", value));

    if data.is_null() {
        return;
    }

    match kind {
        "json_value" | "json_versioned_value" => {
            let found = data.get("found").and_then(Value::as_bool).unwrap_or(false);
            if let Some(leaf) = json_leaf(kind, data, found) {
                println!("{}", raw_scalar(leaf));
            }
            return;
        }
        "kv_versioned_value" => {
            // The KV record nests the stored value under its own `value` field.
            if let Some(value) = point_read_record(data).and_then(|record| record.get("value")) {
                println!("{}", raw_scalar(value));
            }
            return;
        }
        "json_version_history" => {
            if let Some(items) = data.as_array() {
                print_items(items);
            }
            return;
        }
        "vector_matches" => {
            print_matches_data(data);
            return;
        }
        "event_count" => {
            print_count(data);
            return;
        }
        "inference_generation" => {
            print_inference_generation(data, false);
            return;
        }
        "inference_text" => {
            println!("{}", data.as_str().unwrap_or_default());
            return;
        }
        "inference_token_ids" => {
            print_token_ids(data);
            return;
        }
        "inference_embeddings" => {
            print_embedding_values(data);
            return;
        }
        _ => {}
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

/// Prints generated text; with `stats` a trailing summary line follows so the
/// human can see why generation stopped (raw mode prints the text alone).
fn print_inference_generation(data: &Value, stats: bool) {
    let choice = data
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first());
    let text = choice
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
        .and_then(Value::as_str)
        .unwrap_or("");
    println!("{text}");
    if stats {
        let stop = choice
            .and_then(|choice| choice.get("finish_reason"))
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let usage = data.get("usage");
        let prompt = usage
            .and_then(|usage| usage.get("prompt_tokens"))
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let completion = usage
            .and_then(|usage| usage.get("completion_tokens"))
            .and_then(Value::as_u64)
            .unwrap_or(0);
        println!("-- stop: {stop} · prompt {prompt} tok · completion {completion} tok");
    }
}

fn print_token_ids(data: &Value) {
    let ids = data
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_u64)
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default();
    println!("{ids}");
}

/// Prints raw embedding vectors, one line per input, values space-joined.
fn print_embedding_values(data: &Value) {
    let items = data
        .get("data")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for item in &items {
        let values = item
            .get("embedding")
            .and_then(Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(Value::as_f64)
                    .map(|value| value.to_string())
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_default();
        println!("{values}");
    }
}

fn print_embeddings_summary(data: &Value) {
    let dimension = data.get("dimension").and_then(Value::as_u64).unwrap_or(0);
    let items = data
        .get("data")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    println!("{} embeddings · dim {dimension}", items.len());
    for item in &items {
        let index = item.get("index").and_then(Value::as_u64).unwrap_or(0);
        let preview = item.get("embedding").and_then(Value::as_array).map_or_else(
            || "[]".to_string(),
            |values| {
                let head = values
                    .iter()
                    .take(6)
                    .filter_map(Value::as_f64)
                    .map(|value| format!("{value:.4}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                let ellipsis = if values.len() > 6 { ", …" } else { "" };
                format!("[{head}{ellipsis}]")
            },
        );
        println!("  [{index}] {preview}");
    }
}

fn print_ranking(data: &Value) {
    let Some(items) = data.get("items").and_then(Value::as_array) else {
        println!("(nil)");
        return;
    };
    let mut scored: Vec<(u64, f64)> = items
        .iter()
        .filter(|item| item.get("status").and_then(Value::as_str) == Some("ok"))
        .filter_map(|item| {
            Some((
                item.get("index").and_then(Value::as_u64)?,
                item.get("score").and_then(Value::as_f64)?,
            ))
        })
        .collect();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    for (index, score) in scored {
        println!("{index}\t{score:.6}");
    }
    for item in items {
        if item.get("status").and_then(Value::as_str) == Some("error") {
            let code = item.get("code").and_then(Value::as_str).unwrap_or("error");
            println!("failed: {code}");
        }
    }
}

fn print_inference_models(data: &Value) {
    const MIB: u64 = 1_048_576;
    let Some(items) = data.get("items").and_then(Value::as_array) else {
        println!("(nil)");
        return;
    };
    if items.is_empty() {
        println!("(none)");
        return;
    }
    for item in items {
        let text = |key: &str| {
            item.get(key)
                .and_then(Value::as_str)
                .unwrap_or("-")
                .to_owned()
        };
        let local = if item.get("is_local").and_then(Value::as_bool) == Some(true) {
            "local"
        } else {
            "remote"
        };
        let size = item.get("size_bytes").and_then(Value::as_u64).map_or_else(
            || "-".to_owned(),
            |bytes| format!("{}.{} MB", bytes / MIB, (bytes % MIB) * 10 / MIB),
        );
        println!(
            "{}\t{}\t{}\t{}\t{}\t{}",
            text("name"),
            text("task"),
            text("architecture"),
            text("default_quant"),
            local,
            size
        );
    }
}

fn print_model_pulled(data: &Value) {
    let model = data.get("model").and_then(Value::as_str).unwrap_or("model");
    let path = data.get("path").and_then(Value::as_str).unwrap_or("-");
    println!("pulled {model} -> {path}");
}

/// Unwraps a `{found, value}` point-read envelope to its record, or `None`
/// when the record is absent.
fn point_read_record(data: &Value) -> Option<&Value> {
    match data.get("found").and_then(Value::as_bool) {
        Some(true) => data.get("value"),
        _ => None,
    }
}

fn print_optional_data(data: &Value) {
    // KV point reads answer with a {found, value} envelope whose record nests
    // the stored value under its own `value` field.
    match point_read_record(data) {
        None => println!("(nil)"),
        Some(record) => match record.get("value") {
            Some(value) => println!("{}", scalar_summary(value)),
            None => println!("{}", scalar_summary(record)),
        },
    }
}

fn print_optional_record(data: &Value) -> Result<(), CliError> {
    // Vector/event/graph point reads share the envelope but carry structured
    // records; show the record itself, or `(nil)` when absent.
    match point_read_record(data) {
        None => println!("(nil)"),
        Some(record) => render_human_data(record)?,
    }
    Ok(())
}

fn print_maybe_json(kind: &str, data: &Value) -> Result<(), CliError> {
    let Some(found) = data.get("found").and_then(Value::as_bool) else {
        println!("{}", serde_json::to_string_pretty(data)?);
        return Ok(());
    };
    match json_leaf(kind, data, found) {
        // Human output shows the JSON encoding of the leaf value so `"null"`
        // vs `null` and strings vs numbers stay unambiguous; raw output
        // unwraps strings for scripting.
        Some(leaf) => println!("{}", serde_json::to_string(leaf)?),
        None => println!("(nil)"),
    }
    Ok(())
}

/// Extracts the leaf JSON value from a maybe-json envelope. The
/// `json_versioned_value` shape nests the document value inside commit facts
/// (`{found, value: {value, version, timestamp, document_version}}`), so the
/// leaf sits one level deeper than in `json_value`.
fn json_leaf<'a>(kind: &str, data: &'a Value, found: bool) -> Option<&'a Value> {
    if !found {
        return None;
    }
    let value = data.get("value")?;
    if kind == "json_versioned_value" {
        value.get("value")
    } else {
        Some(value)
    }
}

/// `json_version_history` serializes as a bare item array (or null when the
/// document never existed), unlike the `{count, items}` KV history envelope.
fn print_bare_items(data: &Value) {
    if let Value::Array(items) = data {
        print_items(items);
    } else {
        println!("(nil)");
    }
}

/// `vector_matches` serializes its match list as the bare `data` array, so the
/// tabular key/score renderer must be dispatched by tag.
fn print_matches_data(data: &Value) {
    match data.as_array() {
        Some(items) => print_vector_matches(items),
        None => println!("(empty)"),
    }
}

/// `event_count` wraps its count in `{count}`; humans and scripts get the
/// bare number, matching how `kv count` (a plain `uint`) renders.
fn print_count(data: &Value) {
    println!(
        "{}",
        data.get("count").map_or_else(String::new, scalar_summary)
    );
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
        // The stored value sits inside the {found, value} point-read envelope,
        // one level below `data`.
        Output::KvVersionedValue(_) => {
            if let Some(record) = data.get_mut("value") {
                decode_bytes_fields(record, &["value"]);
            }
        }
        Output::VersionHistory(_) => decode_bytes_item_fields(data, &["value"]),
        Output::KeysPage { .. } => {
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

// Errors teach (first-run D4): the human line carries the code, the message,
// the actionable hint, and the stable per-code docs ref, so a human or agent
// can self-correct without a docs round-trip.
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
    let mut rendered = if reference.is_empty() {
        format!("{code}: {message}")
    } else {
        format!("{code}: {message} ({reference})")
    };
    if let Some(hint) = value
        .get("suggested_fix")
        .and_then(Value::as_str)
        .filter(|hint| !hint.trim().is_empty())
    {
        rendered.push_str("\n  hint: ");
        rendered.push_str(hint);
    }
    if let Some(docs) = value
        .get("docs_url")
        .and_then(Value::as_str)
        .filter(|docs| !docs.trim().is_empty())
    {
        rendered.push_str("\n  ref: ");
        rendered.push_str(docs);
    }
    rendered
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use strata_executor::{
        Bytes, CommitReceipt, Maybe, MutationEffect, Output, PageInfo, ScanItem, VersionedValue,
    };

    use super::humanize_kv_bytes;

    fn bytes(text: &str) -> Bytes {
        Bytes::new(text.as_bytes().to_vec())
    }

    #[test]
    fn kv_value_decodes_to_text_for_human_output() {
        let output =
            Output::KvVersionedValue(Maybe::found(VersionedValue::new(bytes("one"), 1, 10)));
        let mut value = serde_json::to_value(&output).expect("output serializes");
        assert_eq!(value["data"]["value"]["value"], json!("b25l"));
        humanize_kv_bytes(&output, &mut value);
        assert_eq!(value["data"]["value"]["value"], json!("one"));
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
        let output = Output::KvVersionedValue(Maybe::found(VersionedValue::new(
            Bytes::new(vec![0xff, 0xfe]),
            1,
            10,
        )));
        let mut value = serde_json::to_value(&output).expect("output serializes");
        humanize_kv_bytes(&output, &mut value);
        assert_eq!(value["data"]["value"]["value"], json!("//4="));
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
        let output = Output::KvVersionedValue(Maybe::missing());
        let mut value = serde_json::to_value(&output).expect("output serializes");
        humanize_kv_bytes(&output, &mut value);
        assert_eq!(value["data"], json!({ "found": false, "value": null }));
    }

    #[test]
    fn json_leaf_unwraps_the_versioned_envelope() {
        let data = json!({
            "found": true,
            "value": {"value": {"name": "Ada"}, "version": 3, "timestamp": 30, "document_version": 1}
        });
        assert_eq!(
            super::json_leaf("json_versioned_value", &data, true),
            Some(&json!({"name": "Ada"}))
        );
    }

    #[test]
    fn json_leaf_reads_plain_values_directly_and_respects_found() {
        let data = json!({"found": true, "value": null});
        assert_eq!(
            super::json_leaf("json_value", &data, true),
            Some(&serde_json::Value::Null)
        );
        assert_eq!(super::json_leaf("json_value", &data, false), None);
    }
}
