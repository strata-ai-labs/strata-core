//! Canonical command examples: load, validate, render, and execute.
//!
//! Each command may declare one example at `idl/v1/examples/<id>.yaml` as a
//! language-neutral **step list** — an ordered sequence of command calls with
//! named arguments and an optional expected result. From that one source the
//! docs generator renders CLI + wire example tabs (SDKs render their own
//! calls from the same spec), and `verify-examples` replays every step against
//! a scratch cache executor so a stale example fails CI. Coverage is tracked
//! by a shrink-only `missing-examples.yaml` allowlist, mirroring
//! `uncovered-commands.yaml`.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use base64::Engine as _;
use serde::{Deserialize, Deserializer};
use serde_json::{Map, Value};

use super::{invalid, read_yaml, CommandIndex, IdlError, ResolvedCommand, Result};
use crate::executor::Executor;
use crate::Command;

const EXAMPLES_SUBDIR: &str = "examples";
const MISSING_EXAMPLES_FILE: &str = "missing-examples.yaml";

/// Rationale for `.expect()` on `write!` into a `String`: the `fmt::Write`
/// impl for `String` never returns `Err`.
const INFALLIBLE: &str = "writing to a String is infallible";

/// A resolved example: an optional caption plus the ordered steps.
pub(super) struct Example {
    pub caption: Option<String>,
    pub steps: Vec<ExampleStep>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExampleSource {
    #[serde(default)]
    caption: Option<String>,
    steps: Vec<ExampleStep>,
}

/// One step: a command call with named args, an optional expected result, and
/// an optional rendered comment.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ExampleStep {
    pub call: String,
    #[serde(default)]
    pub args: Map<String, Value>,
    #[serde(default, deserialize_with = "deserialize_expected")]
    pub returns: Option<ExpectedResult>,
    #[serde(default)]
    pub note: Option<String>,
    /// Optional per-step result-expression override for SDK example renderers
    /// (`{}` is the call). strata-core validates its shape but does not
    /// interpret it — CLI/wire rendering and replay use only the call + args.
    #[serde(default)]
    pub expr: Option<String>,
}

/// A step's declared expectation. An absent `returns` (setup step) maps to
/// `None`; `returns: null` to `Miss`; any other `returns:` value to `Present`.
/// strata-core only asserts miss-ness — the exact value is asserted by the SDK
/// doctest harness, which renders the same spec to its own calls.
pub(super) enum ExpectedResult {
    Miss,
    Present,
}

fn deserialize_expected<'de, D>(
    deserializer: D,
) -> std::result::Result<Option<ExpectedResult>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Some(match Option::<Value>::deserialize(deserializer)? {
        None => ExpectedResult::Miss,
        Some(_) => ExpectedResult::Present,
    }))
}

/// `missing-examples.yaml`: the shrink-only example-coverage allowlist, split
/// by *why* a command has no hermetic example.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MissingExamplesSource {
    /// A hermetic cache-mode example is possible but not yet written.
    #[serde(default)]
    uncovered: Vec<String>,
    /// Covered by a gated integration test or reference-only, because a
    /// cache-mode replay cannot supply the resource (hub, model, API key,
    /// network, machine disk). Not pending.
    #[serde(default)]
    non_hermetic: Vec<String>,
}

fn examples_dir(repo_root: &Path) -> PathBuf {
    repo_root.join(super::IDL_DIR).join(EXAMPLES_SUBDIR)
}

/// Loads every `examples/<id>.yaml`, keyed by command id (the file stem).
pub(super) fn load_examples(repo_root: &Path) -> Result<BTreeMap<String, Example>> {
    let dir = examples_dir(repo_root);
    let mut examples = BTreeMap::new();
    if !dir.exists() {
        return Ok(examples);
    }
    for entry in fs::read_dir(&dir).map_err(|source| IdlError::Read {
        path: dir.clone(),
        source,
    })? {
        let entry = entry.map_err(|source| IdlError::Read {
            path: dir.clone(),
            source,
        })?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("yaml") {
            return Err(invalid(format!(
                "examples dir contains a non-YAML file: {}",
                path.display()
            )));
        }
        let id = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .ok_or_else(|| invalid(format!("example file has no stem: {}", path.display())))?
            .to_owned();
        let source: ExampleSource = read_yaml(&path)?;
        if source.steps.is_empty() {
            return Err(invalid(format!("example `{id}` has no steps")));
        }
        examples.insert(
            id,
            Example {
                caption: source.caption,
                steps: source.steps,
            },
        );
    }
    Ok(examples)
}

