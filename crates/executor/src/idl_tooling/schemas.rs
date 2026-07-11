//! Per-command JSON Schema emission.
//!
//! Schemas are derived mechanically from the executor DTOs via `schemars`,
//! so they cannot drift from the serde wire format (the strategy forbids a
//! hand-rolled second description of the field contract). Each covered
//! command gets one self-contained document under `generated/schemas/`,
//! keyed by command ID, carrying its request schema, response schema(s),
//! and the transitive `$defs` closure they reference.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{json, Map, Value};

use super::{invalid, CommandIndex, IdlError, Result};
use crate::{Command, Output};

/// Published base URL for schema `$id`s (mirrors the ecosystem publishing
/// plan: the same documents ship in the binary and at stratadb.org).
const SCHEMA_ID_BASE: &str = "https://stratadb.org/idl/v1/schemas";

/// Builds every per-command schema document, keyed by command ID.
pub(super) fn schema_documents(index: &CommandIndex) -> Result<BTreeMap<String, Value>> {
    let command_root = to_value(schemars::schema_for!(Command))?;
    let output_root = to_value(schemars::schema_for!(Output))?;
    let command_variants = variant_schemas(&command_root, "Command")?;
    let output_variants = variant_schemas(&output_root, "Output")?;
    let command_defs = root_defs(&command_root);
    let output_defs = root_defs(&output_root);

    let mut documents = BTreeMap::new();
    for entry in &index.commands {
        let request = variant_for(&command_variants, &entry.input, "Command")?;
        let mut responses = Vec::new();
        for output_ref in &entry.outputs {
            responses.push(variant_for(&output_variants, output_ref, "Output")?.clone());
        }
        let response = variant_for(&output_variants, &entry.output, "Output")?;

        let mut refs = BTreeSet::new();
        collect_refs(request, &mut refs);
        for schema in &responses {
            collect_refs(schema, &mut refs);
        }
        let defs = defs_closure(&command_defs, &output_defs, refs, &entry.id)?;

        let mut document = Map::new();
        document.insert(
            "$schema".into(),
            json!("https://json-schema.org/draft/2020-12/schema"),
        );
        document.insert(
            "$id".into(),
            json!(format!("{SCHEMA_ID_BASE}/{}.json", entry.id)),
        );
        document.insert("command".into(), json!(entry.id));
        document.insert("request".into(), request.clone());
        document.insert("response".into(), response.clone());
        if entry.outputs.len() > 1 {
            document.insert("responses".into(), Value::Array(responses));
        }
        if !defs.is_empty() {
            document.insert("$defs".into(), Value::Object(defs));
        }
        documents.insert(entry.id.clone(), Value::Object(document));
    }
    Ok(documents)
}

/// Serializes one schema document with stable formatting.
pub(super) fn to_schema_json(document: &Value) -> Result<String> {
    let mut json = serde_json::to_string_pretty(document).map_err(|source| IdlError::Json {
        path: std::path::PathBuf::from("schema document"),
        source,
    })?;
    json.push('\n');
    Ok(json)
}

/// Validates every checked-in fixture against its command's schemas.
///
/// Serde round-trips already prove fixtures match the Rust types; this pass
/// proves the *generated schemas* describe the same wire, which is what
/// catches a wrong manual `JsonSchema` impl or a serde attribute the derive
/// does not understand.
pub(super) fn validate_fixtures(
    repo_root: &std::path::Path,
    index: &CommandIndex,
    documents: &BTreeMap<String, Value>,
) -> Result<()> {
    for entry in &index.commands {
        let document = documents
            .get(&entry.id)
            .ok_or_else(|| invalid(format!("no schema document for `{}`", entry.id)))?;
        let request_fixture = read_fixture(repo_root, &entry.fixtures.request)?;
        validate_one(document, "request", &request_fixture, &entry.id)?;

        for fixture in std::iter::once(&entry.fixtures.response).chain(&entry.fixtures.responses) {
            let response_fixture = read_fixture(repo_root, fixture)?;
            validate_response(document, &response_fixture, &entry.id, fixture)?;
        }
    }
    Ok(())
}

fn read_fixture(repo_root: &std::path::Path, relative: &str) -> Result<Value> {
    let path = repo_root.join(super::FIXTURE_ROOT).join(relative);
    let text = fs_read(&path)?;
    serde_json::from_str(&text).map_err(|source| IdlError::Json { path, source })
}

fn fs_read(path: &std::path::Path) -> Result<String> {
    std::fs::read_to_string(path).map_err(|source| IdlError::Read {
        path: path.to_path_buf(),
        source,
    })
}

/// Validates a fixture against one named sub-schema of a document.
fn validate_one(document: &Value, key: &str, fixture: &Value, id: &str) -> Result<()> {
    let schema = subschema_document(document, key);
    let validator = jsonschema::validator_for(&schema)
        .map_err(|error| invalid(format!("schema for `{id}` {key} does not compile: {error}")))?;
    let mut errors = validator.iter_errors(fixture);
    if let Some(error) = errors.next() {
        return Err(invalid(format!(
            "fixture for `{id}` does not validate against its {key} schema: {error} at {}",
            error.instance_path()
        )));
    }
    Ok(())
}

