//! Whole-body request goldens and cross-provider wire-mapping parity
//! (TCP3.12).
//!
//! The per-provider inline tests assert wire fields one at a time. That leaves
//! whole-shape drift invisible: an extra key, a dropped key, or a renamed key
//! elsewhere in the body passes every field-by-field test. These goldens pin
//! the *entire* JSON each provider emits for one canonical request, so any
//! shape change is a diff. Because the three cloud providers map the same
//! neutral `ChatRequest`/`GenerateRequest` to different wire shapes, each has
//! its own golden; comparing parsed `Value`s (not raw strings) keeps the pin
//! semantic — key order is irrelevant, but extra/missing/changed keys fail.
//!
//! This also pins the cross-provider *silent-drop* contract that the
//! field-by-field suites cover asymmetrically: OpenAI serializes
//! `logit_bias`/penalties/`seed` on the chat path, and Anthropic and Google
//! must drop them. A regression that leaked those into an Anthropic or Google
//! body would otherwise pass every existing test.

#![cfg(all(
    test,
    any(feature = "openai", feature = "anthropic", feature = "google")
))]

use crate::wire::{ChatMessage, ChatRequest, FunctionDef, Role, Tool, ToolChoice, ToolChoiceMode};
use crate::GenerateRequest;

/// A rich chat request exercising every field whose provider mapping differs:
/// penalties, `logit_bias`, `seed`, `logprobs`, and tools. All three providers
/// accept it (unsupported knobs are silently dropped, not rejected).
fn canonical_chat() -> ChatRequest {
    ChatRequest {
        messages: Some(vec![
            ChatMessage::new(Role::System, "You are a helpful assistant."),
            ChatMessage::new(Role::User, "What is the weather in Paris?"),
        ]),
        max_tokens: Some(128),
        // Only exactly-f32-representable floats, and temperature alone (not
        // top_p): the values survive the f32->f64 JSON widening intact so a
        // parsed-`Value` golden stays exact, and Anthropic rejects setting both
        // temperature and top_p on the chat path.
        temperature: Some(0.5),
        stop: Some(vec!["END".into()]),
        seed: Some(42),
        frequency_penalty: Some(0.5),
        presence_penalty: Some(0.25),
        logit_bias: Some([(123, 1.5)].into_iter().collect()),
        logprobs: Some(true),
        top_logprobs: Some(3),
        tools: Some(vec![Tool::Function {
            function: FunctionDef {
                name: "get_weather".into(),
                description: Some("Look up the weather".into()),
                parameters: Some(serde_json::json!({
                    "type": "object",
                    "properties": { "city": { "type": "string" } },
                })),
                strict: None,
            },
        }]),
        tool_choice: Some(ToolChoice::Mode(ToolChoiceMode::Auto)),
        ..Default::default()
    }
}

/// A completion request exercising the sampling knobs the generate path maps.
fn canonical_generate() -> GenerateRequest {
    GenerateRequest {
        prompt: "Tell me a joke.".into(),
        max_tokens: 64,
        temperature: 0.5,
        top_k: 40,
        top_p: 0.75,
        seed: Some(7),
        stop_sequences: vec!["\n\n".into()],
        stop_tokens: Vec::new(),
        grammar: None,
    }
}

/// Parses a built request body into a `Value` for golden comparison.
fn body(built: &str) -> serde_json::Value {
    serde_json::from_str(built).expect("provider emitted valid JSON")
}

// --- OpenAI ---------------------------------------------------------------

#[cfg(feature = "openai")]
#[test]
fn openai_chat_request_golden() {
    let got = body(&super::openai::build_chat_request_json("gpt-4o", &canonical_chat()).unwrap());
    // OpenAI keeps the system turn inline and forwards every sampling knob.
    assert_eq!(
        got,
        serde_json::json!({
            "model": "gpt-4o",
            "messages": [
                {"role": "system", "content": "You are a helpful assistant."},
                {"role": "user", "content": "What is the weather in Paris?"}
            ],
            "max_tokens": 128,
            "temperature": 0.5,
            "stop": ["END"],
            "seed": 42,
            "frequency_penalty": 0.5,
            "presence_penalty": 0.25,
            "logit_bias": {"123": 1.5},
            "logprobs": true,
            "top_logprobs": 3,
            "tools": [{
                "type": "function",
                "function": {
                    "name": "get_weather",
                    "description": "Look up the weather",
                    "parameters": {"type": "object", "properties": {"city": {"type": "string"}}}
                }
            }],
            "tool_choice": "auto"
        })
    );
}

#[cfg(feature = "openai")]
#[test]
fn openai_generate_request_golden() {
    let got = body(&super::openai::build_request_json(
        "gpt-4o",
        &canonical_generate(),
    ));
    assert_eq!(
        got,
        serde_json::json!({
            "model": "gpt-4o",
            "messages": [{"role": "user", "content": "Tell me a joke."}],
            "max_tokens": 64,
            "temperature": 0.5,
            "top_p": 0.75,
            "seed": 7,
            "stop": ["\n\n"]
        })
    );
}

#[cfg(feature = "openai")]
#[test]
fn openai_embed_request_golden() {
    let got = body(&super::openai::build_embed_request_json(
        "text-embedding-3-small",
        &["alpha", "beta"],
    ));
    assert_eq!(
        got,
        serde_json::json!({"model": "text-embedding-3-small", "input": ["alpha", "beta"]})
    );
}

#[cfg(feature = "openai")]
#[test]
fn openai_parse_chat_response_rejects_malformed_json() {
    // The analogous test exists for Anthropic and Google; OpenAI's chat parser
    // had none. Malformed input must be a structured Provider error, not a panic.
    let err = super::openai::parse_chat_response_json("not json{", "gpt-4o").unwrap_err();
    assert!(
        matches!(err, crate::InferenceError::Provider(_)),
        "expected Provider error, got {err:?}"
    );
}