/// Validates every example against the schemas and enforces coverage: each
/// example file names a real command, each step calls a real command with
/// valid args, and every catalog command has an example or is allowlisted.
pub(super) fn validate_examples(
    repo_root: &Path,
    index: &CommandIndex,
    schemas: &BTreeMap<String, Value>,
    examples: &BTreeMap<String, Example>,
) -> Result<()> {
    let ids: BTreeSet<&str> = index.commands.iter().map(|c| c.id.as_str()).collect();
    for (id, example) in examples {
        if !ids.contains(id.as_str()) {
            return Err(invalid(format!(
                "examples/{id}.yaml does not name a known command"
            )));
        }
        for (position, step) in example.steps.iter().enumerate() {
            let schema = schemas.get(&step.call).ok_or_else(|| {
                invalid(format!(
                    "example `{id}` step {position} calls unknown command `{}`",
                    step.call
                ))
            })?;
            validate_step_args(id, position, step, schema)?;
        }
    }
    enforce_example_coverage(repo_root, &ids, examples)
}

fn validate_step_args(id: &str, position: usize, step: &ExampleStep, schema: &Value) -> Result<()> {
    let props = request_properties(schema).ok_or_else(|| {
        invalid(format!(
            "example `{id}` step {position}: `{}` has no request properties",
            step.call
        ))
    })?;
    for name in step.args.keys() {
        if name == "type" {
            return Err(invalid(format!(
                "example `{id}` step {position}: must not set the wire discriminator `type`"
            )));
        }
        if !props.contains_key(name) {
            return Err(invalid(format!(
                "example `{id}` step {position}: `{}` has no argument `{name}`",
                step.call
            )));
        }
    }
    for required in schema_required(schema) {
        if required != "type" && !step.args.contains_key(required) {
            return Err(invalid(format!(
                "example `{id}` step {position}: `{}` is missing required argument `{required}`",
                step.call
            )));
        }
    }
    if let Some(expr) = &step.expr {
        if !expr.contains("{}") {
            return Err(invalid(format!(
                "example `{id}` step {position}: `expr` must contain the `{{}}` call placeholder"
            )));
        }
    }
    Ok(())
}

/// Every command must have an example file or appear in `missing-examples.yaml`.
/// The allowlist may only shrink: a listed command that gains an example must
/// be removed, an unknown listed command is rejected, and a new command with
/// neither an example nor a listing fails the build.
fn enforce_example_coverage(
    repo_root: &Path,
    ids: &BTreeSet<&str>,
    examples: &BTreeMap<String, Example>,
) -> Result<()> {
    let allowlist: MissingExamplesSource =
        read_yaml(&repo_root.join(super::IDL_DIR).join(MISSING_EXAMPLES_FILE))?;
    let mut listed = BTreeSet::new();
    for id in allowlist.uncovered.iter().chain(allowlist.non_hermetic.iter()) {
        if !ids.contains(id.as_str()) {
            return Err(invalid(format!(
                "{MISSING_EXAMPLES_FILE} lists `{id}` which is not a command"
            )));
        }
        if examples.contains_key(id) {
            return Err(invalid(format!(
                "`{id}` has an example; remove it from {MISSING_EXAMPLES_FILE} (the allowlist may only shrink)"
            )));
        }
        if !listed.insert(id.as_str()) {
            return Err(invalid(format!(
                "duplicate `{id}` in {MISSING_EXAMPLES_FILE}"
            )));
        }
    }
    for id in ids {
        if !examples.contains_key(*id) && !listed.contains(id) {
            return Err(invalid(format!(
                "command `{id}` has no example and is not listed in {MISSING_EXAMPLES_FILE}; add examples/{id}.yaml or list it"
            )));
        }
    }
    Ok(())
}

