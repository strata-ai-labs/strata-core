//! JSON Schema → GBNF grammar conversion for local (llama.cpp) constrained
//! generation.
//!
//! `response_format: json_object` maps to [`JSON_OBJECT_GRAMMAR`]; a
//! `json_schema` maps via [`json_schema_to_gbnf`]. The resulting GBNF string is
//! fed to the already-bound `llama_sampler_init_grammar` path (see
//! `llama/ffi.rs`), so local models emit output constrained to the schema.
//!
//! This module is pure Rust (no llama.cpp dependency) and is compiled
//! unconditionally so its unit tests run without the `local` feature. Only the
//! wiring in `provider/local.rs` is feature-gated.
//!
//! Supported subset (covers the common structured-output shapes): `object`
//! (`properties` + `required`), `array` (`items`), `string`, `number`,
//! `integer`, `boolean`, `null`, `enum`, and `const`. Documented limitations,
//! left for a later slice: optional-property permutations (all listed keys are
//! required), `additionalProperties`, `$ref`, `anyOf`/`oneOf` beyond
//! `enum`/`const`, string `pattern`/`format`, and numeric bounds. Unconvertible
//! schemas still produce a valid grammar (falling back to "any JSON value" for
//! the unknown parts) rather than an error.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use serde_json::Value;

/// A GBNF grammar accepting any single JSON object — used for
/// `response_format: json_object`.
pub(crate) const JSON_OBJECT_GRAMMAR: &str = concat!(
    "root   ::= object\n",
    "value  ::= object | array | string | number | boolean | null\n",
    "object ::= \"{\" ws ( string \":\" ws value ( \",\" ws string \":\" ws value )* )? \"}\" ws\n",
    "array  ::= \"[\" ws ( value ( \",\" ws value )* )? \"]\" ws\n",
    "string ::= \"\\\"\" ( [^\"\\\\] | \"\\\\\" ([\"\\\\/bfnrt] | \"u\" [0-9a-fA-F] [0-9a-fA-F] [0-9a-fA-F] [0-9a-fA-F]) )* \"\\\"\" ws\n",
    "number ::= \"-\"? (\"0\" | [1-9] [0-9]*) (\".\" [0-9]+)? ([eE] [-+]? [0-9]+)? ws\n",
    "boolean ::= (\"true\" | \"false\") ws\n",
    "null   ::= \"null\" ws\n",
    "ws     ::= [ \\t\\n]*\n",
);

/// Shared primitive rules appended to every generated schema grammar.
const PRIMITIVES: &str = concat!(
    "string  ::= \"\\\"\" ( [^\"\\\\] | \"\\\\\" ([\"\\\\/bfnrt] | \"u\" [0-9a-fA-F] [0-9a-fA-F] [0-9a-fA-F] [0-9a-fA-F]) )* \"\\\"\" ws\n",
    "number  ::= \"-\"? (\"0\" | [1-9] [0-9]*) (\".\" [0-9]+)? ([eE] [-+]? [0-9]+)? ws\n",
    "integer ::= \"-\"? (\"0\" | [1-9] [0-9]*) ws\n",
    "boolean ::= (\"true\" | \"false\") ws\n",
    "null    ::= \"null\" ws\n",
    "ws      ::= [ \\t\\n]*\n",
);

/// Convert a JSON Schema value into a GBNF grammar whose `root` rule matches
/// documents valid under the schema (best-effort subset — see the module docs).
pub(crate) fn json_schema_to_gbnf(schema: &Value) -> String {
    let mut builder = GrammarBuilder::default();
    let root = builder.visit(schema);

    let mut out = String::new();
    // `visit` returns the name of the entry rule; alias it to `root`, which is
    // the default grammar root llama.cpp starts from.
    if root == "root" {
        // Only possible if a future change names a rule "root"; keep it safe.
    } else {
        let _ = writeln!(out, "root ::= {root}");
    }
    for (name, body) in &builder.rules {
        let _ = writeln!(out, "{name} ::= {body}");
    }
    out.push_str(PRIMITIVES);
    out
}

#[derive(Default)]
struct GrammarBuilder {
    /// name → body. `BTreeMap` keeps output deterministic; GBNF allows forward
    /// references, so rule order does not matter.
    rules: BTreeMap<String, String>,
    counter: usize,
}