// --- Anthropic ------------------------------------------------------------

#[cfg(feature = "anthropic")]
#[test]
fn anthropic_chat_request_golden() {
    let got = body(
        &super::anthropic::build_chat_request_json("claude-3-5-sonnet", &canonical_chat()).unwrap(),
    );
    // Anthropic hoists the system turn to a top-level `system` field, renames
    // stop/tool shapes, and drops every OpenAI-only knob (penalties, seed,
    // logit_bias, logprobs) — the absence of those keys is the contract.
    assert_eq!(
        got,
        serde_json::json!({
            "model": "claude-3-5-sonnet",
            "system": "You are a helpful assistant.",
            "messages": [{"role": "user", "content": "What is the weather in Paris?"}],
            "max_tokens": 128,
            "temperature": 0.5,
            "stop_sequences": ["END"],
            "tool_choice": {"type": "auto"},
            "tools": [{
                "name": "get_weather",
                "description": "Look up the weather",
                "input_schema": {"type": "object", "properties": {"city": {"type": "string"}}}
            }]
        })
    );
}

#[cfg(feature = "anthropic")]
#[test]
fn anthropic_generate_request_golden() {
    let got = body(&super::anthropic::build_request_json(
        "claude-3-5-sonnet",
        &canonical_generate(),
    ));
    // The generate path drops top_p and seed too.
    assert_eq!(
        got,
        serde_json::json!({
            "model": "claude-3-5-sonnet",
            "messages": [{"role": "user", "content": "Tell me a joke."}],
            "max_tokens": 64,
            "temperature": 0.5,
            "stop_sequences": ["\n\n"]
        })
    );
}

// --- Google ---------------------------------------------------------------

#[cfg(feature = "google")]
#[test]
fn google_chat_request_golden() {
    let got = body(&super::google::build_chat_request_json(&canonical_chat()).unwrap());
    // Gemini nests sampling under generationConfig, maps logprobs to
    // responseLogprobs/logprobs, forces thinkingBudget 0, and drops the
    // OpenAI-only knobs (penalties, seed, logit_bias).
    assert_eq!(
        got,
        serde_json::json!({
            "contents": [{
                "role": "user",
                "parts": [{"text": "What is the weather in Paris?"}]
            }],
            "system_instruction": {"parts": [{"text": "You are a helpful assistant."}]},
            "generationConfig": {
                "maxOutputTokens": 128,
                "temperature": 0.5,
                "stopSequences": ["END"],
                "responseLogprobs": true,
                "logprobs": 3,
                "thinkingConfig": {"thinkingBudget": 0}
            },
            "toolConfig": {"functionCallingConfig": {"mode": "AUTO"}},
            "tools": [{
                "functionDeclarations": [{
                    "name": "get_weather",
                    "description": "Look up the weather",
                    "parameters": {"type": "object", "properties": {"city": {"type": "string"}}}
                }]
            }]
        })
    );
}

#[cfg(feature = "google")]
#[test]
fn google_generate_request_golden() {
    let got = body(&super::google::build_request_json(&canonical_generate()));
    assert_eq!(
        got,
        serde_json::json!({
            "contents": [{"parts": [{"text": "Tell me a joke."}]}],
            "generationConfig": {
                "maxOutputTokens": 64,
                "temperature": 0.5,
                "topK": 40,
                "topP": 0.75,
                "stopSequences": ["\n\n"],
                "thinkingConfig": {"thinkingBudget": 0}
            }
        })
    );
}

#[cfg(feature = "google")]
#[test]
fn google_embed_request_goldens() {
    let single = body(&super::google::build_embed_request_json("hello"));
    assert_eq!(
        single,
        serde_json::json!({"content": {"parts": [{"text": "hello"}]}})
    );

    let batch = body(&super::google::build_batch_embed_request_json(
        "text-embedding-004",
        &["alpha", "beta"],
    ));
    assert_eq!(
        batch,
        serde_json::json!({
            "requests": [
                {"model": "models/text-embedding-004", "content": {"parts": [{"text": "alpha"}]}},
                {"model": "models/text-embedding-004", "content": {"parts": [{"text": "beta"}]}}
            ]
        })
    );
}

// --- Cross-provider silent-drop parity ------------------------------------

/// OpenAI serializes `logit_bias`, the penalties, and `seed`; Anthropic and
/// Google must drop all of them on the chat path. The whole-body goldens above
/// already encode this, but this names the contract so a regression localizes
/// here rather than in a large golden diff.
#[cfg(all(feature = "openai", feature = "anthropic", feature = "google"))]
#[test]
fn advanced_knobs_are_serialized_by_openai_and_dropped_by_anthropic_and_google() {
    let req = canonical_chat();
    let dropped = [
        "logit_bias",
        "frequency_penalty",
        "presence_penalty",
        "seed",
    ];

    let openai = body(&super::openai::build_chat_request_json("gpt-4o", &req).unwrap());
    for key in dropped {
        assert!(openai.get(key).is_some(), "OpenAI must serialize `{key}`");
    }

    let anthropic = body(&super::anthropic::build_chat_request_json("claude", &req).unwrap());
    let google = body(&super::google::build_chat_request_json(&req).unwrap());
    for key in dropped {
        assert!(anthropic.get(key).is_none(), "Anthropic must drop `{key}`");
        // Google nests config, but these keys must appear nowhere in the body.
        assert!(
            !google.to_string().contains(&format!("\"{key}\"")),
            "Google must drop `{key}`"
        );
    }
}
