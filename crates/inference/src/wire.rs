//! OpenAI-shaped wire DTOs for the inference command surface.
//!
//! These are the request/response bodies the executor, IDL, and SDKs speak.
//! We adopt the OpenAI Chat Completions and Embeddings *body schemas* — the
//! lingua franca every AI developer and coding agent already knows — inside
//! Strata's own envelope, so the surface stays small while exposing the full
//! knob set. Provider- and llama.cpp-specific knobs ride along as a documented
//! set of **flat optional extension fields** (`top_k`, `min_p`, `mirostat`, …),
//! the vLLM `extra_body` pattern.
//!
//! Two config tiers: per-call sampling lives on the request; per-model load
//! params live on [`ModelConfig`], set once and cache-keyed (threaded through
//! in a later phase).
//!
//! Phase A lands these types plus a naive bridge to the current engine path
//! ([`ChatRequest::to_internal_generate`]); phases B/C replace the bridge with
//! real chat templating (local, via the GGUF chat-template cascade), the full
//! sampler chain, and native cloud message mapping.

use std::collections::BTreeMap;

use crate::{GenerateRequest, GenerateResponse, InferenceError, StopReason};

// ---------------------------------------------------------------------------
// Chat
// ---------------------------------------------------------------------------

/// A chat message role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "wire-schemas", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// System / developer instructions.
    System,
    /// End-user turn.
    User,
    /// Model turn (also used for assistant prefill).
    Assistant,
    /// Tool result turn (function calling — phase 2).
    Tool,
}

/// One chat message. (Content is text in this phase; typed multimodal parts and
/// tool calls arrive in a later phase.)
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "wire-schemas", derive(schemars::JsonSchema))]
pub struct ChatMessage {
    /// Message author.
    pub role: Role,
    /// Message text.
    pub content: String,
    /// Optional author name (OpenAI `name`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl ChatMessage {
    /// A convenience constructor.
    pub fn new(role: Role, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
            name: None,
        }
    }
}

/// Output format constraint. (`json_schema` and tools arrive with structured
/// outputs in a later phase.)
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "wire-schemas", derive(schemars::JsonSchema))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponseFormat {
    /// Free-form text (default).
    Text,
    /// Constrain output to a single JSON object.
    JsonObject,
}

/// Mirostat perplexity-control sampling (llama.cpp extension).
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "wire-schemas", derive(schemars::JsonSchema))]
pub struct Mirostat {
    /// Algorithm version: 1 or 2.
    pub mode: u8,
    /// Target entropy (tau).
    pub tau: f32,
    /// Learning rate (eta).
    pub eta: f32,
}

/// A generation request: the OpenAI Chat Completions body plus Strata /
/// llama.cpp extension fields.
///
/// Exactly one of `messages` (chat) or `prompt` (raw completion) must be set.
/// Unknown fields are ignored (OpenAI-compatible, forward-compatible with
/// provider `extra_body`), a deliberate exception to Strata's strict-wire
/// default for this knob-rich body.
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "wire-schemas", derive(schemars::JsonSchema))]
#[serde(default)]
pub struct ChatRequest {
    // --- input: messages XOR prompt ---
    /// Chat messages (system/user/assistant/tool).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub messages: Option<Vec<ChatMessage>>,
    /// Raw completion prompt (base models / full control).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,

    // --- OpenAI core sampling (all optional) ---
    /// Maximum completion tokens.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    /// Sampling temperature.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// Nucleus sampling cutoff.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    /// Stop sequences.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<Vec<String>>,
    /// Deterministic sampling seed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
    /// Frequency penalty.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frequency_penalty: Option<f32>,
    /// Presence penalty.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presence_penalty: Option<f32>,
    /// Per-token logit bias (token id → bias).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logit_bias: Option<BTreeMap<u32, f32>>,
    /// Whether to return log-probabilities.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logprobs: Option<bool>,
    /// Number of top log-probabilities to return per token.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_logprobs: Option<u32>,
    /// Output format constraint.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_format: Option<ResponseFormat>,

    // --- Strata / llama.cpp extensions (flat, documented) ---
    /// Top-k sampling cutoff.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<u32>,
    /// Min-p sampling cutoff.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_p: Option<f32>,
    /// Typical-p (locally typical) sampling.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub typical_p: Option<f32>,
    /// Tail-free sampling z.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tfs_z: Option<f32>,
    /// Repetition penalty.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repeat_penalty: Option<f32>,
    /// Repetition penalty look-back window.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repeat_last_n: Option<u32>,
    /// Mirostat sampling.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mirostat: Option<Mirostat>,
    /// GBNF grammar for constrained generation (local).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grammar: Option<String>,
    /// Token-id stop sequences (local).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_token_ids: Option<Vec<u32>>,