impl GrammarBuilder {
    fn fresh(&mut self, hint: &str) -> String {
        self.counter += 1;
        format!("{hint}-{}", self.counter)
    }

    /// Register the rules needed to match `schema` and return the entry rule's
    /// name (or a primitive rule name).
    fn visit(&mut self, schema: &Value) -> String {
        if let Some(constant) = schema.get("const") {
            return self.named(&format!("{} ws", gbnf_literal(constant)), "const");
        }
        if let Some(Value::Array(variants)) = schema.get("enum") {
            let alts: Vec<String> = variants.iter().map(gbnf_literal).collect();
            return self.named(&format!("({}) ws", alts.join(" | ")), "enum");
        }

        match schema.get("type").and_then(Value::as_str) {
            Some("object") => self.visit_object(schema),
            Some("array") => self.visit_array(schema),
            Some("string") => "string".to_string(),
            Some("integer") => "integer".to_string(),
            Some("number") => "number".to_string(),
            Some("boolean") => "boolean".to_string(),
            Some("null") => "null".to_string(),
            // No/unknown type → accept any JSON value.
            _ => self.any_value(),
        }
    }

    fn visit_object(&mut self, schema: &Value) -> String {
        let props = schema.get("properties").and_then(Value::as_object);
        // Emit the `required` keys in order if present, else every declared
        // property. Each emitted key is required (strict subset).
        let keys: Vec<String> = match schema.get("required").and_then(Value::as_array) {
            Some(required) => required
                .iter()
                .filter_map(|k| k.as_str().map(String::from))
                .collect(),
            None => props
                .map(|p| p.keys().cloned().collect())
                .unwrap_or_default(),
        };

        if keys.is_empty() {
            // Any object.
            self.any_value();
            return "object".to_string();
        }

        let mut parts: Vec<String> = vec!["\"{\" ws".to_string()];
        for (i, key) in keys.iter().enumerate() {
            if i > 0 {
                parts.push("\",\" ws".to_string());
            }
            let sub = match props.and_then(|p| p.get(key)) {
                Some(sub_schema) => self.visit(sub_schema),
                None => self.any_value(),
            };
            parts.push(format!("{} ws \":\" ws {sub}", gbnf_key(key)));
        }
        parts.push("\"}\" ws".to_string());
        self.named(&parts.join(" "), "obj")
    }

    fn visit_array(&mut self, schema: &Value) -> String {
        let item = match schema.get("items") {
            Some(items) => self.visit(items),
            None => self.any_value(),
        };
        self.named(
            &format!("\"[\" ws ( {item} ( \",\" ws {item} )* )? \"]\" ws"),
            "arr",
        )
    }

    /// Register a fresh rule with `body` and return its name.
    fn named(&mut self, body: &str, hint: &str) -> String {
        let name = self.fresh(hint);
        self.rules.insert(name.clone(), body.to_string());
        name
    }

    /// Ensure the generic `value`/`object`/`array` rules exist and return
    /// `value` (the "any JSON value" rule).
    fn any_value(&mut self) -> String {
        self.rules
            .entry("value".to_string())
            .or_insert_with(|| "object | array | string | number | boolean | null".to_string());
        self.rules.entry("object".to_string()).or_insert_with(|| {
            "\"{\" ws ( string \":\" ws value ( \",\" ws string \":\" ws value )* )? \"}\" ws"
                .to_string()
        });
        self.rules
            .entry("array".to_string())
            .or_insert_with(|| "\"[\" ws ( value ( \",\" ws value )* )? \"]\" ws".to_string());
        "value".to_string()
    }
}

/// A GBNF string literal matching exactly the JSON serialization of `value`
/// (used for `const`/`enum`). Backslash and double-quote are escaped for GBNF.
fn gbnf_literal(value: &Value) -> String {
    let json = serde_json::to_string(value).unwrap_or_default();
    format!("\"{}\"", gbnf_escape(&json))
}

/// A GBNF string literal matching a quoted JSON object key.
fn gbnf_key(key: &str) -> String {
    let json = serde_json::to_string(&Value::String(key.to_string())).unwrap_or_default();
    format!("\"{}\"", gbnf_escape(&json))
}

