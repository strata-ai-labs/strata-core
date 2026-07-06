//! CLI response rendering.

use serde::Serialize;
use serde_json::Value;
use strata_executor_next::Output;

use crate::options::Format;
use crate::CliError;

pub(crate) fn render_output(output: &Output, format: Format) -> Result<(), CliError> {
    let value = serde_json::to_value(output)?;
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
            "kv_value" | "kv_versioned_value" => print_optional_data(data),
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

fn scalar_summary(value: &Value) -> String {
    match value {
        Value::Null => "(nil)".to_owned(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        Value::String(value) => value.clone(),
        Value::Array(values) => bytes_from_json_array(values)
            .and_then(|bytes| String::from_utf8(bytes).ok())
            .unwrap_or_else(|| {
                serde_json::to_string(value).unwrap_or_else(|_| "<array>".to_owned())
            }),
        Value::Object(_) => serde_json::to_string(value).unwrap_or_else(|_| "<object>".to_owned()),
    }
}

fn raw_scalar(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        Value::Array(values) => bytes_from_json_array(values)
            .and_then(|bytes| String::from_utf8(bytes).ok())
            .unwrap_or_else(|| serde_json::to_string(value).unwrap_or_default()),
        Value::Null => String::new(),
        Value::Bool(_) | Value::Number(_) => value.to_string(),
        Value::Object(_) => serde_json::to_string(value).unwrap_or_default(),
    }
}

fn bytes_from_json_array(values: &[Value]) -> Option<Vec<u8>> {
    values
        .iter()
        .map(|value| value.as_u64().and_then(|byte| u8::try_from(byte).ok()))
        .collect()
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