/// Replays every example against a scratch cache executor: each step must
/// execute, a `returns: null` step must produce a miss, and a step with a
/// non-null `returns` must produce a value. Exact-value assertions live in the
/// SDK doctest harness, which renders the same spec to its own calls.
pub(super) fn verify_examples(
    repo_root: &Path,
    index: &CommandIndex,
    schemas: &BTreeMap<String, Value>,
) -> Result<()> {
    let examples = load_examples(repo_root)?;
    validate_examples(repo_root, index, schemas, &examples)?;
    for (id, example) in &examples {
        // A per-example scratch dir backs `{tmpdir}` file-path placeholders (a
        // round-trip export/import writes and reads within one example).
        let tmpdir = tempfile::tempdir()
            .map_err(|error| invalid(format!("example `{id}`: scratch dir failed: {error}")))?;
        let tmpdir_path = tmpdir.path().to_string_lossy();
        let mut executor = Executor::open_cache().map_err(|error| {
            invalid(format!("example `{id}`: scratch executor failed: {error}"))
        })?;
        for (position, step) in example.steps.iter().enumerate() {
            let schema = schemas
                .get(&step.call)
                .ok_or_else(|| invalid(format!("example `{id}`: no schema for `{}`", step.call)))?;
            let wire = step_wire_json(id, position, step, schema, &tmpdir_path)?;
            let command: Command =
                serde_json::from_value(wire).map_err(|source| IdlError::Json {
                    path: PathBuf::from(format!("examples/{id}.yaml")),
                    source,
                })?;
            let output = executor.execute(command).map_err(|error| {
                invalid(format!(
                    "example `{id}` step {position} (`{}`) failed to execute: {error}",
                    step.call
                ))
            })?;
            if let Some(expected) = &step.returns {
                let value = serde_json::to_value(&output).map_err(|source| IdlError::Json {
                    path: PathBuf::from(format!("examples/{id}.yaml")),
                    source,
                })?;
                let expect_miss = matches!(expected, ExpectedResult::Miss);
                assert_result(id, position, step, expect_miss, &value)?;
            }
        }
    }
    Ok(())
}

/// Whether a point-read output represents a miss. The canonical `Maybe`
/// envelope serializes absence as `{found: false, value: null}` (absence never
/// aliases a bare `null`), so `data.found == false` is the authoritative flag;
/// a bare-null `data` is treated as a miss too.
fn is_miss(output: &Value) -> bool {
    match output.get("data") {
        Some(Value::Object(map)) => matches!(map.get("found"), Some(Value::Bool(false))),
        Some(Value::Null) => true,
        _ => false,
    }
}

fn assert_result(
    id: &str,
    position: usize,
    step: &ExampleStep,
    expect_miss: bool,
    output: &Value,
) -> Result<()> {
    let miss = is_miss(output);
    if expect_miss && !miss {
        return Err(invalid(format!(
            "example `{id}` step {position} (`{}`): declared `returns: null` but the call returned a value",
            step.call
        )));
    }
    if !expect_miss && miss {
        return Err(invalid(format!(
            "example `{id}` step {position} (`{}`): declared a value but the call returned a miss",
            step.call
        )));
    }
    Ok(())
}

/// Representative scratch directory shown in rendered docs wherever a spec
/// uses the `{tmpdir}` path placeholder; the replay substitutes a real
/// per-example temp dir instead.
const DOC_TMPDIR: &str = "/tmp/exports";

