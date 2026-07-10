//! Gated live provider integration tests.

#![cfg(any(feature = "anthropic", feature = "openai", feature = "google"))]

use std::fs;
use std::path::PathBuf;
use std::sync::Once;

#[cfg(any(feature = "openai", feature = "google"))]
use strata_inference::EmbedRequest;
use strata_inference::{GenerateRequest, InferenceRuntime, InferenceRuntimeConfig};

static LOAD_ENV: Once = Once::new();

fn load_dotenv() {
    LOAD_ENV.call_once(|| {
        let Some(path) = dotenv_path() else {
            return;
        };
        let Ok(contents) = fs::read_to_string(path) else {
            return;
        };
        for line in contents.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim();
            if std::env::var_os(key).is_some() {
                continue;
            }
            let value = value.trim().trim_matches('"').trim_matches('\'');
            unsafe { std::env::set_var(key, value) };
        }
    });
}

fn dotenv_path() -> Option<PathBuf> {
    let cwd = PathBuf::from(".env");
    if cwd.exists() {
        return Some(cwd);
    }

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    [
        manifest_dir.join(".env"),
        manifest_dir.join("..").join("..").join(".env"),
    ]
    .into_iter()
    .find(|candidate| candidate.exists())
}

fn integration_enabled() -> bool {
    load_dotenv();
    std::env::var_os("STRATA_RUN_PROVIDER_INFERENCE_INTEGRATION").is_some()
}

fn runtime() -> InferenceRuntime {
    InferenceRuntime::new(InferenceRuntimeConfig {
        models_dir: None,
        network_enabled: true,
    })
}

fn generation_request() -> GenerateRequest {
    GenerateRequest {
        prompt: "Reply with exactly one short sentence about embedded databases.".to_owned(),
        max_tokens: 32,
        temperature: 0.0,
        ..GenerateRequest::default()
    }
}

fn model_spec(env_var: &str, default_spec: &str) -> String {
    std::env::var(env_var).unwrap_or_else(|_| default_spec.to_owned())
}

#[cfg(any(feature = "openai", feature = "google"))]
fn assert_finite_embedding(vector: &[f32]) {
    assert!(!vector.is_empty());
    assert!(vector.iter().all(|value| value.is_finite()));
}

#[test]
#[cfg(feature = "openai")]
fn openai_generation_uses_real_api() {
    if !integration_enabled() {
        return;
    }
    let model = model_spec("STRATA_OPENAI_GENERATION_MODEL", "openai:gpt-4o-mini");
    let response = runtime()
        .generate(&model, &generation_request())
        .expect("OpenAI generation succeeds");
    assert!(!response.text.trim().is_empty());
    assert!(response.completion_tokens <= 64);
}

#[test]
#[cfg(feature = "openai")]
fn openai_embedding_uses_real_api() {
    if !integration_enabled() {
        return;
    }
    let model = model_spec(
        "STRATA_OPENAI_EMBEDDING_MODEL",
        "openai:text-embedding-3-small",
    );
    let vector = runtime()
        .embed(
            &model,
            &EmbedRequest {
                text: "embedded database".to_owned(),
            },
        )
        .expect("OpenAI embedding succeeds");
    assert_finite_embedding(&vector);
}

#[test]
#[cfg(feature = "anthropic")]
fn anthropic_generation_uses_real_api() {
    if !integration_enabled() {
        return;
    }
    let model = model_spec(
        "STRATA_ANTHROPIC_GENERATION_MODEL",
        "anthropic:claude-sonnet-4-6",
    );
    let response = runtime()
        .generate(&model, &generation_request())
        .expect("Anthropic generation succeeds");
    assert!(!response.text.trim().is_empty());
    assert!(response.completion_tokens <= 64);
}

#[test]
#[cfg(feature = "google")]
fn google_generation_uses_real_api() {
    if !integration_enabled() {
        return;
    }
    let model = model_spec("STRATA_GOOGLE_GENERATION_MODEL", "google:gemini-2.5-flash");
    let response = runtime()
        .generate(&model, &generation_request())
        .expect("Google generation succeeds");
    assert!(!response.text.trim().is_empty());
    assert!(response.completion_tokens <= 64);
}

#[test]
#[cfg(feature = "google")]
fn google_embedding_uses_real_api() {
    if !integration_enabled() {
        return;
    }
    let model = model_spec(
        "STRATA_GOOGLE_EMBEDDING_MODEL",
        "google:gemini-embedding-001",
    );
    let vector = runtime()
        .embed(
            &model,
            &EmbedRequest {
                text: "embedded database".to_owned(),
            },
        )
        .expect("Google embedding succeeds");
    assert_finite_embedding(&vector);
}