/// Escape a raw string for inclusion inside a GBNF double-quoted literal.
fn gbnf_escape(raw: &str) -> String {
    raw.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contains_rule(grammar: &str, name: &str) -> bool {
        grammar.lines().any(|l| l.starts_with(&format!("{name} ")))
    }

    #[test]
    fn json_object_grammar_has_root_and_object() {
        assert!(JSON_OBJECT_GRAMMAR.contains("root   ::= object"));
        assert!(JSON_OBJECT_GRAMMAR.contains("object ::="));
        assert!(JSON_OBJECT_GRAMMAR.contains("ws     ::="));
    }

    #[test]
    fn object_schema_emits_required_keys_in_order() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "age": {"type": "integer"}
            },
            "required": ["name", "age"]
        });
        let g = json_schema_to_gbnf(&schema);
        assert!(
            g.starts_with("root ::= obj-"),
            "root aliases the object: {g}"
        );
        // Both keys present as quoted literals, name before age.
        let name_at = g.find("\\\"name\\\"").expect("name key");
        let age_at = g.find("\\\"age\\\"").expect("age key");
        assert!(name_at < age_at, "required order preserved");
        // Primitives are appended.
        assert!(contains_rule(&g, "string"));
        assert!(contains_rule(&g, "integer"));
        assert!(contains_rule(&g, "ws"));
    }

    #[test]
    fn object_without_required_uses_all_properties() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {"city": {"type": "string"}}
        });
        let g = json_schema_to_gbnf(&schema);
        assert!(g.contains("\\\"city\\\""));
    }

    #[test]
    fn enum_becomes_alternation_of_literals() {
        let schema = serde_json::json!({"enum": ["red", "green", "blue"]});
        let g = json_schema_to_gbnf(&schema);
        assert!(g.contains("\\\"red\\\""));
        assert!(g.contains("\\\"green\\\""));
        assert!(g.contains(" | "), "alternation: {g}");
    }

    #[test]
    fn const_becomes_single_literal() {
        let schema = serde_json::json!({"const": "yes"});
        let g = json_schema_to_gbnf(&schema);
        assert!(g.contains("\\\"yes\\\""));
    }

    #[test]
    fn array_of_strings() {
        let schema = serde_json::json!({"type": "array", "items": {"type": "string"}});
        let g = json_schema_to_gbnf(&schema);
        assert!(g.starts_with("root ::= arr-"));
        assert!(g.contains("\"[\""));
        assert!(g.contains("\"]\""));
        assert!(contains_rule(&g, "string"));
    }

    #[test]
    fn nested_object_in_array() {
        let schema = serde_json::json!({
            "type": "array",
            "items": {
                "type": "object",
                "properties": {"id": {"type": "integer"}},
                "required": ["id"]
            }
        });
        let g = json_schema_to_gbnf(&schema);
        assert!(g.contains("\\\"id\\\""));
        assert!(contains_rule(&g, "integer"));
    }

    #[test]
    fn untyped_schema_falls_back_to_any_value() {
        let schema = serde_json::json!({});
        let g = json_schema_to_gbnf(&schema);
        assert!(contains_rule(&g, "value"));
        assert!(g.starts_with("root ::= value"));
    }

    #[test]
    fn scalar_types_reference_primitives() {
        for (ty, rule) in [
            ("string", "string"),
            ("integer", "integer"),
            ("number", "number"),
            ("boolean", "boolean"),
            ("null", "null"),
        ] {
            let g = json_schema_to_gbnf(&serde_json::json!({ "type": ty }));
            assert_eq!(g.lines().next().unwrap(), format!("root ::= {rule}"));
            assert!(contains_rule(&g, rule), "primitive {rule} defined");
        }
    }

    #[test]
    fn keys_with_special_chars_are_escaped() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {"a\"b": {"type": "string"}},
            "required": ["a\"b"]
        });
        let g = json_schema_to_gbnf(&schema);
        // serde serializes the key to `"a\"b"`; GBNF-escaping doubles that
        // backslash, so the literal contains `a\\` (a followed by two
        // backslashes) rather than the under-escaped `a\"`.
        assert!(g.contains("a\\\\"), "backslash doubled for GBNF: {g}");
        assert!(!g.contains("a\\\"b"), "not under-escaped: {g}");
    }
}