    // --- tier-2 load config (cache-keyed; threaded in a later phase) ---
    /// Per-model load/context configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_config: Option<ModelConfig>,
}

impl ChatRequest {
    /// Validates the input invariant: exactly one of `messages`/`prompt`, and
    /// non-empty.
    pub fn validate(&self) -> Result<(), InferenceError> {
        match (&self.messages, &self.prompt) {
            (Some(_), Some(_)) => Err(InferenceError::InvalidSpec(
                "set exactly one of `messages` or `prompt`, not both".to_string(),
            )),
            (None, None) => Err(InferenceError::InvalidSpec(
                "request must set either `messages` or `prompt`".to_string(),
            )),
            (Some(messages), None) if messages.is_empty() => Err(InferenceError::InvalidSpec(
                "`messages` must not be empty".to_string(),
            )),
            _ => Ok(()),
        }
    }

    /// Renders this request to a flat prompt string.
    ///
    /// PLACEHOLDER for phase A: `prompt` is used verbatim; `messages` get a
    /// naive `Role: content` join. Phase B replaces this with the real
    /// per-model chat-template cascade (local) and native message mapping
    /// (cloud).
    pub fn to_prompt(&self) -> String {
        if let Some(prompt) = &self.prompt {
            return prompt.clone();
        }
        let mut out = String::new();
        for message in self.messages.iter().flatten() {
            let tag = match message.role {
                Role::System => "System",
                Role::User => "User",
                Role::Assistant => "Assistant",
                Role::Tool => "Tool",
            };
            out.push_str(tag);
            out.push_str(": ");
            out.push_str(&message.content);
            out.push('\n');
        }
        out.push_str("Assistant:");
        out
    }

    /// Bridges to the current internal engine request (the seam phases B/C
    /// replace). Only the sampling knobs the current sampler understands are
    /// mapped; the richer extensions are wired later.
    pub fn to_internal_generate(&self) -> GenerateRequest {
        GenerateRequest {
            prompt: self.to_prompt(),
            max_tokens: self.max_tokens.unwrap_or(256) as usize,
            temperature: self.temperature.unwrap_or(0.0),
            top_k: self.top_k.unwrap_or(0) as usize,
            top_p: self.top_p.unwrap_or(1.0),
            seed: self.seed,
            stop_sequences: self.stop.clone().unwrap_or_default(),
            stop_tokens: self.stop_token_ids.clone().unwrap_or_default(),
            grammar: self.grammar.clone(),
        }
    }
}

/// Why generation stopped (OpenAI `finish_reason` + Strata `cancelled`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "wire-schemas", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum FinishReason {
    /// Hit a stop token or stop sequence.
    Stop,
    /// Hit the max-token or context limit.
    Length,
    /// Stopped to call tools (phase 2).
    ToolCalls,
    /// Stopped by a content filter.
    ContentFilter,
    /// Cancelled by provider policy or caller control.
    Cancelled,
}

impl From<StopReason> for FinishReason {
    fn from(reason: StopReason) -> Self {
        match reason {
            StopReason::StopToken => FinishReason::Stop,
            StopReason::MaxTokens | StopReason::ContextLength => FinishReason::Length,
            StopReason::Cancelled => FinishReason::Cancelled,
        }
    }
}

/// Token usage accounting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "wire-schemas", derive(schemars::JsonSchema))]
pub struct Usage {
    /// Prompt token count.
    pub prompt_tokens: u32,
    /// Completion token count.
    pub completion_tokens: u32,
    /// Total tokens.
    pub total_tokens: u32,
}

/// One generation choice.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "wire-schemas", derive(schemars::JsonSchema))]
pub struct ChatChoice {
    /// Choice index.
    pub index: u32,
    /// The assistant message.
    pub message: ChatMessage,
    /// Why this choice stopped.
    pub finish_reason: FinishReason,
}

/// A generation response (OpenAI-shaped, minimal).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "wire-schemas", derive(schemars::JsonSchema))]
pub struct ChatResponse {
    /// Resolved model spec.
    pub model: String,
    /// Generation choices (one today).
    pub choices: Vec<ChatChoice>,
    /// Token usage.
    pub usage: Usage,
}

impl ChatResponse {
    /// Wraps the current internal engine response (the phase-A bridge).
    pub fn from_internal(model: impl Into<String>, response: GenerateResponse) -> Self {
        let usage = Usage {
            prompt_tokens: response.prompt_tokens as u32,
            completion_tokens: response.completion_tokens as u32,
            total_tokens: (response.prompt_tokens + response.completion_tokens) as u32,
        };
        Self {
            model: model.into(),
            choices: vec![ChatChoice {
                index: 0,
                message: ChatMessage::new(Role::Assistant, response.text),
                finish_reason: response.stop_reason.into(),
            }],
            usage,
        }
    }
}

