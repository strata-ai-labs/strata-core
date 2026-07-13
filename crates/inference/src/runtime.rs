//! Runtime facade for model execution.

#[cfg(any(
    feature = "local",
    feature = "anthropic",
    feature = "openai",
    feature = "google"
))]
use std::collections::HashMap;
#[cfg(feature = "local")]
use std::path::Path;
use std::path::PathBuf;
#[cfg(any(
    feature = "local",
    feature = "anthropic",
    feature = "openai",
    feature = "google"
))]
use std::sync::Mutex;

use crate::{
    generation_provider_feature_enabled, parse_model_spec, GenerateRequest, GenerateResponse,
    InferenceError, ModelInfo, ModelRegistry, ModelTask, ProviderKind,
};

#[cfg(any(feature = "openai", feature = "google"))]
use crate::embedding_provider_feature_enabled;

#[cfg(any(feature = "openai", feature = "google"))]
use crate::InferenceEngine;

#[cfg(any(feature = "anthropic", feature = "openai", feature = "google"))]
use crate::api_key_env_var;

#[cfg(any(
    feature = "local",
    feature = "anthropic",
    feature = "openai",
    feature = "google"
))]
use crate::GenerationEngine;

#[cfg(any(feature = "openai", feature = "google"))]
use crate::CloudEmbeddingEngine;

#[cfg(feature = "local")]
use crate::{EmbeddingEngine, RankingEngine};

/// Runtime configuration for model execution.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "wire-schemas", derive(schemars::JsonSchema))]
pub struct InferenceRuntimeConfig {
    /// Optional model directory override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub models_dir: Option<PathBuf>,
    /// Whether provider network calls and model downloads are allowed.
    pub network_enabled: bool,
}

impl Default for InferenceRuntimeConfig {
    fn default() -> Self {
        Self {
            models_dir: None,
            network_enabled: true,
        }
    }
}

/// Model cache facts exposed for diagnostics.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "wire-schemas", derive(schemars::JsonSchema))]
pub struct ModelCacheStatus {
    /// Cached generation model specs.
    pub generation_models: Vec<String>,
    /// Cached embedding model specs.
    pub embedding_models: Vec<String>,
    /// Cached ranking model specs.
    pub ranking_models: Vec<String>,
}

/// Provider/model capability facts.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "wire-schemas", derive(schemars::JsonSchema))]
pub struct InferenceCapability {
    /// Provider kind.
    pub provider: ProviderKind,
    /// Model name or path after provider parsing.
    pub model: String,
    /// Whether generation is supported.
    pub can_generate: bool,
    /// Whether tokenization is supported.
    pub can_tokenize: bool,
    /// Whether embedding is supported.
    pub can_embed: bool,
    /// Whether ranking is supported.
    pub can_rank: bool,
    /// Whether the operation requires network access.
    pub requires_network: bool,
    /// Whether the provider requires an API key.
    pub requires_api_key: bool,
    /// Whether this binary was compiled with the provider feature needed for execution.
    pub provider_feature_enabled: bool,
    /// Whether this runtime configuration currently allows network access.
    pub network_enabled: bool,
    /// Known embedding dimension, if available.
    pub embedding_dim: usize,
}

/// Pull-model command output.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "wire-schemas", derive(schemars::JsonSchema))]
pub struct PullModelOutput {
    /// Requested model spec.
    pub model: String,
    /// Local path containing the resolved GGUF file.
    pub path: PathBuf,
}

/// Embedding request.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "wire-schemas", derive(schemars::JsonSchema))]
pub struct EmbedRequest {
    /// Text to embed.
    pub text: String,
}

/// Batch embedding response.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "wire-schemas", derive(schemars::JsonSchema))]
pub struct EmbedResponse {
    /// Ordered embedding item outcomes.
    pub items: Vec<EmbedRuntimeOutcome>,
    /// Embedding dimension when known.
    pub dimension: usize,
}

/// Embedding item outcome.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "wire-schemas", derive(schemars::JsonSchema))]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum EmbedRuntimeOutcome {
    /// Successful embedding item.
    Ok {
        /// Embedding vector.
        vector: Vec<f32>,
    },
    /// Failed embedding item.
    Error {
        /// Stable error code.
        code: String,
        /// Redacted public error message.
        message: String,
    },
}

/// Ranking request.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "wire-schemas", derive(schemars::JsonSchema))]
pub struct RankRequest {
    /// Query text.
    pub query: String,
    /// Candidate passages.
    pub passages: Vec<String>,
}

