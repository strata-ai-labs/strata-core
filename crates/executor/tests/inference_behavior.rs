//! Executor inference command behavior tests.

#![cfg(feature = "inference")]

use std::path::PathBuf;

use strata_executor::{
    Command, CommitOutcomeStatus, ErrorClass, Executor, ExecutorError, ExecutorErrorClass, Output,
    PageInfo, RetryPolicy,
};
use strata_inference::{
    ChatChoice, ChatMessage, ChatRequest, ChatResponse, EmbedInput, EmbeddingItem,
    EmbeddingsRequest, EmbeddingsResponse, FinishReason, InferenceCapability, InferenceError,
    InferenceRuntime, InferenceRuntimeConfig, ModelCacheStatus, ModelInfo, ModelTask,
    PullModelOutput, RankRequest, RankResponse, RankRuntimeOutcome, Role, Usage,
};

fn output_round_trip(value: &Output) -> Output {
    let encoded = serde_json::to_string(value).expect("output serializes");
    serde_json::from_str(&encoded).expect("output deserializes")
}

#[test]
fn inference_commands_round_trip_through_json() {
    let cases = vec![
        Command::InferenceModelsList,
        Command::InferenceModelsLocal,
        Command::InferenceModelsPull {
            model: "miniLM".to_owned(),
        },
        Command::InferenceModelCapability {
            model: "openai:gpt-4o-mini".to_owned(),
        },
        Command::InferenceGenerate {
            model: "openai:gpt-4o-mini".to_owned(),
            request: ChatRequest {
                prompt: Some("hello".to_owned()),
                max_tokens: Some(8),
                ..ChatRequest::default()
            },
        },
        Command::InferenceTokenize {
            model: "local:gpt2".to_owned(),
            text: "hello".to_owned(),
            add_special: true,
        },
        Command::InferenceDetokenize {
            model: "local:gpt2".to_owned(),
            ids: vec![1, 2, 3],
        },
        Command::InferenceEmbed {
            model: "openai:text-embedding-3-small".to_owned(),
            request: EmbeddingsRequest {
                input: EmbedInput::One("hello".to_owned()),
                dimensions: None,
                normalize: None,
                input_type: None,
                instruction: None,
            },
        },
        Command::InferenceEmbed {
            model: "openai:text-embedding-3-small".to_owned(),
            request: EmbeddingsRequest {
                input: EmbedInput::Many(vec!["a".to_owned(), "b".to_owned()]),
                dimensions: None,
                normalize: None,
                input_type: None,
                instruction: None,
            },
        },
        Command::InferenceRank {
            model: "local:jina-reranker-v1-tiny".to_owned(),
            request: RankRequest {
                query: "q".to_owned(),
                passages: vec!["p".to_owned()],
            },
        },
        Command::InferenceUnload {
            model: Some("local:gpt2".to_owned()),
        },
        Command::InferenceCacheStatus,
    ];

    for command in cases {
        let encoded = serde_json::to_string(&command).expect("command serializes");
        let decoded: Command = serde_json::from_str(&encoded).expect("command deserializes");
        assert_eq!(decoded, command);
    }
}

#[test]
fn inference_outputs_round_trip_through_json() {
    let model = ModelInfo {
        name: "miniLM".to_owned(),
        task: ModelTask::Embed,
        architecture: "bert".to_owned(),
        default_quant: "f16".to_owned(),
        embedding_dim: 384,
        is_local: false,
        local_path: None,
        size_bytes: 45_000_000,
        hf_repo: "stratalab-org/all-MiniLM-L6-v2-GGUF".to_owned(),
    };
    let capability = InferenceCapability {
        provider: strata_inference::ProviderKind::OpenAI,
        model: "gpt-4o-mini".to_owned(),
        can_generate: true,
        can_tokenize: false,
        can_embed: false,
        can_rank: false,
        requires_network: true,
        requires_api_key: true,
        provider_feature_enabled: true,
        network_enabled: true,
        embedding_dim: 0,
        supports_tools: true,
        supports_json_object: true,
        supports_json_schema: true,
        supports_logprobs: true,
    };
    let cases = vec![
        Output::InferenceModels {
            items: vec![model],
            page: PageInfo::terminal(),
        },
        Output::InferenceModelPulled(PullModelOutput {
            model: "miniLM".to_owned(),
            path: PathBuf::from("/tmp/miniLM.gguf"),
        }),
        Output::InferenceCapability(capability),
        Output::InferenceGeneration(ChatResponse {
            model: "openai:gpt-4o-mini".to_owned(),
            choices: vec![ChatChoice {
                index: 0,
                message: ChatMessage::new(Role::Assistant, "hello"),
                finish_reason: FinishReason::Stop,
                logprobs: None,
            }],
            usage: Usage {
                prompt_tokens: 2,
                completion_tokens: 1,
                total_tokens: 3,
            },
        }),
        Output::InferenceTokenIds(vec![1, 2, 3]),
        Output::InferenceText("hello".to_owned()),
        Output::InferenceEmbeddings(EmbeddingsResponse {
            model: "openai:text-embedding-3-small".to_owned(),
            data: vec![
                EmbeddingItem {
                    index: 0,
                    embedding: vec![0.1, 0.2, 0.3],
                },
                EmbeddingItem {
                    index: 1,
                    embedding: vec![0.4, 0.5, 0.6],
                },
            ],
            dimension: 3,
            usage: Usage {
                prompt_tokens: 4,
                completion_tokens: 0,
                total_tokens: 4,
            },
        }),
        Output::InferenceRanking(RankResponse {
            items: vec![
                RankRuntimeOutcome::Ok {
                    index: 0,
                    score: 0.9,
                },
                RankRuntimeOutcome::Error {
                    index: 1,
                    code: "inference.local_runtime_failed".to_owned(),
                    message: "runtime failed".to_owned(),
                },
            ],
        }),
        Output::InferenceUnloadResult { unloaded: true },
        Output::InferenceCacheStatus(ModelCacheStatus {
            generation_models: vec!["local:qwen3".to_owned()],
            embedding_models: vec!["openai:text-embedding-3-small".to_owned()],
            ranking_models: vec!["local:reranker".to_owned()],
        }),
    ];

    for output in cases {
        assert_eq!(output_round_trip(&output), output);
    }
}