// ---------------------------------------------------------------------------
// Model load config (tier 2)
// ---------------------------------------------------------------------------

/// Embedding pooling strategy (a context-creation param — load-time).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "wire-schemas", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum Pooling {
    /// Mean over tokens.
    Mean,
    /// CLS token.
    Cls,
    /// Last token.
    Last,
    /// Reranker pooling.
    Rank,
}

/// Per-model load/context configuration (llama.cpp local; ignored by cloud).
/// Set once and cache-keyed — not repeated per call.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "wire-schemas", derive(schemars::JsonSchema))]
#[serde(default)]
pub struct ModelConfig {
    /// Context window size.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub n_ctx: Option<u32>,
    /// GPU layers to offload (-1 = all).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub n_gpu_layers: Option<i32>,
    /// Logical batch size.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub n_batch: Option<u32>,
    /// CPU threads.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub n_threads: Option<u32>,
    /// Enable flash attention.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flash_attn: Option<bool>,
    /// Embedding pooling strategy.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pooling: Option<Pooling>,
    /// Named chat template (e.g. `"chatml"`, `"llama3"`, `"gemma"`) overriding
    /// the model's embedded `tokenizer.chat_template`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chat_format: Option<String>,
}

// ---------------------------------------------------------------------------
// Embeddings (collapses embed + embed_batch)
// ---------------------------------------------------------------------------

/// Embedding input: a single string or a batch.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "wire-schemas", derive(schemars::JsonSchema))]
#[serde(untagged)]
pub enum EmbedInput {
    /// One text.
    One(String),
    /// A batch of texts.
    Many(Vec<String>),
}

impl EmbedInput {
    /// Flattens to a vector of texts.
    pub fn to_vec(&self) -> Vec<String> {
        match self {
            EmbedInput::One(text) => vec![text.clone()],
            EmbedInput::Many(texts) => texts.clone(),
        }
    }
}

/// Instruction-tuned embedder input role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "wire-schemas", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum InputType {
    /// Embed as a search query.
    Query,
    /// Embed as a document/passage.
    Document,
}

/// An embeddings request (the OpenAI Embeddings body plus extensions).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "wire-schemas", derive(schemars::JsonSchema))]
pub struct EmbeddingsRequest {
    /// Text(s) to embed.
    pub input: EmbedInput,
    /// Truncate to this many dimensions (matryoshka), then renormalize.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dimensions: Option<u32>,
    /// Force L2 normalization on/off (default per-model).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub normalize: Option<bool>,
    /// Query vs document role for instruction-tuned embedders.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_type: Option<InputType>,
    /// Explicit instruction prefix override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instruction: Option<String>,
}

/// One embedding result.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "wire-schemas", derive(schemars::JsonSchema))]
pub struct EmbeddingItem {
    /// Position in the input batch.
    pub index: u32,
    /// The embedding vector.
    pub embedding: Vec<f32>,
}

/// An embeddings response (OpenAI-shaped).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "wire-schemas", derive(schemars::JsonSchema))]
pub struct EmbeddingsResponse {
    /// Resolved model spec.
    pub model: String,
    /// One item per input, in order.
    pub data: Vec<EmbeddingItem>,
    /// Embedding dimension.
    pub dimension: usize,
    /// Token usage (when known).
    pub usage: Usage,
}

// ---------------------------------------------------------------------------
// Rerank (light enrichment over the current rank surface)
// ---------------------------------------------------------------------------

/// A rerank request.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "wire-schemas", derive(schemars::JsonSchema))]
pub struct RerankRequest {
    /// Query text.
    pub query: String,
    /// Candidate documents.
    pub documents: Vec<String>,
    /// Return only the top-N results.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_n: Option<u32>,
    /// Echo the document text in each result.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub return_documents: Option<bool>,
    /// Optional task instruction for instruction-tuned rerankers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instruction: Option<String>,
}

/// One rerank result.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "wire-schemas", derive(schemars::JsonSchema))]
pub struct RerankResult {
    /// Index into the input documents.
    pub index: u32,
    /// Relevance score (higher is more relevant).
    pub score: f32,
    /// The document text, when `return_documents` is set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document: Option<String>,
}

/// A rerank response.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "wire-schemas", derive(schemars::JsonSchema))]
pub struct RerankResponse {
    /// Resolved model spec.
    pub model: String,
    /// Results ordered as requested.
    pub results: Vec<RerankResult>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_request_validates_messages_xor_prompt() {
        let mut req = ChatRequest::default();
        assert!(req.validate().is_err(), "neither set");

        req.prompt = Some("hi".into());
        assert!(req.validate().is_ok());

        req.messages = Some(vec![ChatMessage::new(Role::User, "hi")]);
        assert!(req.validate().is_err(), "both set");

        req.prompt = None;
        assert!(req.validate().is_ok());

        req.messages = Some(vec![]);
        assert!(req.validate().is_err(), "empty messages");
    }

