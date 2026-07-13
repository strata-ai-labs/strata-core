//! Tool-call GBNF grammar synthesis and best-effort tool-call parsing for the
//! local (llama.cpp) provider.
//!
//! Two entry points:
//! - [`tool_call_grammar`] builds a GBNF grammar that forces the model to emit a
//!   JSON tool-call object `{"name": <tool>, "arguments": <args>}`, used for
//!   `tool_choice: required` / a named function. When a single tool is allowed
//!   its `arguments` are constrained to the tool's JSON Schema (reusing
//!   [`crate::grammar::json_schema_to_gbnf`]); when several tools are allowed the
//!   `name` is constrained to the allowed set but `arguments` fall back to a
//!   generic JSON object (see the note below).
//! - [`parse_tool_calls_from_text`] extracts tool calls from generated text —
//!   both the grammar-forced whole-object form and the best-effort
//!   `<tool_call>…</tool_call>` convention used for `tool_choice: auto`.
//!
//! Like [`crate::grammar`], this module is pure Rust (no llama.cpp dependency)
//! and is compiled unconditionally so its unit tests run without the `local`
//! feature; only `provider/local.rs` consumes it.
//!
//! ## Multi-tool argument constraint (v1 limitation)
//!
//! GBNF has a single rule namespace and no include mechanism. Merging several
//! per-tool argument schemas — each produced by
//! [`crate::grammar::json_schema_to_gbnf`] with its own `root` / `obj-N` /
//! primitive rules — into one document would require rewriting rule names to
//! avoid collisions. To stay robust we emit per-tool argument schemas only when
//! exactly one tool is allowed (the named-function and single-tool `required`
//! cases); when several tools are allowed we constrain the tool `name` to the
//! allowed set and accept any JSON object for `arguments`. The tool name is
//! always strictly constrained, so a valid tool is always selected.

use std::fmt::Write as _;

use serde_json::Value;

use crate::wire::{FunctionDef, Tool, ToolCall, ToolCallFunction};

/// Extract the [`FunctionDef`] carried by a [`Tool`].
pub(crate) fn function_of(tool: &Tool) -> &FunctionDef {
    // `Tool` has a single variant today; the irrefutable `let` keeps this
    // clippy-clean and will fail to compile (a deliberate tripwire) if a new
    // tool kind is ever added.
    let Tool::Function { function } = tool;
    function
}

/// Build a GBNF grammar that forces a JSON tool-call object.
///
/// `forced` names a single function to require (`tool_choice: {name}`); when it
/// is `None` every tool in `tools` is allowed (`tool_choice: required`). See the
/// module docs for the per-tool-vs-generic `arguments` behavior.
pub(crate) fn tool_call_grammar(tools: &[Tool], forced: Option<&str>) -> String {
    let specs = allowed_calls(tools, forced);
    if specs.is_empty() {
        // No tools to constrain: accept any JSON object rather than emit an
        // invalid empty grammar. The local provider only calls this with a
        // non-empty tool set, so this is a defensive fallback.
        return crate::grammar::JSON_OBJECT_GRAMMAR.to_string();
    }

    // Typed path: a single allowed tool with a parameter schema constrains its
    // `arguments` precisely (single namespace, no rule collisions).
    if specs.len() == 1 {
        if let Some(schema) = specs[0].1 {
            if let Some((entry, rest)) = schema_args_grammar(schema) {
                let mut out = String::new();
                // Writing to a `String` is infallible.
                let _ = writeln!(out, "root ::= {}", call_body(&specs[0].0, &entry));
                out.push_str(&rest);
                return out;
            }
        }
    }

    // Generic path: constrain the tool `name`(s); `arguments` are any JSON
    // object.
    let names: Vec<String> = specs.into_iter().map(|(name, _)| name).collect();
    generic_call_grammar(&names)
}