/// Substitutes the `{tmpdir}` placeholder in string arg values (at any depth)
/// with `tmpdir`. File-path arguments are written as `{tmpdir}/<file>` so the
/// replay targets a real scratch dir and the docs render a representative one.
fn resolve_tmpdir(value: &Value, tmpdir: &str) -> Value {
    match value {
        Value::String(text) if text.contains("{tmpdir}") => {
            Value::String(text.replace("{tmpdir}", tmpdir))
        }
        Value::Array(items) => {
            Value::Array(items.iter().map(|item| resolve_tmpdir(item, tmpdir)).collect())
        }
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, item)| (key.clone(), resolve_tmpdir(item, tmpdir)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// Builds the wire command JSON for a step (`{ "type": <tag>, <args> }`),
/// base64-encoding arguments whose schema type is the `Bytes` newtype and
/// substituting the `{tmpdir}` path placeholder with `tmpdir`.
fn step_wire_json(
    id: &str,
    position: usize,
    step: &ExampleStep,
    schema: &Value,
    tmpdir: &str,
) -> Result<Value> {
    let props = request_properties(schema).ok_or_else(|| {
        invalid(format!(
            "example `{id}` step {position}: `{}` has no request properties",
            step.call
        ))
    })?;
    let tag = props
        .get("type")
        .and_then(|t| t.get("const"))
        .and_then(Value::as_str)
        .ok_or_else(|| {
            invalid(format!(
                "example `{id}` step {position}: `{}` has no wire tag",
                step.call
            ))
        })?;
    let defs = schema.get("$defs").and_then(Value::as_object);
    let mut object = Map::new();
    object.insert("type".to_owned(), Value::String(tag.to_owned()));
    for (name, value) in &step.args {
        let resolved = resolve_tmpdir(value, tmpdir);
        let encoded = encode_arg(&resolved, props.get(name), defs);
        object.insert(name.clone(), encoded);
    }
    Ok(Value::Object(object))
}

/// Encodes an argument for the wire against its schema, resolving `$ref`s and
/// recursing through arrays, objects, and `anyOf` so a `Bytes` field at any
/// depth (e.g. `entries[].value`, `keys[]`, a nullable `prefix`) is
/// base64-encoded. Non-`Bytes` values pass through unchanged.
fn encode_arg(value: &Value, schema: Option<&Value>, defs: Option<&Map<String, Value>>) -> Value {
    let Some(schema) = schema else {
        return value.clone();
    };
    if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
        let name = reference.rsplit('/').next().unwrap_or(reference);
        if name == "Bytes" {
            return match value {
                Value::String(text) => {
                    Value::String(base64::engine::general_purpose::STANDARD.encode(text.as_bytes()))
                }
                other => other.clone(),
            };
        }
        return match defs.and_then(|d| d.get(name)) {
            Some(target) => encode_arg(value, Some(target), defs),
            None => value.clone(),
        };
    }
    if let Some(arms) = schema.get("anyOf").and_then(Value::as_array) {
        for arm in arms {
            if arm.get("type").and_then(Value::as_str) != Some("null") {
                return encode_arg(value, Some(arm), defs);
            }
        }
        return value.clone();
    }
    if schema_type_is(schema, "array") {
        if let Value::Array(items) = value {
            let item_schema = schema.get("items");
            return Value::Array(
                items
                    .iter()
                    .map(|item| encode_arg(item, item_schema, defs))
                    .collect(),
            );
        }
        return value.clone();
    }
    if schema_type_is(schema, "object") {
        if let (Value::Object(object), Some(props)) =
            (value, schema.get("properties").and_then(Value::as_object))
        {
            let mut out = Map::new();
            for (key, val) in object {
                out.insert(key.clone(), encode_arg(val, props.get(key), defs));
            }
            return Value::Object(out);
        }
        return value.clone();
    }
    value.clone()
}

fn schema_type_is(schema: &Value, wanted: &str) -> bool {
    match schema.get("type") {
        Some(Value::String(name)) => name == wanted,
        Some(Value::Array(names)) => names.iter().any(|value| value.as_str() == Some(wanted)),
        _ => false,
    }
}

/// Renders the `## Examples` section (CLI + wire tabs) for one command, given
/// the whole catalog (steps may call sibling commands) and schemas.
pub(super) fn render_section(
    by_id: &BTreeMap<&str, &ResolvedCommand>,
    schemas: &BTreeMap<String, Value>,
    example: &Example,
) -> String {
    let mut out = String::from("## Examples\n\n");
    if let Some(caption) = &example.caption {
        out.push_str(caption.trim());
        out.push_str("\n\n");
    }

    out.push_str("### CLI\n\n```console\n");
    for step in &example.steps {
        let line = render_cli(by_id, schemas, step);
        let note = step
            .note
            .as_deref()
            .map_or_else(String::new, |n| format!("  # {n}"));
        writeln!(out, "$ {line}{note}").expect(INFALLIBLE);
    }
    out.push_str("```\n\n### Wire\n\n```json\n");
    for step in &example.steps {
        if let Some(schema) = schemas.get(&step.call) {
            if let Ok(wire) = step_wire_json("", 0, step, schema, DOC_TMPDIR) {
                writeln!(out, "{}", compact(&wire)).expect(INFALLIBLE);
            }
        }
    }
    out.push_str("```\n\n");
    out
}

fn render_cli(
    by_id: &BTreeMap<&str, &ResolvedCommand>,
    schemas: &BTreeMap<String, Value>,
    step: &ExampleStep,
) -> String {
    let Some(command) = by_id.get(step.call.as_str()) else {
        return step.call.clone();
    };
    let schema = schemas.get(&step.call);
    // Wire-only commands have no dedicated clap verb; their CLI form is the
    // generic command runner over the exact wire JSON.
    if command.cli.surface != "verb" {
        if let Some(wire) = schema.and_then(|s| step_wire_json("", 0, step, s, DOC_TMPDIR).ok()) {
            return format!("strata command run --command-json '{}'", compact(&wire));
        }
    }
    let mut tokens = vec![String::from("strata")];
    tokens.extend(command.cli.path.iter().cloned());
    if let Some(schema) = schema {
        // Required scalars render as positionals in schema order; the rest as
        // long flags. Faithful for the seeded verb commands; broader arg-layout
        // fidelity is a later CLI-golden-runner slice.
        for name in schema_required(schema) {
            if name == "type" {
                continue;
            }
            if let Some(value) = step.args.get(name) {
                tokens.push(cli_token(&resolve_tmpdir(value, DOC_TMPDIR)));
            }
        }
        let required: BTreeSet<&str> = schema_required(schema).into_iter().collect();
        for (name, value) in &step.args {
            if !required.contains(name.as_str()) {
                let token = cli_token(&resolve_tmpdir(value, DOC_TMPDIR));
                tokens.push(format!("--{} {}", name.replace('_', "-"), token));
            }
        }
    }
    tokens.join(" ")
}

fn cli_token(value: &Value) -> String {
    let text = match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    };
    if text.is_empty() || text.contains(char::is_whitespace) {
        format!("{text:?}")
    } else {
        text
    }
}