    #[test]
    fn chat_request_wire_is_minimal() {
        // Only set fields serialize.
        let req = ChatRequest {
            prompt: Some("hello".into()),
            temperature: Some(0.7),
            top_k: Some(40),
            ..Default::default()
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"prompt\":\"hello\""));
        assert!(json.contains("\"temperature\":0.7"));
        assert!(json.contains("\"top_k\":40"));
        assert!(!json.contains("messages"));
        assert!(!json.contains("min_p"));
    }

    #[test]
    fn chat_request_ignores_unknown_fields() {
        // Forward-compatible / extra_body: unknown knobs do not fail parsing.
        let json = r#"{"prompt":"hi","some_future_knob":123}"#;
        let req: ChatRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.prompt.as_deref(), Some("hi"));
    }

    #[test]
    fn extensions_round_trip() {
        let req = ChatRequest {
            messages: Some(vec![
                ChatMessage::new(Role::System, "be terse"),
                ChatMessage::new(Role::User, "hi"),
            ]),
            min_p: Some(0.05),
            mirostat: Some(Mirostat {
                mode: 2,
                tau: 5.0,
                eta: 0.1,
            }),
            response_format: Some(ResponseFormat::JsonObject),
            model_config: Some(ModelConfig {
                n_ctx: Some(8192),
                n_gpu_layers: Some(-1),
                ..Default::default()
            }),
            ..Default::default()
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: ChatRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req, back);
    }

    #[test]
    fn response_format_tags() {
        assert_eq!(
            serde_json::to_string(&ResponseFormat::JsonObject).unwrap(),
            r#"{"type":"json_object"}"#
        );
    }

    #[test]
    fn to_internal_maps_sampling() {
        let req = ChatRequest {
            prompt: Some("once".into()),
            max_tokens: Some(64),
            temperature: Some(0.8),
            top_p: Some(0.9),
            top_k: Some(40),
            stop: Some(vec!["\n".into()]),
            ..Default::default()
        };
        let internal = req.to_internal_generate();
        assert_eq!(internal.prompt, "once");
        assert_eq!(internal.max_tokens, 64);
        assert_eq!(internal.temperature, 0.8);
        assert_eq!(internal.top_p, 0.9);
        assert_eq!(internal.top_k, 40);
        assert_eq!(internal.stop_sequences, vec!["\n".to_string()]);
    }

    #[test]
    fn to_prompt_renders_messages() {
        let req = ChatRequest {
            messages: Some(vec![
                ChatMessage::new(Role::System, "be terse"),
                ChatMessage::new(Role::User, "hi"),
            ]),
            ..Default::default()
        };
        let prompt = req.to_prompt();
        assert!(prompt.contains("System: be terse"));
        assert!(prompt.contains("User: hi"));
        assert!(prompt.ends_with("Assistant:"));
    }

    #[test]
    fn finish_reason_maps_from_stop_reason() {
        assert_eq!(
            FinishReason::from(StopReason::StopToken),
            FinishReason::Stop
        );
        assert_eq!(
            FinishReason::from(StopReason::MaxTokens),
            FinishReason::Length
        );
        assert_eq!(
            FinishReason::from(StopReason::ContextLength),
            FinishReason::Length
        );
        assert_eq!(
            FinishReason::from(StopReason::Cancelled),
            FinishReason::Cancelled
        );
    }

    #[test]
    fn chat_response_from_internal_shapes_choices() {
        let internal = GenerateResponse {
            text: "hello there".into(),
            stop_reason: StopReason::StopToken,
            prompt_tokens: 3,
            completion_tokens: 2,
        };
        let response = ChatResponse::from_internal("local:qwen3", internal);
        assert_eq!(response.model, "local:qwen3");
        assert_eq!(response.choices.len(), 1);
        assert_eq!(response.choices[0].message.content, "hello there");
        assert_eq!(response.choices[0].finish_reason, FinishReason::Stop);
        assert_eq!(response.usage.total_tokens, 5);
    }

    #[test]
    fn embed_input_single_or_batch() {
        let one: EmbedInput = serde_json::from_str(r#""hi""#).unwrap();
        assert_eq!(one.to_vec(), vec!["hi".to_string()]);
        let many: EmbedInput = serde_json::from_str(r#"["a","b"]"#).unwrap();
        assert_eq!(many.to_vec(), vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn embeddings_request_round_trips() {
        let req = EmbeddingsRequest {
            input: EmbedInput::Many(vec!["a".into(), "b".into()]),
            dimensions: Some(256),
            normalize: Some(true),
            input_type: Some(InputType::Query),
            instruction: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: EmbeddingsRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req, back);
    }
}