/// Ranking response.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "wire-schemas", derive(schemars::JsonSchema))]
pub struct RankResponse {
    /// Ordered ranking item outcomes.
    pub items: Vec<RankRuntimeOutcome>,
}

/// Ranking item outcome.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "wire-schemas", derive(schemars::JsonSchema))]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum RankRuntimeOutcome {
    /// Successful ranking score.
    Ok {
        /// Passage index.
        index: usize,
        /// Relevance score.
        score: f32,
    },
    /// Failed ranking item.
    Error {
        /// Passage index.
        index: usize,
        /// Stable error code.
        code: String,
        /// Redacted public error message.
        message: String,
    },
}

/// Public runtime facade.
#[derive(Debug)]
pub struct InferenceRuntime {
    config: InferenceRuntimeConfig,
    #[cfg(any(
        feature = "local",
        feature = "anthropic",
        feature = "openai",
        feature = "google"
    ))]
    generation: Mutex<HashMap<String, GenerationEngine>>,
    #[cfg(feature = "local")]
    embeddings: Mutex<HashMap<String, EmbeddingEngine>>,
    #[cfg(feature = "local")]
    rankers: Mutex<HashMap<String, RankingEngine>>,
}

impl Default for InferenceRuntime {
    fn default() -> Self {
        Self::new(InferenceRuntimeConfig::default())
    }
}

