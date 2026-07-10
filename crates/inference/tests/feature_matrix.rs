//! Feature matrix behavior for partially compiled inference providers.

#![cfg(any(feature = "anthropic", feature = "openai", feature = "google"))]

#[test]
#[cfg(all(feature = "openai", not(feature = "anthropic")))]
fn anthropic_generation_requires_anthropic_feature_before_api_key() {
    let runtime = strata_inference::InferenceRuntime::default();
    let err = runtime
        .generate(
            "anthropic:claude-sonnet-4-6",
            &strata_inference::GenerateRequest {
                prompt: "hello".to_owned(),
                max_tokens: 4,
                ..strata_inference::GenerateRequest::default()
            },
        )
        .expect_err("anthropic feature is disabled");
    assert_eq!(err.code(), "inference.unsupported_provider");

    let err = strata_inference::load("anthropic:claude-sonnet-4-6")
        .expect_err("anthropic feature is disabled");
    assert_eq!(err.code(), "inference.unsupported_provider");
}

#[test]
#[cfg(all(feature = "openai", not(feature = "google")))]
fn google_embedding_requires_google_feature_before_api_key() {
    let runtime = strata_inference::InferenceRuntime::default();
    let err = runtime
        .embed(
            "google:gemini-embedding-001",
            &strata_inference::EmbedRequest {
                text: "embedded database".to_owned(),
            },
        )
        .expect_err("google feature is disabled");
    assert_eq!(err.code(), "inference.unsupported_provider");

    let err = strata_inference::load_embedder("google:gemini-embedding-001", None)
        .expect_err("google feature is disabled");
    assert_eq!(err.code(), "inference.unsupported_provider");
}

#[test]
#[cfg(all(feature = "openai", not(feature = "local")))]
fn local_single_embedding_requires_local_feature_before_cloud_policy() {
    let runtime =
        strata_inference::InferenceRuntime::new(strata_inference::InferenceRuntimeConfig {
            models_dir: None,
            network_enabled: false,
        });

    let err = runtime
        .embed(
            "local:miniLM",
            &strata_inference::EmbedRequest {
                text: "embedded database".to_owned(),
            },
        )
        .expect_err("local feature is disabled");

    assert_eq!(err.code(), "inference.unsupported_operation");
    assert!(
        err.to_string()
            .contains("local embedding requires the local feature"),
        "{err}"
    );
}
