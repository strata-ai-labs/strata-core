//! Gated real GGUF local runtime integration tests.

#![cfg(feature = "local")]

use strata_inference::{
    ChatMessage, ChatRequest, EmbedRequest, FinishReason, GenerateRequest, InferenceRuntime,
    InferenceRuntimeConfig, RankRequest, Role,
};

fn integration_enabled() -> bool {
    std::env::var_os("STRATA_RUN_LOCAL_INFERENCE_INTEGRATION").is_some()
}

fn runtime() -> InferenceRuntime {
    InferenceRuntime::new(InferenceRuntimeConfig {
        models_dir: None,
        network_enabled: false,
    })
}

fn env_path(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
}

#[test]
#[cfg(feature = "local")]
fn local_generation_uses_real_gguf() {
    if !integration_enabled() {
        return;
    }
    let Some(path) = env_path("STRATA_INFERENCE_GENERATION_GGUF") else {
        return;
    };
    let model = format!("local:{path}");
    let response = runtime()
        .generate(
            &model,
            &GenerateRequest {
                prompt: "Write one short sentence about embedded databases.".to_owned(),
                max_tokens: 32,
                temperature: 0.0,
                ..GenerateRequest::default()
            },
        )
        .expect("local generation succeeds");
    assert!(!response.text.trim().is_empty());
}

#[test]
#[cfg(feature = "local")]
fn local_chat_applies_template_and_full_sampler() {
    if !integration_enabled() {
        return;
    }
    let Some(path) = env_path("STRATA_INFERENCE_GENERATION_GGUF") else {
        return;
    };
    let model = format!("local:{path}");
    // messages (not a raw prompt) exercise the chat-template cascade; the
    // extension knobs exercise the full sampler chain.
    let request = ChatRequest {
        messages: Some(vec![
            ChatMessage::new(Role::System, "You are terse. Answer in one word."),
            ChatMessage::new(Role::User, "What is the capital of France?"),
        ]),
        max_tokens: Some(16),
        temperature: Some(0.7),
        top_k: Some(40),
        min_p: Some(0.05),
        repeat_penalty: Some(1.1),
        seed: Some(42),
        ..ChatRequest::default()
    };
    let response = runtime().chat(&model, &request).expect("local chat succeeds");
    assert_eq!(response.choices.len(), 1);
    assert!(
        !response.choices[0].message.content.trim().is_empty(),
        "empty generation: {:?}",
        response.choices[0].message.content
    );
    assert!(matches!(
        response.choices[0].finish_reason,
        FinishReason::Stop | FinishReason::Length
    ));
    assert!(response.usage.prompt_tokens > 0);
    assert_eq!(
        response.usage.total_tokens,
        response.usage.prompt_tokens + response.usage.completion_tokens
    );
}

#[test]
#[cfg(feature = "local")]
fn local_chat_raw_prompt_greedy() {
    if !integration_enabled() {
        return;
    }
    let Some(path) = env_path("STRATA_INFERENCE_GENERATION_GGUF") else {
        return;
    };
    let model = format!("local:{path}");
    // Raw-prompt path (no template) + greedy (temperature omitted -> 0).
    let request = ChatRequest {
        prompt: Some("The capital of France is".to_owned()),
        max_tokens: Some(8),
        ..ChatRequest::default()
    };
    let response = runtime().chat(&model, &request).expect("raw-prompt chat succeeds");
    assert!(!response.choices[0].message.content.trim().is_empty());
}

#[test]
#[cfg(feature = "local")]
fn local_tokenize_and_detokenize_use_real_gguf() {
    if !integration_enabled() {
        return;
    }
    let Some(path) = env_path("STRATA_INFERENCE_GENERATION_GGUF") else {
        return;
    };
    let model = format!("local:{path}");
    let runtime = runtime();
    let tokens = runtime
        .tokenize(&model, "embedded database", true)
        .expect("local tokenize succeeds");
    assert!(!tokens.is_empty());
    let text = runtime
        .detokenize(&model, &tokens)
        .expect("local detokenize succeeds");
    assert!(!text.trim().is_empty());
}

#[test]
#[cfg(feature = "local")]
fn local_embedding_uses_real_gguf() {
    if !integration_enabled() {
        return;
    }
    let Some(path) = env_path("STRATA_INFERENCE_EMBEDDING_GGUF") else {
        return;
    };
    let model = format!("local:{path}");
    let vector = runtime()
        .embed(
            &model,
            &EmbedRequest {
                text: "embedded database".to_owned(),
            },
        )
        .expect("local embedding succeeds");
    assert!(!vector.is_empty());
    assert!(vector.iter().all(|value| value.is_finite()));
}

#[test]
#[cfg(feature = "local")]
fn local_ranking_uses_real_gguf() {
    if !integration_enabled() {
        return;
    }
    let Some(path) = env_path("STRATA_INFERENCE_RANKING_GGUF") else {
        return;
    };
    let model = format!("local:{path}");
    let response = runtime()
        .rank(
            &model,
            &RankRequest {
                query: "embedded database".to_owned(),
                passages: vec![
                    "Strata is an embedded multi-model database.".to_owned(),
                    "A compiler parses source code.".to_owned(),
                ],
            },
        )
        .expect("local ranking succeeds");
    assert_eq!(response.items.len(), 2);
}
