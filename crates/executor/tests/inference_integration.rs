//! Gated real inference integration through executor commands.

#![cfg(feature = "inference")]

use std::fs;
use std::path::PathBuf;
use std::sync::Once;

use strata_executor::{Command, Executor, Output};
use strata_inference::{EmbedRequest, GenerateRequest, InferenceRuntime, InferenceRuntimeConfig};
#[cfg(feature = "inference-local")]
use strata_inference::{RankRequest, RankRuntimeOutcome};

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
    std::env::var_os("STRATA_RUN_EXECUTOR_INFERENCE_INTEGRATION").is_some()
}

fn executor(network_enabled: bool) -> Executor {
    Executor::open_cache()
        .expect("executor opens")
        .with_inference_runtime(InferenceRuntime::new(InferenceRuntimeConfig {
            models_dir: None,
            network_enabled,
        }))
}

fn model_spec(env_var: &str, default_spec: &str) -> String {
    std::env::var(env_var).unwrap_or_else(|_| default_spec.to_owned())
}

fn assert_finite_embedding(vector: &[f32]) {
    assert!(!vector.is_empty());
    assert!(vector.iter().all(|value| value.is_finite()));
}

#[cfg(feature = "inference-local")]
fn env_path(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
}

#[test]
fn executor_openai_generation_uses_real_api() {
    if !integration_enabled() {
        return;
    }
    let model = model_spec("STRATA_OPENAI_GENERATION_MODEL", "openai:gpt-4o-mini");
    let mut executor = executor(true);
    let output = executor
        .execute(Command::InferenceGenerate {
            model,
            request: GenerateRequest {
                prompt: "Reply with exactly one short sentence about embedded databases."
                    .to_owned(),
                max_tokens: 32,
                temperature: 0.0,
                ..GenerateRequest::default()
            },
        })
        .expect("OpenAI generation succeeds through executor");
    let Output::InferenceGeneration(response) = output else {
        panic!("expected generation output");
    };
    assert!(!response.text.trim().is_empty());
    assert!(response.completion_tokens <= 64);
}

#[test]
fn executor_anthropic_generation_uses_real_api() {
    if !integration_enabled() {
        return;
    }
    let model = model_spec(
        "STRATA_ANTHROPIC_GENERATION_MODEL",
        "anthropic:claude-sonnet-4-6",
    );
    let mut executor = executor(true);
    let output = executor
        .execute(Command::InferenceGenerate {
            model,
            request: GenerateRequest {
                prompt: "Reply with exactly one short sentence about embedded databases."
                    .to_owned(),
                max_tokens: 32,
                temperature: 0.0,
                ..GenerateRequest::default()
            },
        })
        .expect("Anthropic generation succeeds through executor");
    let Output::InferenceGeneration(response) = output else {
        panic!("expected generation output");
    };
    assert!(!response.text.trim().is_empty());
    assert!(response.completion_tokens <= 64);
}

#[test]
fn executor_google_generation_uses_real_api() {
    if !integration_enabled() {
        return;
    }
    let model = model_spec("STRATA_GOOGLE_GENERATION_MODEL", "google:gemini-2.5-flash");
    let mut executor = executor(true);
    let output = executor
        .execute(Command::InferenceGenerate {
            model,
            request: GenerateRequest {
                prompt: "Reply with exactly one short sentence about embedded databases."
                    .to_owned(),
                max_tokens: 32,
                temperature: 0.0,
                ..GenerateRequest::default()
            },
        })
        .expect("Google generation succeeds through executor");
    let Output::InferenceGeneration(response) = output else {
        panic!("expected generation output");
    };
    assert!(!response.text.trim().is_empty());
    assert!(response.completion_tokens <= 64);
}

#[test]
fn executor_openai_embedding_uses_real_api() {
    if !integration_enabled() {
        return;
    }
    let model = model_spec(
        "STRATA_OPENAI_EMBEDDING_MODEL",
        "openai:text-embedding-3-small",
    );
    let mut executor = executor(true);
    let output = executor
        .execute(Command::InferenceEmbed {
            model,
            request: EmbedRequest {
                text: "embedded database".to_owned(),
            },
        })
        .expect("OpenAI embedding succeeds through executor");
    let Output::InferenceEmbedding(vector) = output else {
        panic!("expected embedding output");
    };
    assert_finite_embedding(&vector);
}