impl InferenceRuntime {
    /// Creates a runtime facade.
    pub fn new(config: InferenceRuntimeConfig) -> Self {
        Self {
            config,
            #[cfg(any(
                feature = "local",
                feature = "anthropic",
                feature = "openai",
                feature = "google"
            ))]
            generation: Mutex::new(HashMap::new()),
            #[cfg(feature = "local")]
            embeddings: Mutex::new(HashMap::new()),
            #[cfg(feature = "local")]
            rankers: Mutex::new(HashMap::new()),
        }
    }

    /// Returns catalog model information.
    pub fn list_models(&self) -> Vec<ModelInfo> {
        self.registry().list_available()
    }

    /// Returns locally available catalog model information.
    pub fn list_local_models(&self) -> Vec<ModelInfo> {
        self.registry().list_local()
    }

    /// Resolves or downloads a model into the local model directory.
    pub fn pull_model(&self, model: &str) -> Result<PullModelOutput, InferenceError> {
        if !self.config.network_enabled {
            return Err(InferenceError::NotSupported(
                "model download requires network access".to_owned(),
            ));
        }

        #[cfg(feature = "download")]
        {
            let path = self.registry().resolve_or_pull(model)?;
            Ok(PullModelOutput {
                model: model.to_owned(),
                path,
            })
        }

        #[cfg(not(feature = "download"))]
        {
            let _ = model;
            Err(InferenceError::NotSupported(
                "model download requires the download feature".to_owned(),
            ))
        }
    }

    /// Returns capability facts for a model spec.
    pub fn capability(&self, model_spec: &str) -> Result<InferenceCapability, InferenceError> {
        let (provider, model) = parse_model_spec(model_spec)?;
        let local_info = if provider == ProviderKind::Local {
            self.registry()
                .list_available()
                .into_iter()
                .find(|info| info.name.eq_ignore_ascii_case(&model))
        } else {
            None
        };
        let task = local_info.as_ref().map(|info| info.task);
        Ok(InferenceCapability {
            provider,
            model,
            can_generate: provider != ProviderKind::Local || task != Some(ModelTask::Embed),
            can_tokenize: provider == ProviderKind::Local,
            can_embed: provider == ProviderKind::OpenAI
                || provider == ProviderKind::Google
                || task == Some(ModelTask::Embed),
            can_rank: provider == ProviderKind::Local && task == Some(ModelTask::Rank),
            requires_network: provider != ProviderKind::Local,
            requires_api_key: provider != ProviderKind::Local,
            provider_feature_enabled: generation_provider_feature_enabled(provider)
                || embedding_provider_feature_enabled_for_capability(provider),
            network_enabled: self.config.network_enabled,
            embedding_dim: local_info.map_or(0, |info| info.embedding_dim),
        })
    }

    /// Generates text from a model spec.
    pub fn generate(
        &self,
        model_spec: &str,
        request: &GenerateRequest,
    ) -> Result<GenerateResponse, InferenceError> {
        #[cfg(any(
            feature = "local",
            feature = "anthropic",
            feature = "openai",
            feature = "google"
        ))]
        {
            let (provider, _model) = parse_model_spec(model_spec)?;
            if provider == ProviderKind::Local {
                let mut cache = self.lock_generation()?;
                let engine = self.cached_generation_engine(&mut cache, model_spec, None)?;
                return engine.generate(request);
            }

            if !self.config.network_enabled {
                return Err(InferenceError::NotSupported(
                    "cloud generation requires network access".to_owned(),
                ));
            }
            #[cfg(not(any(feature = "anthropic", feature = "openai", feature = "google")))]
            {
                Err(InferenceError::NotSupported(format!(
                    "{provider} provider not enabled"
                )))
            }
            #[cfg(any(feature = "anthropic", feature = "openai", feature = "google"))]
            {
                let mut engine = self.cloud_generation_engine(model_spec)?;
                engine.generate(request)
            }
        }

        #[cfg(not(any(
            feature = "local",
            feature = "anthropic",
            feature = "openai",
            feature = "google"
        )))]
        {
            let _ = (model_spec, request);
            Err(InferenceError::NotSupported(
                "generation requires a provider feature".to_owned(),
            ))
        }
    }

    /// Runs a chat/generation request (the OpenAI-shaped body).
    ///
    /// Local applies the model's chat template and the full sampler chain; cloud
    /// providers map messages natively (system prompt, multi-turn history,
    /// assistant prefill) and forward every knob the provider supports.
    /// `model_config` is threaded into local model loading (cache-keyed).
    pub fn chat(
        &self,
        model_spec: &str,
        request: &crate::wire::ChatRequest,
    ) -> Result<crate::wire::ChatResponse, InferenceError> {
        request.validate()?;

        #[cfg(any(
            feature = "local",
            feature = "anthropic",
            feature = "openai",
            feature = "google"
        ))]
        {
            let (provider, _model) = parse_model_spec(model_spec)?;
            if provider == ProviderKind::Local {
                let mut cache = self.lock_generation()?;
                let engine = self.cached_generation_engine(
                    &mut cache,
                    model_spec,
                    request.model_config.as_ref(),
                )?;
                let mut response = engine.generate_chat(request)?;
                response.model = model_spec.to_string();
                return Ok(response);
            }

            if !self.config.network_enabled {
                return Err(InferenceError::NotSupported(
                    "cloud generation requires network access".to_owned(),
                ));
            }
            #[cfg(not(any(feature = "anthropic", feature = "openai", feature = "google")))]
            {
                Err(InferenceError::NotSupported(format!(
                    "{provider} provider not enabled"
                )))
            }
            #[cfg(any(feature = "anthropic", feature = "openai", feature = "google"))]
            {
                let mut engine = self.cloud_generation_engine(model_spec)?;
                let mut response = engine.generate_chat(request)?;
                response.model = model_spec.to_string();
                Ok(response)
            }
        }

        #[cfg(not(any(
            feature = "local",
            feature = "anthropic",
            feature = "openai",
            feature = "google"
        )))]
        {
            let _ = (model_spec, request);
            Err(InferenceError::NotSupported(
                "chat requires a provider feature".to_owned(),
            ))
        }
    }

    /// Runs an embeddings request (single or batch, OpenAI-shaped).
    ///
    /// Phase A bridges to [`Self::embed_batch`]; `dimensions`/`normalize`/
    /// `input_type` are accepted but applied in a later phase.
    pub fn embeddings(
        &self,
        model_spec: &str,
        request: &crate::wire::EmbeddingsRequest,
    ) -> Result<crate::wire::EmbeddingsResponse, InferenceError> {
        let texts = request.input.to_vec();
        let batch = self.embed_batch(model_spec, &texts)?;
        let mut data = Vec::with_capacity(batch.items.len());
        for (index, item) in batch.items.into_iter().enumerate() {
            match item {
                EmbedRuntimeOutcome::Ok { vector } => data.push(crate::wire::EmbeddingItem {
                    index: index as u32,
                    embedding: vector,
                }),
                EmbedRuntimeOutcome::Error { code, message } => {
                    return Err(InferenceError::Provider(format!(
                        "embedding item {index} failed [{code}]: {message}"
                    )));
                }
            }
        }
        Ok(crate::wire::EmbeddingsResponse {
            model: model_spec.to_string(),
            dimension: batch.dimension,
            data,
            usage: crate::wire::Usage::default(),
        })
    }

    /// Tokenizes text with a local generation model.
    pub fn tokenize(
        &self,
        model_spec: &str,
        text: &str,
        add_special: bool,
    ) -> Result<Vec<u32>, InferenceError> {
        #[cfg(feature = "local")]
        {
            let mut cache = self.lock_generation()?;
            let engine = self.cached_generation_engine(&mut cache, model_spec, None)?;
            engine.encode(text, add_special)
        }

        #[cfg(not(feature = "local"))]
        {
            let _ = (model_spec, text, add_special);
            Err(InferenceError::NotSupported(
                "tokenization requires the local feature".to_owned(),
            ))
        }
    }

    /// Detokenizes local token ids.
    pub fn detokenize(&self, model_spec: &str, ids: &[u32]) -> Result<String, InferenceError> {
        #[cfg(feature = "local")]
        {
            let mut cache = self.lock_generation()?;
            let engine = self.cached_generation_engine(&mut cache, model_spec, None)?;
            engine.decode(ids)
        }

        #[cfg(not(feature = "local"))]
        {
            let _ = (model_spec, ids);
            Err(InferenceError::NotSupported(
                "detokenization requires the local feature".to_owned(),
            ))
        }
    }

    /// Embeds one text.
    pub fn embed(
        &self,
        model_spec: &str,
        request: &EmbedRequest,
    ) -> Result<Vec<f32>, InferenceError> {
        #[cfg(any(feature = "local", feature = "openai", feature = "google"))]
        {
            let (provider, _model) = parse_model_spec(model_spec)?;
            if provider == ProviderKind::Local {
                #[cfg(feature = "local")]
                {
                    let mut cache = self.lock_embeddings()?;
                    let engine = self.cached_embedding_engine(&mut cache, model_spec)?;
                    return engine.embed(&request.text);
                }
                #[cfg(not(feature = "local"))]
                {
                    return Err(InferenceError::NotSupported(
                        "local embedding requires the local feature".to_owned(),
                    ));
                }
            }

            if !self.config.network_enabled {
                return Err(InferenceError::NotSupported(
                    "cloud embedding requires network access".to_owned(),
                ));
            }
            #[cfg(any(feature = "openai", feature = "google"))]
            {
                let engine = self.cloud_embedding_engine(model_spec)?;
                engine.embed(&request.text)
            }
            #[cfg(not(any(feature = "openai", feature = "google")))]
            {
                Err(InferenceError::NotSupported(
                    "cloud embedding requires openai or google feature".to_owned(),
                ))
            }
        }

        #[cfg(not(any(feature = "local", feature = "openai", feature = "google")))]
        {
            let _ = (model_spec, request);
            Err(InferenceError::NotSupported(
                "embedding requires local, openai, or google feature".to_owned(),
            ))
        }
    }

    /// Embeds a batch of texts.
    pub fn embed_batch(
        &self,
        model_spec: &str,
        texts: &[String],
    ) -> Result<EmbedResponse, InferenceError> {
        let refs: Vec<&str> = texts.iter().map(String::as_str).collect();

        #[cfg(any(feature = "local", feature = "openai", feature = "google"))]
        {
            let (provider, _model) = parse_model_spec(model_spec)?;
            let embeddings = if provider == ProviderKind::Local {
                #[cfg(feature = "local")]
                {
                    let mut cache = self.lock_embeddings()?;
                    let engine = self.cached_embedding_engine(&mut cache, model_spec)?;
                    engine.embed_batch(&refs)?
                }
                #[cfg(not(feature = "local"))]
                {
                    return Err(InferenceError::NotSupported(
                        "local embedding requires the local feature".to_owned(),
                    ));
                }
            } else {
                if !self.config.network_enabled {
                    return Err(InferenceError::NotSupported(
                        "cloud embedding requires network access".to_owned(),
                    ));
                }
                #[cfg(any(feature = "openai", feature = "google"))]
                {
                    let engine = self.cloud_embedding_engine(model_spec)?;
                    engine.embed_batch(&refs)?
                }
                #[cfg(not(any(feature = "openai", feature = "google")))]
                {
                    return Err(InferenceError::NotSupported(
                        "cloud embedding requires openai or google feature".to_owned(),
                    ));
                }
            };
            let dimension = embeddings.first().map_or(0, Vec::len);
            Ok(EmbedResponse {
                dimension,
                items: embeddings
                    .into_iter()
                    .map(|vector| EmbedRuntimeOutcome::Ok { vector })
                    .collect(),
            })
        }

        #[cfg(not(any(feature = "local", feature = "openai", feature = "google")))]
        {
            let _ = (model_spec, refs);
            Err(InferenceError::NotSupported(
                "embedding requires local, openai, or google feature".to_owned(),
            ))
        }
    }

    /// Ranks passages against a query.
    pub fn rank(
        &self,
        model_spec: &str,
        request: &RankRequest,
    ) -> Result<RankResponse, InferenceError> {
        #[cfg(feature = "local")]
        {
            let refs: Vec<&str> = request.passages.iter().map(String::as_str).collect();
            let mut cache = self.lock_rankers()?;
            let engine = self.cached_ranking_engine(&mut cache, model_spec)?;
            let scores = engine.rank(&request.query, &refs)?;
            Ok(RankResponse {
                items: scores
                    .into_iter()
                    .enumerate()
                    .map(|(index, score)| RankRuntimeOutcome::Ok { index, score })
                    .collect(),
            })
        }

        #[cfg(not(feature = "local"))]
        {
            let _ = (model_spec, request);
            Err(InferenceError::NotSupported(
                "ranking requires the local feature".to_owned(),
            ))
        }
    }

    /// Unloads cached local models. When `model_spec` is `None`, all caches are cleared.
    pub fn unload(&self, _model_spec: Option<&str>) -> Result<bool, InferenceError> {
        #[cfg(any(
            feature = "local",
            feature = "anthropic",
            feature = "openai",
            feature = "google"
        ))]
        let mut unloaded = false;
        #[cfg(not(any(
            feature = "local",
            feature = "anthropic",
            feature = "openai",
            feature = "google"
        )))]
        let unloaded = false;
        #[cfg(any(
            feature = "local",
            feature = "anthropic",
            feature = "openai",
            feature = "google"
        ))]
        {
            let mut cache = self.lock_generation()?;
            unloaded |= remove_matching(&mut cache, _model_spec);
        }
        #[cfg(feature = "local")]
        {
            let mut cache = self.lock_embeddings()?;
            unloaded |= remove_matching(&mut cache, _model_spec);
            let mut cache = self.lock_rankers()?;
            unloaded |= remove_matching(&mut cache, _model_spec);
        }
        Ok(unloaded)
    }

    /// Returns model cache status.
    pub fn cache_status(&self) -> Result<ModelCacheStatus, InferenceError> {
        Ok(ModelCacheStatus {
            generation_models: {
                #[cfg(any(
                    feature = "local",
                    feature = "anthropic",
                    feature = "openai",
                    feature = "google"
                ))]
                {
                    let cache = self.lock_generation()?;
                    sorted_keys(&cache)
                }
                #[cfg(not(any(
                    feature = "local",
                    feature = "anthropic",
                    feature = "openai",
                    feature = "google"
                )))]
                {
                    Vec::new()
                }
            },
            embedding_models: {
                #[cfg(feature = "local")]
                {
                    let cache = self.lock_embeddings()?;
                    sorted_keys(&cache)
                }
                #[cfg(not(feature = "local"))]
                {
                    Vec::new()
                }
            },
            ranking_models: {
                #[cfg(feature = "local")]
                {
                    let cache = self.lock_rankers()?;
                    sorted_keys(&cache)
                }
                #[cfg(not(feature = "local"))]
                {
                    Vec::new()
                }
            },
        })
    }

    fn registry(&self) -> ModelRegistry {
        self.config
            .models_dir
            .clone()
            .map_or_else(ModelRegistry::new, ModelRegistry::with_dir)
    }

    #[cfg(any(
        feature = "local",
        feature = "anthropic",
        feature = "openai",
        feature = "google"
    ))]
    fn lock_generation(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, HashMap<String, GenerationEngine>>, InferenceError> {
        self.generation
            .lock()
            .map_err(|err| InferenceError::Io(format!("generation cache mutex poisoned: {err}")))
    }

    #[cfg(feature = "local")]
    fn lock_embeddings(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, HashMap<String, EmbeddingEngine>>, InferenceError> {
        self.embeddings
            .lock()
            .map_err(|err| InferenceError::Io(format!("embedding cache mutex poisoned: {err}")))
    }

    #[cfg(feature = "local")]
    fn lock_rankers(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, HashMap<String, RankingEngine>>, InferenceError> {
        self.rankers
            .lock()
            .map_err(|err| InferenceError::Io(format!("ranking cache mutex poisoned: {err}")))
    }

    #[cfg(any(
        feature = "local",
        feature = "anthropic",
        feature = "openai",
        feature = "google"
    ))]
    fn cached_generation_engine<'a>(
        &self,
        cache: &'a mut HashMap<String, GenerationEngine>,
        model_spec: &str,
        config: Option<&crate::wire::ModelConfig>,
    ) -> Result<&'a mut GenerationEngine, InferenceError> {
        let key = engine_cache_key(model_spec, config);
        if !cache.contains_key(&key) {
            let engine = self.local_generation_engine(model_spec, config)?;
            cache.insert(key.clone(), engine);
        }
        Ok(cache
            .get_mut(&key)
            .expect("generation engine inserted before lookup"))
    }

    #[cfg(feature = "local")]
    fn cached_embedding_engine<'a>(
        &self,
        cache: &'a mut HashMap<String, EmbeddingEngine>,
        model_spec: &str,
    ) -> Result<&'a EmbeddingEngine, InferenceError> {
        if !cache.contains_key(model_spec) {
            let engine = self.local_embedding_engine(model_spec)?;
            cache.insert(model_spec.to_owned(), engine);
        }
        Ok(cache
            .get(model_spec)
            .expect("embedding engine inserted before lookup"))
    }

    #[cfg(feature = "local")]
    fn cached_ranking_engine<'a>(
        &self,
        cache: &'a mut HashMap<String, RankingEngine>,
        model_spec: &str,
    ) -> Result<&'a RankingEngine, InferenceError> {
        if !cache.contains_key(model_spec) {
            let engine = self.local_ranking_engine(model_spec)?;
            cache.insert(model_spec.to_owned(), engine);
        }
        Ok(cache
            .get(model_spec)
            .expect("ranking engine inserted before lookup"))
    }

    #[cfg(any(
        feature = "local",
        feature = "anthropic",
        feature = "openai",
        feature = "google"
    ))]
    fn local_generation_engine(
        &self,
        model_spec: &str,
        config: Option<&crate::wire::ModelConfig>,
    ) -> Result<GenerationEngine, InferenceError> {
        let (provider, model) = parse_model_spec(model_spec)?;
        if provider != ProviderKind::Local {
            return Err(InferenceError::NotSupported(
                "generation cache only stores local models".to_owned(),
            ));
        }
        #[cfg(feature = "local")]
        {
            if looks_like_path(&model) {
                return GenerationEngine::from_gguf_with_config(Path::new(&model), config);
            }
            GenerationEngine::from_registry_with_config(&model, config)
        }
        #[cfg(not(feature = "local"))]
        {
            let _ = (model, config);
            Err(InferenceError::NotSupported(
                "local generation requires the local feature".to_owned(),
            ))
        }
    }

    #[cfg(any(feature = "anthropic", feature = "openai", feature = "google"))]
    fn cloud_generation_engine(
        &self,
        model_spec: &str,
    ) -> Result<GenerationEngine, InferenceError> {
        let (provider, model) = parse_model_spec(model_spec)?;
        if !generation_provider_feature_enabled(provider) {
            return Err(InferenceError::NotSupported(format!(
                "{provider} provider not enabled"
            )));
        }
        let api_key = api_key(provider)?;
        GenerationEngine::cloud(provider, api_key, model)
    }

    #[cfg(feature = "local")]
    fn local_embedding_engine(&self, model_spec: &str) -> Result<EmbeddingEngine, InferenceError> {
        let (provider, model) = parse_model_spec(model_spec)?;
        if provider != ProviderKind::Local {
            return Err(InferenceError::NotSupported(
                "local embedding requires local provider".to_owned(),
            ));
        }
        if looks_like_path(&model) {
            return EmbeddingEngine::from_gguf(Path::new(&model));
        }
        EmbeddingEngine::from_registry(&model)
    }

    #[cfg(feature = "local")]
    fn local_ranking_engine(&self, model_spec: &str) -> Result<RankingEngine, InferenceError> {
        let (provider, model) = parse_model_spec(model_spec)?;
        if provider != ProviderKind::Local {
            return Err(InferenceError::NotSupported(
                "local ranking requires local provider".to_owned(),
            ));
        }
        if looks_like_path(&model) {
            return RankingEngine::from_gguf(Path::new(&model));
        }
        RankingEngine::from_registry(&model)
    }

    #[cfg(any(feature = "openai", feature = "google"))]
    fn cloud_embedding_engine(
        &self,
        model_spec: &str,
    ) -> Result<CloudEmbeddingEngine, InferenceError> {
        let (provider, model) = parse_model_spec(model_spec)?;
        if !embedding_provider_feature_enabled(provider) {
            return Err(InferenceError::NotSupported(format!(
                "{provider} embedding provider not enabled"
            )));
        }
        let api_key = api_key(provider)?;
        CloudEmbeddingEngine::new(provider, api_key, model)
    }
}