fn request_properties(schema: &Value) -> Option<&Map<String, Value>> {
    schema
        .get("request")
        .and_then(|request| request.get("properties"))
        .and_then(Value::as_object)
}

fn schema_required(schema: &Value) -> Vec<&str> {
    schema
        .get("request")
        .and_then(|request| request.get("required"))
        .and_then(Value::as_array)
        .map_or_else(Vec::new, |array| {
            array.iter().filter_map(Value::as_str).collect()
        })
}

fn compact(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "{}".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn is_miss_reads_the_maybe_found_flag_not_a_bare_null() {
        // The canonical point-read envelope: absence is {found:false,...},
        // presence is {found:true,...} — never a bare null.
        assert!(is_miss(
            &json!({"type": "kv_versioned_value", "data": {"found": false, "value": null}})
        ));
        assert!(!is_miss(
            &json!({"type": "kv_versioned_value", "data": {"found": true, "value": {}}})
        ));
        // A status/scalar read (e.g. exists=false) is never a miss.
        assert!(!is_miss(&json!({"type": "kv_bool", "data": false})));
        // A bare-null data still counts as a miss.
        assert!(is_miss(&json!({"data": null})));
    }

    #[test]
    fn encode_arg_base64s_bytes_at_any_depth() {
        let bytes = json!({"$ref": "#/$defs/Bytes"});
        assert_eq!(encode_arg(&json!("a1"), Some(&bytes), None), json!("YTE=")); // base64("a1")
                                                                                 // Non-Bytes args and unknown schemas pass through unchanged.
        assert_eq!(
            encode_arg(&json!(2), Some(&json!({"type": "integer"})), None),
            json!(2)
        );
        assert_eq!(encode_arg(&json!("raw"), None, None), json!("raw"));

        // Array of Bytes (kv.batch_get `keys`).
        let key_list = json!({"type": "array", "items": {"$ref": "#/$defs/Bytes"}});
        assert_eq!(
            encode_arg(&json!(["a1", "a1"]), Some(&key_list), None),
            json!(["YTE=", "YTE="])
        );

        // Bytes nested in an object behind a $ref (kv.batch_put `entries`).
        let mut defs = Map::new();
        defs.insert(
            "Entry".into(),
            json!({"type": "object", "properties": {"key": {"$ref": "#/$defs/Bytes"}}}),
        );
        let entries = json!({"type": "array", "items": {"$ref": "#/$defs/Entry"}});
        assert_eq!(
            encode_arg(&json!([{"key": "a1"}]), Some(&entries), Some(&defs)),
            json!([{"key": "YTE="}])
        );

        // Bytes inside anyOf (nullable `prefix`).
        let nullable = json!({"anyOf": [{"$ref": "#/$defs/Bytes"}, {"type": "null"}]});
        assert_eq!(
            encode_arg(&json!("a1"), Some(&nullable), None),
            json!("YTE=")
        );
    }

    #[test]
    fn deserialize_expected_distinguishes_absent_null_and_value() {
        #[derive(Deserialize)]
        struct Holder {
            #[serde(default, deserialize_with = "deserialize_expected")]
            returns: Option<ExpectedResult>,
        }
        let absent: Holder = serde_yaml::from_str("{}").expect("absent");
        assert!(absent.returns.is_none());
        let miss: Holder = serde_yaml::from_str("returns: null").expect("null");
        assert!(matches!(miss.returns, Some(ExpectedResult::Miss)));
        let present: Holder = serde_yaml::from_str("returns: hello").expect("value");
        assert!(matches!(present.returns, Some(ExpectedResult::Present)));
    }
}