/// The set of `(name, parameters)` pairs the grammar may match.
fn allowed_calls<'a>(tools: &'a [Tool], forced: Option<&str>) -> Vec<(String, Option<&'a Value>)> {
    match forced {
        Some(name) => match tools.iter().map(function_of).find(|f| f.name == name) {
            Some(function) => vec![(function.name.clone(), function.parameters.as_ref())],
            // Named a function not in the offered set: still constrain the name,
            // with a generic-object argument since we have no schema for it.
            None => vec![(name.to_string(), None)],
        },
        None => tools
            .iter()
            .map(function_of)
            .map(|f| (f.name.clone(), f.parameters.as_ref()))
            .collect(),
    }
}

/// Convert a tool's `parameters` schema into `(entry_rule, other_rules)` for
/// inlining as the `arguments` value: the entry rule name to reference and the
/// remaining rule block (which already carries its own primitives). Returns
/// `None` if the converter did not emit the expected `root ::= …` header.
fn schema_args_grammar(schema: &Value) -> Option<(String, String)> {
    let full = crate::grammar::json_schema_to_gbnf(schema);
    let first = full.lines().next()?;
    let entry = first.strip_prefix("root ::= ")?.trim().to_string();
    let mut rest = String::new();
    for line in full.lines().skip(1) {
        rest.push_str(line);
        rest.push('\n');
    }
    Some((entry, rest))
}

/// A grammar whose root alternates over one tool-call rule per allowed name,
/// each with a generic JSON object for `arguments`.
fn generic_call_grammar(names: &[String]) -> String {
    let mut out = String::new();
    let call_rules: Vec<String> = (0..names.len()).map(|i| format!("call-{i}")).collect();
    // Writing to a `String` is infallible.
    let _ = writeln!(out, "root ::= {}", call_rules.join(" | "));
    for (i, name) in names.iter().enumerate() {
        let _ = writeln!(out, "call-{i} ::= {}", call_body(name, "object"));
    }
    out.push_str(&json_object_body());
    out
}

/// The body (right-hand side) of a tool-call rule: a JSON object with a `name`
/// literal fixed to `name` and its `arguments` produced by `args_entry`.
fn call_body(name: &str, args_entry: &str) -> String {
    let parts = [
        "\"{\"".to_string(),
        "ws".to_string(),
        gbnf_json_string_literal("name"),
        "ws".to_string(),
        "\":\"".to_string(),
        "ws".to_string(),
        gbnf_json_string_literal(name),
        "ws".to_string(),
        "\",\"".to_string(),
        "ws".to_string(),
        gbnf_json_string_literal("arguments"),
        "ws".to_string(),
        "\":\"".to_string(),
        "ws".to_string(),
        args_entry.to_string(),
        "\"}\"".to_string(),
        "ws".to_string(),
    ];
    parts.join(" ")
}