#[cfg(any(feature = "anthropic", feature = "openai", feature = "google"))]
fn api_key(provider: ProviderKind) -> Result<String, InferenceError> {
    if provider == ProviderKind::Local {
        return Err(InferenceError::NotSupported(
            "local provider does not use API keys".to_owned(),
        ));
    }
    let env_var = api_key_env_var(provider);
    std::env::var(env_var).map_err(|_| {
        InferenceError::Provider(format!(
            "{env_var} not set (required for {provider} provider)"
        ))
    })
}

/// Cache key for a loaded generation engine. A default or absent config shares
/// the plain-spec key (so tokenize/generate and a config-less chat reuse one
/// engine); a non-default config keys a distinct engine so its load params
/// (`n_ctx`/`n_gpu_layers`/`n_batch`/`n_threads`) take effect.
#[cfg(any(
    feature = "local",
    feature = "anthropic",
    feature = "openai",
    feature = "google"
))]
fn engine_cache_key(model_spec: &str, config: Option<&crate::wire::ModelConfig>) -> String {
    match config {
        Some(c) if *c != crate::wire::ModelConfig::default() => format!("{model_spec}\u{0}{c:?}"),
        _ => model_spec.to_owned(),
    }
}

#[cfg(feature = "local")]
fn looks_like_path(model: &str) -> bool {
    model.ends_with(".gguf")
        || model.contains('/')
        || model.contains('\\')
        || Path::new(model).exists()
}

