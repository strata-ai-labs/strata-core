//! Gated real GGUF local runtime integration tests.

#![cfg(feature = "local")]

use strata_inference_next::{
    EmbedRequest, GenerateRequest, InferenceRuntime, InferenceRuntimeConfig, RankRequest,
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