/// A response fixture must validate against at least one declared output.
fn validate_response(
    document: &Value,
    fixture: &Value,
    id: &str,
    fixture_name: &str,
) -> Result<()> {
    let mut schemas = vec![subschema_document(document, "response")];
    if let Some(Value::Array(extra)) = document.get("responses") {
        for schema in extra {
            schemas.push(with_defs(document, schema.clone()));
        }
    }
    for schema in &schemas {
        let validator = jsonschema::validator_for(schema).map_err(|error| {
            invalid(format!(
                "response schema for `{id}` does not compile: {error}"
            ))
        })?;
        if validator.is_valid(fixture) {
            return Ok(());
        }
    }
    Err(invalid(format!(
        "response fixture `{fixture_name}` for `{id}` does not validate against any declared output schema"
    )))
}

/// Lifts a named sub-schema into a standalone document sharing the root `$defs`.
fn subschema_document(document: &Value, key: &str) -> Value {
    with_defs(document, document.get(key).cloned().unwrap_or(Value::Null))
}

fn with_defs(document: &Value, schema: Value) -> Value {
    let mut root = match schema {
        Value::Object(map) => map,
        other => {
            let mut map = Map::new();
            map.insert("$ref".into(), other);
            map
        }
    };
    if let Some(defs) = document.get("$defs") {
        root.insert("$defs".into(), defs.clone());
    }
    Value::Object(root)
}

fn to_value(schema: schemars::Schema) -> Result<Value> {
    serde_json::to_value(schema).map_err(|source| IdlError::Json {
        path: std::path::PathBuf::from("derived schema"),
        source,
    })
}

fn root_defs(root: &Value) -> Map<String, Value> {
    root.get("$defs")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default()
}

/// Maps wire tag -> variant schema for a tagged enum root.
fn variant_schemas(root: &Value, enum_name: &str) -> Result<BTreeMap<String, Value>> {
    let variants = root
        .get("oneOf")
        .or_else(|| root.get("anyOf"))
        .and_then(Value::as_array)
        .ok_or_else(|| {
            invalid(format!(
                "derived `{enum_name}` schema has no oneOf variants"
            ))
        })?;
    let mut map = BTreeMap::new();
    for variant in variants {
        let tag = variant
            .pointer("/properties/type/const")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .or_else(|| {
                variant
                    .pointer("/properties/type/enum/0")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
            .ok_or_else(|| {
                invalid(format!(
                    "a `{enum_name}` variant schema carries no `type` tag: {variant}"
                ))
            })?;
        if map.insert(tag.clone(), variant.clone()).is_some() {
            return Err(invalid(format!("duplicate `{enum_name}` wire tag `{tag}`")));
        }
    }
    Ok(map)
}

/// Looks up the variant schema for an overlay reference like `Command::KvPut`.
fn variant_for<'a>(
    variants: &'a BTreeMap<String, Value>,
    reference: &str,
    enum_name: &str,
) -> Result<&'a Value> {
    let prefix = format!("{enum_name}::");
    let variant = reference.strip_prefix(&prefix).ok_or_else(|| {
        invalid(format!(
            "reference `{reference}` does not name a `{enum_name}` variant"
        ))
    })?;
    let tag = snake_case(variant);
    variants.get(&tag).ok_or_else(|| {
        invalid(format!(
            "reference `{reference}` (wire tag `{tag}`) is not a `{enum_name}` variant on the current wire"
        ))
    })
}

/// serde `rename_all = "snake_case"` for variant identifiers.
fn snake_case(ident: &str) -> String {
    let mut out = String::with_capacity(ident.len() + 4);
    for (position, ch) in ident.char_indices() {
        if ch.is_uppercase() {
            if position > 0 {
                out.push('_');
            }
            out.extend(ch.to_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

/// Collects `#/$defs/...` reference names transitively reachable from `value`.
fn collect_refs(value: &Value, out: &mut BTreeSet<String>) {
    match value {
        Value::Object(map) => {
            if let Some(Value::String(reference)) = map.get("$ref") {
                if let Some(name) = reference.strip_prefix("#/$defs/") {
                    out.insert(name.to_owned());
                }
            }
            for nested in map.values() {
                collect_refs(nested, out);
            }
        }
        Value::Array(items) => {
            for nested in items {
                collect_refs(nested, out);
            }
        }
        _ => {}
    }
}

/// Resolves the transitive `$defs` closure across both enum roots.
fn defs_closure(
    command_defs: &Map<String, Value>,
    output_defs: &Map<String, Value>,
    initial: BTreeSet<String>,
    id: &str,
) -> Result<Map<String, Value>> {
    let mut pending: Vec<String> = initial.into_iter().collect();
    let mut resolved: Map<String, Value> = Map::new();
    while let Some(name) = pending.pop() {
        if resolved.contains_key(&name) {
            continue;
        }
        let (schema, other) = match (command_defs.get(&name), output_defs.get(&name)) {
            (Some(schema), other) => (schema, other),
            (None, Some(schema)) => (schema, None),
            (None, None) => {
                return Err(invalid(format!(
                    "`{id}` references `$defs/{name}` which neither enum root defines"
                )))
            }
        };
        if let Some(other) = other {
            if other != schema {
                return Err(invalid(format!(
                    "`$defs/{name}` differs between the Command and Output roots"
                )));
            }
        }
        let mut nested = BTreeSet::new();
        collect_refs(schema, &mut nested);
        pending.extend(nested);
        resolved.insert(name, schema.clone());
    }
    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::snake_case;

    #[test]
    fn snake_case_matches_serde_rename_all() {
        assert_eq!(snake_case("KvPut"), "kv_put");
        assert_eq!(snake_case("GraphWcc"), "graph_wcc");
        assert_eq!(snake_case("ConfigureGetKey"), "configure_get_key");
        assert_eq!(snake_case("Ping"), "ping");
    }
}