#[cfg(any(
    feature = "local",
    feature = "anthropic",
    feature = "openai",
    feature = "google"
))]
fn sorted_keys<T>(map: &HashMap<String, T>) -> Vec<String> {
    let mut keys: Vec<String> = map.keys().cloned().collect();
    keys.sort();
    keys
}

#[cfg(any(
    feature = "local",
    feature = "anthropic",
    feature = "openai",
    feature = "google"
))]
fn remove_matching<T>(map: &mut HashMap<String, T>, model_spec: Option<&str>) -> bool {
    match model_spec {
        Some(model_spec) => map.remove(model_spec).is_some(),
        None => {
            let had_entries = !map.is_empty();
            map.clear();
            had_entries
        }
    }
}

fn embedding_provider_feature_enabled_for_capability(provider: ProviderKind) -> bool {
    match provider {
        ProviderKind::Local => cfg!(feature = "local"),
        ProviderKind::OpenAI => cfg!(feature = "openai"),
        ProviderKind::Google => cfg!(feature = "google"),
        ProviderKind::Anthropic => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_allows_network() {
        let config = InferenceRuntimeConfig::default();
        assert!(config.network_enabled);
        assert!(config.models_dir.is_none());
    }

    #[test]
    fn capability_for_local_embed_model_reports_embedding() {
        let runtime = InferenceRuntime::default();
        let capability = runtime.capability("local:miniLM").expect("capability");
        assert_eq!(capability.provider, ProviderKind::Local);
        assert!(capability.can_embed);
        assert!(capability.can_tokenize);
        assert!(!capability.requires_api_key);
        assert_eq!(capability.embedding_dim, 384);
    }

    #[test]
    fn capability_for_openai_reports_network_and_api_key() {
        let runtime = InferenceRuntime::default();
        let capability = runtime
            .capability("openai:text-embedding-3-small")
            .expect("capability");
        assert_eq!(capability.provider, ProviderKind::OpenAI);
        assert!(capability.can_generate);
        assert!(capability.can_embed);
        assert!(capability.requires_network);
        assert!(capability.requires_api_key);
    }

    #[test]
    fn cache_status_is_empty_by_default() {
        let runtime = InferenceRuntime::default();
        assert_eq!(
            runtime.cache_status().expect("cache status"),
            ModelCacheStatus::default()
        );
    }
}