/// [`crate::grammar::JSON_OBJECT_GRAMMAR`] with its `root ::= object` header
/// stripped, so the caller can supply its own root while reusing the generic
/// `value` / `object` / `array` / primitive rules.
fn json_object_body() -> String {
    let mut out = String::new();
    for line in crate::grammar::JSON_OBJECT_GRAMMAR.lines() {
        if line.trim_start().starts_with("root ") {
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// Escape a raw string for inclusion inside a GBNF double-quoted literal.
fn gbnf_escape(raw: &str) -> String {
    raw.replace('\\', "\\\\").replace('"', "\\\"")
}

/// A GBNF string literal matching the quoted JSON string form of `s`
/// (e.g. `get_weather` → the GBNF literal matching `"get_weather"`).
fn gbnf_json_string_literal(s: &str) -> String {
    // Serializing a `String` is infallible; the default is unreachable.
    let json = serde_json::to_string(&Value::String(s.to_string())).unwrap_or_default();
    format!("\"{}\"", gbnf_escape(&json))
}

/// A parsed tool call: the function name and its arguments as a JSON string.
struct CallData {
    name: String,
    arguments: String,
}

/// Parse a single JSON object `{"name": …, "arguments": …}` into [`CallData`].
///
/// `arguments` is normalized to the OpenAI convention (a JSON-encoded string):
/// an object/value is re-encoded, a string is passed through. When
/// `require_arguments` is set, a missing `arguments` key rejects the parse (used
/// to avoid misclassifying arbitrary JSON as a tool call); otherwise it defaults
/// to `{}`.
fn parse_call_object(s: &str, require_arguments: bool) -> Option<CallData> {
    let value: Value = serde_json::from_str(s).ok()?;
    let object = value.as_object()?;
    let name = object.get("name").and_then(Value::as_str)?.to_string();
    if name.is_empty() {
        return None;
    }
    let arguments = match object.get("arguments") {
        Some(Value::String(inner)) => inner.clone(),
        // Re-encode a JSON value to its string form; a `serde_json::Value`
        // always re-serializes, so the error arm is unreachable.
        Some(other) => serde_json::to_string(other).ok()?,
        None if require_arguments => return None,
        None => "{}".to_string(),
    };
    Some(CallData { name, arguments })
}

/// Build a wire [`ToolCall`] with a synthesized `call_{index}` id.
fn into_tool_call(index: usize, data: CallData) -> ToolCall {
    ToolCall::Function {
        id: format!("call_{index}"),
        function: ToolCallFunction {
            name: data.name,
            arguments: data.arguments,
        },
    }
}

/// Extract tool calls from generated text.
///
/// Returns the leftover assistant text (with any `<tool_call>` blocks removed)
/// and the parsed tool calls, or `None` when no tool call is present. Two forms
/// are recognized:
/// - the whole trimmed text is a single `{"name": …, "arguments": …}` object
///   (the grammar-forced case), or
/// - one or more `<tool_call>…</tool_call>` blocks (the best-effort `auto` case).
pub(crate) fn parse_tool_calls_from_text(text: &str) -> (String, Option<Vec<ToolCall>>) {
    const OPEN: &str = "<tool_call>";
    const CLOSE: &str = "</tool_call>";

    // (a) The whole trimmed text is a single tool-call object.
    if let Some(data) = parse_call_object(text.trim(), true) {
        return (String::new(), Some(vec![into_tool_call(0, data)]));
    }

    // (b) Best-effort: scan for <tool_call>…</tool_call> blocks.
    let mut calls: Vec<ToolCall> = Vec::new();
    let mut leftover = String::new();
    let mut rest = text;
    while let Some(start) = rest.find(OPEN) {
        leftover.push_str(&rest[..start]);
        let after_open = &rest[start + OPEN.len()..];
        let Some(end) = after_open.find(CLOSE) else {
            // Unterminated block: keep the remainder as text and stop.
            leftover.push_str(&rest[start..]);
            rest = "";
            break;
        };
        let inner = after_open[..end].trim();
        if let Some(data) = parse_call_object(inner, false) {
            let index = calls.len();
            calls.push(into_tool_call(index, data));
        }
        // A malformed block is dropped (neither a call nor leftover text).
        rest = &after_open[end + CLOSE.len()..];
    }
    leftover.push_str(rest);

    if calls.is_empty() {
        (text.to_string(), None)
    } else {
        (leftover.trim().to_string(), Some(calls))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tool(name: &str, parameters: Option<Value>) -> Tool {
        Tool::Function {
            function: FunctionDef {
                name: name.to_string(),
                description: None,
                parameters,
                strict: None,
            },
        }
    }

    fn arguments_of(call: &ToolCall) -> (&str, &str) {
        let ToolCall::Function { id, function } = call;
        (id.as_str(), function.arguments.as_str())
    }

    // -----------------------------------------------------------------------
    // tool_call_grammar
    // -----------------------------------------------------------------------

    #[test]
    fn single_forced_tool_uses_typed_arguments() {
        let tools = [tool(
            "get_weather",
            Some(json!({
                "type": "object",
                "properties": {"city": {"type": "string"}},
                "required": ["city"]
            })),
        )];
        let g = tool_call_grammar(&tools, Some("get_weather"));
        assert!(g.starts_with("root ::= "), "grammar has a root: {g}");
        assert!(g.contains("\\\"name\\\""), "name key literal: {g}");
        assert!(
            g.contains("\\\"arguments\\\""),
            "arguments key literal: {g}"
        );
        assert!(g.contains("\\\"get_weather\\\""), "name value literal: {g}");
        // Typed args: the schema key is constrained.
        assert!(
            g.contains("\\\"city\\\""),
            "typed args include schema key: {g}"
        );
        assert!(g.lines().any(|l| l.starts_with("ws ")), "primitives: {g}");
    }

    #[test]
    fn required_single_tool_is_typed() {
        let tools = [tool(
            "f",
            Some(json!({
                "type": "object",
                "properties": {"x": {"type": "integer"}},
                "required": ["x"]
            })),
        )];
        let g = tool_call_grammar(&tools, None);
        assert!(g.contains("\\\"f\\\""));
        assert!(g.contains("\\\"x\\\""), "typed args: {g}");
        assert!(
            g.lines().any(|l| l.starts_with("integer ")),
            "int primitive: {g}"
        );
    }

    #[test]
    fn multiple_tools_use_name_alternation_and_generic_args() {
        let tools = [
            tool(
                "a",
                Some(json!({
                    "type": "object",
                    "properties": {"p": {"type": "string"}},
                    "required": ["p"]
                })),
            ),
            tool(
                "b",
                Some(json!({
                    "type": "object",
                    "properties": {"q": {"type": "string"}},
                    "required": ["q"]
                })),
            ),
        ];
        let g = tool_call_grammar(&tools, None);
        let root = g.lines().next().unwrap();
        assert_eq!(root, "root ::= call-0 | call-1", "root alternation: {root}");
        assert!(g.contains("\\\"a\\\""));
        assert!(g.contains("\\\"b\\\""));
        // Generic-object body is present.
        assert!(
            g.lines().any(|l| l.starts_with("object ")),
            "generic object: {g}"
        );
        assert!(
            g.lines().any(|l| l.starts_with("value ")),
            "generic value: {g}"
        );
        // Per-tool schema keys are NOT constrained (generic arguments).
        assert!(!g.contains("\\\"p\\\""), "multi-tool args are generic: {g}");
        assert!(!g.contains("\\\"q\\\""), "multi-tool args are generic: {g}");
    }

    #[test]
    fn named_choice_among_many_constrains_to_one() {
        let tools = [
            tool(
                "a",
                Some(json!({
                    "type": "object",
                    "properties": {"p": {"type": "string"}},
                    "required": ["p"]
                })),
            ),
            tool(
                "b",
                Some(json!({
                    "type": "object",
                    "properties": {"q": {"type": "string"}},
                    "required": ["q"]
                })),
            ),
        ];
        let g = tool_call_grammar(&tools, Some("b"));
        assert!(g.contains("\\\"b\\\""));
        assert!(!g.contains("\\\"a\\\""), "only the named tool: {g}");
        assert!(
            g.contains("\\\"q\\\""),
            "typed args for the named tool: {g}"
        );
    }

    #[test]
    fn forced_name_absent_from_tools_still_constrains_name() {
        let tools = [tool("a", Some(json!({"type": "object"})))];
        let g = tool_call_grammar(&tools, Some("ghost"));
        assert!(g.contains("\\\"ghost\\\""));
        // No schema known → generic object arguments.
        assert!(
            g.lines().any(|l| l.starts_with("object ")),
            "generic object: {g}"
        );
    }

    #[test]
    fn tool_without_parameters_uses_generic_args() {
        let tools = [tool("ping", None)];
        let g = tool_call_grammar(&tools, None);
        let root = g.lines().next().unwrap();
        assert_eq!(root, "root ::= call-0");
        assert!(g.contains("\\\"ping\\\""));
        assert!(
            g.lines().any(|l| l.starts_with("object ")),
            "generic object: {g}"
        );
    }

    #[test]
    fn empty_tools_falls_back_to_json_object() {
        let g = tool_call_grammar(&[], None);
        assert!(g.contains("root   ::= object"), "json-object fallback: {g}");
    }

    // -----------------------------------------------------------------------
    // parse_tool_calls_from_text
    // -----------------------------------------------------------------------

    #[test]
    fn parse_whole_object_single_call() {
        let (leftover, calls) =
            parse_tool_calls_from_text(r#"{"name":"get_weather","arguments":{"city":"Paris"}}"#);
        assert_eq!(leftover, "");
        let calls = calls.expect("a tool call");
        assert_eq!(calls.len(), 1);
        let ToolCall::Function { id, function } = &calls[0];
        assert_eq!(id, "call_0");
        assert_eq!(function.name, "get_weather");
        assert_eq!(function.arguments, r#"{"city":"Paris"}"#);
    }

    #[test]
    fn parse_whole_object_with_surrounding_whitespace() {
        let (leftover, calls) =
            parse_tool_calls_from_text("  {\"name\":\"f\",\"arguments\":{}}  \n");
        assert_eq!(leftover, "");
        assert_eq!(calls.expect("a tool call").len(), 1);
    }

    #[test]
    fn parse_arguments_string_is_passed_through() {
        let (_, calls) = parse_tool_calls_from_text(r#"{"name":"f","arguments":"{\"a\":1}"}"#);
        let calls = calls.expect("a tool call");
        assert_eq!(arguments_of(&calls[0]).1, r#"{"a":1}"#);
    }

    #[test]
    fn object_without_arguments_is_not_a_tool_call() {
        // A plain structured-output object must not be mistaken for a call.
        let text = r#"{"name":"Alice"}"#;
        let (leftover, calls) = parse_tool_calls_from_text(text);
        assert_eq!(leftover, text);
        assert!(calls.is_none());
    }

    #[test]
    fn parse_tool_call_tag_block() {
        let text = "Let me check. <tool_call>{\"name\":\"f\",\"arguments\":{\"x\":1}}</tool_call>";
        let (leftover, calls) = parse_tool_calls_from_text(text);
        assert_eq!(leftover, "Let me check.");
        let calls = calls.expect("a tool call");
        assert_eq!(calls.len(), 1);
        let ToolCall::Function { id, function } = &calls[0];
        assert_eq!(id, "call_0");
        assert_eq!(function.name, "f");
        assert_eq!(function.arguments, r#"{"x":1}"#);
    }

    #[test]
    fn parse_multiple_tool_call_blocks_indexes_ids() {
        let text = "<tool_call>{\"name\":\"a\",\"arguments\":{}}</tool_call>\
                    <tool_call>{\"name\":\"b\",\"arguments\":{}}</tool_call>";
        let (leftover, calls) = parse_tool_calls_from_text(text);
        assert_eq!(leftover, "");
        let calls = calls.expect("two tool calls");
        assert_eq!(calls.len(), 2);
        assert_eq!(arguments_of(&calls[0]).0, "call_0");
        assert_eq!(arguments_of(&calls[1]).0, "call_1");
    }

    #[test]
    fn plain_text_yields_no_calls() {
        let text = "The weather is sunny today.";
        let (leftover, calls) = parse_tool_calls_from_text(text);
        assert_eq!(leftover, text);
        assert!(calls.is_none());
    }

    #[test]
    fn block_without_arguments_defaults_to_empty_object() {
        let text = "<tool_call>{\"name\":\"noargs\"}</tool_call>";
        let (_, calls) = parse_tool_calls_from_text(text);
        let calls = calls.expect("a tool call");
        let ToolCall::Function { function, .. } = &calls[0];
        assert_eq!(function.name, "noargs");
        assert_eq!(function.arguments, "{}");
    }

    #[test]
    fn malformed_block_is_dropped() {
        let text = "<tool_call>not json</tool_call>";
        let (leftover, calls) = parse_tool_calls_from_text(text);
        assert!(calls.is_none());
        assert_eq!(leftover, text);
    }

    #[test]
    fn unterminated_block_is_treated_as_text() {
        let text = "hello <tool_call>{\"name\":\"f\"";
        let (leftover, calls) = parse_tool_calls_from_text(text);
        assert!(calls.is_none());
        assert_eq!(leftover, text);
    }

    #[test]
    fn leftover_text_around_a_block_is_preserved() {
        let text = "before <tool_call>{\"name\":\"f\",\"arguments\":{}}</tool_call> after";
        let (leftover, calls) = parse_tool_calls_from_text(text);
        assert_eq!(calls.expect("a tool call").len(), 1);
        assert_eq!(leftover, "before  after");
    }
}