#[test]
fn inference_errors_preserve_stable_code_class_and_redaction_through_executor() {
    let error: ExecutorError = InferenceError::Provider("OPENAI_API_KEY not set".to_owned()).into();
    assert_eq!(error.code(), "inference.missing_api_key");
    assert_eq!(error.class(), ExecutorErrorClass::Unavailable);
    assert_eq!(error.public_class(), ErrorClass::FailedPrecondition);
    assert_eq!(error.retry_policy(), RetryPolicy::AfterStateChange);
    assert_eq!(error.commit_outcome(), CommitOutcomeStatus::NotApplicable);
    assert!(error.retryable());

    let secret_error: ExecutorError =
        InferenceError::Provider("request failed for sk-test-secret".to_owned()).into();
    let encoded = serde_json::to_string(&secret_error).expect("error serializes");
    assert!(!secret_error.message().contains("sk-test-secret"));
    assert!(!encoded.contains("sk-test-secret"));
    assert!(encoded.contains("[REDACTED]"));
}

#[test]
fn inference_error_retry_policies_match_v1_contract() {
    let verification_error: ExecutorError =
        InferenceError::Registry("sha-256 hash mismatch".to_owned()).into();
    assert_eq!(
        verification_error.code(),
        "inference.download_verification_failed"
    );
    assert_eq!(
        verification_error.retry_policy(),
        RetryPolicy::AfterStateChange
    );

    let local_runtime_error: ExecutorError =
        InferenceError::LlamaCpp("context allocation failed".to_owned()).into();
    assert_eq!(local_runtime_error.code(), "inference.local_runtime_failed");
    assert_eq!(local_runtime_error.retry_policy(), RetryPolicy::Unknown);
}

#[test]
fn model_list_and_capability_execute_with_default_cloud_providers() {
    let mut executor = Executor::open_cache().expect("executor opens");
    let output = executor
        .execute(Command::InferenceModelsList)
        .expect("model list succeeds");
    let Output::InferenceModels { items: models, .. } = output else {
        panic!("expected inference model output");
    };
    assert!(models.iter().any(|model| model.name == "miniLM"));

    let output = executor
        .execute(Command::InferenceModelCapability {
            model: "openai:text-embedding-3-small".to_owned(),
        })
        .expect("capability succeeds");
    let Output::InferenceCapability(InferenceCapability {
        requires_api_key,
        requires_network,
        can_embed,
        provider_feature_enabled,
        network_enabled,
        ..
    }) = output
    else {
        panic!("expected capability output");
    };
    assert!(requires_api_key);
    assert!(requires_network);
    assert!(can_embed);
    assert!(provider_feature_enabled);
    assert!(network_enabled);
}

#[test]
fn cloud_generate_reports_missing_api_key_without_env() {
    let runtime = InferenceRuntime::new(InferenceRuntimeConfig {
        models_dir: None,
        network_enabled: true,
    });
    let mut executor = Executor::open_cache()
        .expect("executor opens")
        .with_inference_runtime(runtime);

    let previous = std::env::var_os("OPENAI_API_KEY");
    unsafe { std::env::remove_var("OPENAI_API_KEY") };
    let err = executor
        .execute(Command::InferenceGenerate {
            model: "openai:gpt-4o-mini".to_owned(),
            request: ChatRequest {
                prompt: Some("hello".to_owned()),
                max_tokens: Some(4),
                ..ChatRequest::default()
            },
        })
        .expect_err("missing API key is reported before provider call");
    if let Some(previous) = previous {
        unsafe { std::env::set_var("OPENAI_API_KEY", previous) };
    }
    assert_eq!(err.code(), "inference.missing_api_key");
    assert_eq!(err.public_class(), ErrorClass::FailedPrecondition);
    assert_eq!(err.retry_policy(), RetryPolicy::AfterStateChange);
}