#[test]
fn executor_google_embedding_uses_real_api() {
    if !integration_enabled() {
        return;
    }
    let model = model_spec(
        "STRATA_GOOGLE_EMBEDDING_MODEL",
        "google:gemini-embedding-001",
    );
    let mut executor = executor(true);
    let output = executor
        .execute(Command::InferenceEmbed {
            model,
            request: EmbedRequest {
                text: "embedded database".to_owned(),
            },
        })
        .expect("Google embedding succeeds through executor");
    let Output::InferenceEmbedding(vector) = output else {
        panic!("expected embedding output");
    };
    assert_finite_embedding(&vector);
}

#[test]
#[cfg(feature = "inference-local")]
fn executor_local_generation_and_tokenizer_use_real_gguf() {
    if !integration_enabled() {
        return;
    }
    let Some(path) = env_path("STRATA_INFERENCE_GENERATION_GGUF") else {
        return;
    };
    let model = format!("local:{path}");
    let mut executor = executor(false);

    let output = executor
        .execute(Command::InferenceGenerate {
            model: model.clone(),
            request: GenerateRequest {
                prompt: "Write one short sentence about embedded databases.".to_owned(),
                max_tokens: 32,
                temperature: 0.0,
                ..GenerateRequest::default()
            },
        })
        .expect("local generation succeeds through executor");
    let Output::InferenceGeneration(response) = output else {
        panic!("expected generation output");
    };
    assert!(!response.text.trim().is_empty());

    let output = executor
        .execute(Command::InferenceTokenize {
            model: model.clone(),
            text: "embedded database".to_owned(),
            add_special: true,
        })
        .expect("local tokenize succeeds through executor");
    let Output::InferenceTokenIds(tokens) = output else {
        panic!("expected token output");
    };
    assert!(!tokens.is_empty());

    let output = executor
        .execute(Command::InferenceDetokenize { model, ids: tokens })
        .expect("local detokenize succeeds through executor");
    let Output::InferenceText(text) = output else {
        panic!("expected text output");
    };
    assert!(!text.trim().is_empty());
}

#[test]
#[cfg(feature = "inference-local")]
fn executor_local_embedding_uses_real_gguf() {
    if !integration_enabled() {
        return;
    }
    let Some(path) = env_path("STRATA_INFERENCE_EMBEDDING_GGUF") else {
        return;
    };
    let model = format!("local:{path}");
    let mut executor = executor(false);

    let output = executor
        .execute(Command::InferenceEmbed {
            model: model.clone(),
            request: EmbedRequest {
                text: "embedded database".to_owned(),
            },
        })
        .expect("local embedding succeeds through executor");
    let Output::InferenceEmbedding(vector) = output else {
        panic!("expected embedding output");
    };
    assert!(!vector.is_empty());
    assert!(vector.iter().all(|value| value.is_finite()));

    let output = executor
        .execute(Command::InferenceEmbedBatch {
            model,
            texts: vec!["embedded database".to_owned(), "storage engine".to_owned()],
        })
        .expect("local batch embedding succeeds through executor");
    let Output::InferenceEmbeddings(response) = output else {
        panic!("expected batch embedding output");
    };
    assert_eq!(response.items.len(), 2);
    assert!(response.dimension > 0);
}

#[test]
#[cfg(feature = "inference-local")]
fn executor_local_ranking_uses_real_gguf() {
    if !integration_enabled() {
        return;
    }
    let Some(path) = env_path("STRATA_INFERENCE_RANKING_GGUF") else {
        return;
    };
    let model = format!("local:{path}");
    let mut executor = executor(false);
    let output = executor
        .execute(Command::InferenceRank {
            model,
            request: RankRequest {
                query: "embedded database".to_owned(),
                passages: vec![
                    "Strata is an embedded multi-model database.".to_owned(),
                    "A compiler parses source code.".to_owned(),
                ],
            },
        })
        .expect("local ranking succeeds through executor");
    let Output::InferenceRanking(response) = output else {
        panic!("expected ranking output");
    };
    assert_eq!(response.items.len(), 2);
    for item in response.items {
        let RankRuntimeOutcome::Ok { score, .. } = item else {
            panic!("expected successful ranking item");
        };
        assert!(score.is_finite());
    }
}
