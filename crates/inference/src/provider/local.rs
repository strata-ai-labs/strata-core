//! Local generation provider backed by llama.cpp.
//!
//! llama.cpp speaks `i32` token ids and counts; the casts at this boundary
//! are the interface, not accidents.
#![allow(clippy::cast_sign_loss, clippy::cast_possible_wrap)]
//!
//! [`LocalProvider`] wraps a [`LlamaCppContext`] loaded for generation and
//! implements the autoregressive decode loop with sampler chain support.

use std::fmt::Write as _;
use std::path::Path;

use crate::llama::context::LlamaCppContext;
use crate::llama::ffi::{llama_api_lock, LlamaSampler};
use crate::wire::{
    ChatRequest, ChatResponse, FinishReason, ModelConfig, NamedToolChoice, ResponseFormat, Role,
    Tool, ToolChoice, ToolChoiceMode,
};
use crate::{GenerateRequest, GenerateResponse, InferenceError, StopReason};

/// Local generation provider using llama.cpp.
///
/// Not thread-safe on its own — the caller (`GenerationEngine`) is responsible
/// for synchronization if needed.
pub(crate) struct LocalProvider {
    ctx: LlamaCppContext,
}

impl LocalProvider {
    /// Load from a GGUF file with an optional model/context configuration.
    pub(crate) fn from_gguf(
        path: &Path,
        config: Option<&ModelConfig>,
    ) -> Result<Self, InferenceError> {
        let ctx = LlamaCppContext::load_for_generation(path, config)?;
        Ok(Self { ctx })
    }

    /// Generate text from a prompt using the loaded model.
    ///
    /// Flow:
    /// 1. Tokenize prompt
    /// 2. Validate (non-empty, fits in context)
    /// 3. Clear memory state
    /// 4. Build sampler chain
    /// 5. Prefill prompt tokens
    /// 6. Autoregressive decode loop with stop condition checks
    /// 7. Cleanup (free sampler, clear memory)
    /// 8. Detokenize and return response
    pub(crate) fn generate(
        &mut self,
        request: &GenerateRequest,
    ) -> Result<GenerateResponse, InferenceError> {
        let _api_guard = llama_api_lock();

        // 1. Tokenize prompt
        let mut prompt_tokens = self.ctx.tokenize(&request.prompt, true);
        let prompt_token_count = prompt_tokens.len();

        // 2. Validate
        if prompt_tokens.is_empty() {
            return Err(InferenceError::LlamaCpp(
                "prompt tokenized to empty sequence".to_string(),
            ));
        }

        if prompt_tokens.len() >= self.ctx.n_ctx {
            return Err(InferenceError::LlamaCpp(format!(
                "prompt length ({}) exceeds context size ({})",
                prompt_tokens.len(),
                self.ctx.n_ctx,
            )));
        }

        // Handle max_tokens=0 early
        if request.max_tokens == 0 {
            return Ok(GenerateResponse {
                text: String::new(),
                stop_reason: StopReason::MaxTokens,
                prompt_tokens: prompt_token_count,
                completion_tokens: 0,
            });
        }

        // 3. Clear any previous state
        self.ctx.clear_memory();

        // 4. Build sampler chain
        let sampler = self.build_sampler(request)?;

        // 5. Prefill: decode all prompt tokens at once
        let batch = self.ctx.api.batch_get_one(&mut prompt_tokens);
        if let Err(e) = self.ctx.api.decode(self.ctx.ctx, batch) {
            self.ctx.api.sampler_free(sampler);
            self.ctx.clear_memory();
            return Err(InferenceError::LlamaCpp(e));
        }

        // Sample first token
        let mut next_token = self.ctx.api.sampler_sample(sampler, self.ctx.ctx, -1);

        // 6. Decode loop
        let mut generated_ids: Vec<u32> = Vec::new();
        let mut stop_reason = StopReason::MaxTokens;
        let mut pos = prompt_tokens.len();

        for _step in 0..request.max_tokens {
            // Check EOG (end-of-generation) tokens
            if self.ctx.api.vocab_is_eog(self.ctx.vocab, next_token) {
                stop_reason = StopReason::StopToken;
                break;
            }

            // Check user-provided stop tokens
            if request.stop_tokens.contains(&(next_token as u32)) {
                stop_reason = StopReason::StopToken;
                break;
            }

            generated_ids.push(next_token as u32);

            pos += 1;
            if pos >= self.ctx.n_ctx {
                stop_reason = StopReason::ContextLength;
                break;
            }

            // Decode single token
            let mut single = [next_token];
            let batch = self.ctx.api.batch_get_one(&mut single);
            if let Err(e) = self.ctx.api.decode(self.ctx.ctx, batch) {
                self.ctx.api.sampler_free(sampler);
                self.ctx.clear_memory();
                return Err(InferenceError::LlamaCpp(e));
            }

            // Sample next token
            next_token = self.ctx.api.sampler_sample(sampler, self.ctx.ctx, -1);
        }

        // 7. Cleanup
        self.ctx.api.sampler_free(sampler);
        self.ctx.clear_memory();

        // 8. Detokenize
        let i32_ids: Vec<i32> = generated_ids.iter().map(|&id| id as i32).collect();
        let text = self.ctx.detokenize(&i32_ids);

        // Check stop_sequences against generated text
        let (final_text, final_reason) =
            check_stop_sequences(&text, &request.stop_sequences, stop_reason);

        Ok(GenerateResponse {
            text: final_text,
            stop_reason: final_reason,
            prompt_tokens: prompt_token_count,
            completion_tokens: generated_ids.len(),
        })
    }

    /// Generates from an OpenAI-shaped chat request: applies the model's chat
    /// template (or uses the raw prompt), builds the full sampler chain, and
    /// runs the decode loop.
    pub(crate) fn generate_chat(
        &mut self,
        request: &ChatRequest,
    ) -> Result<ChatResponse, InferenceError> {
        let _api_guard = llama_api_lock();
        let plan = tool_plan(request);

        // `tool_choice: auto` (best-effort) injects a tool-instruction preamble;
        // Forced/Disabled do not.
        let preamble = match plan.mode {
            // In `Auto` mode `tool_plan` guarantees a non-empty tool set, so the
            // `unwrap_or_default` fallback is unreachable.
            ToolMode::Auto => Some(tool_preamble(request.tools.as_deref().unwrap_or_default())),
            ToolMode::Forced | ToolMode::Disabled => None,
        };
        let prompt = self.render_chat(request, preamble.as_deref())?;

        // Grammar precedence: explicit `grammar` > forced tool-call grammar >
        // `response_format` > none (see `resolve_grammar`).
        let grammar = resolve_grammar(request, &plan);
        let sampler = self.build_sampler_chat(request, grammar.as_deref())?;

        let max_tokens = request.max_tokens.unwrap_or(256) as usize;
        let stop_sequences = request.stop.clone().unwrap_or_default();
        let stop_tokens = request.stop_token_ids.clone().unwrap_or_default();
        let generated =
            self.run_generation(&prompt, sampler, max_tokens, &stop_sequences, &stop_tokens)?;

        // When tools were offered (forced or auto), lift any tool calls out of
        // the generated text and surface them with `finish_reason: tool_calls`.
        if plan.mode != ToolMode::Disabled {
            let (leftover, calls) =
                crate::tool_grammar::parse_tool_calls_from_text(&generated.text);
            if let Some(calls) = calls {
                let mut response = ChatResponse::from_internal("local", generated);
                let choice = &mut response.choices[0];
                choice.message.content = leftover;
                choice.message.tool_calls = Some(calls);
                choice.finish_reason = FinishReason::ToolCalls;
                return Ok(response);
            }
        }
        // `model` is normalized to the caller's spec upstream.
        Ok(ChatResponse::from_internal("local", generated))
    }

    /// Runs the autoregressive decode loop with a pre-built sampler.
    ///
    /// The caller must hold the llama API lock. This frees `sampler` on every
    /// path. (The legacy [`Self::generate`] keeps its own inline loop until the
    /// Phase-D cutover removes it in favor of this method.)
    #[allow(
        clippy::too_many_lines,
        reason = "one linear FFI decode loop; splitting it would obscure the tokenize/prefill/decode/cleanup sequence"
    )]
    fn run_generation(
        &mut self,
        prompt: &str,
        sampler: LlamaSampler,
        max_tokens: usize,
        stop_sequences: &[String],
        stop_tokens: &[u32],
    ) -> Result<GenerateResponse, InferenceError> {
        let mut prompt_tokens = self.ctx.tokenize(prompt, true);
        let prompt_token_count = prompt_tokens.len();

        if prompt_tokens.is_empty() {
            self.ctx.api.sampler_free(sampler);
            return Err(InferenceError::LlamaCpp(
                "prompt tokenized to empty sequence".to_string(),
            ));
        }
        if prompt_tokens.len() >= self.ctx.n_ctx {
            self.ctx.api.sampler_free(sampler);
            return Err(InferenceError::LlamaCpp(format!(
                "prompt length ({}) exceeds context size ({})",
                prompt_tokens.len(),
                self.ctx.n_ctx,
            )));
        }
        if max_tokens == 0 {
            self.ctx.api.sampler_free(sampler);
            return Ok(GenerateResponse {
                text: String::new(),
                stop_reason: StopReason::MaxTokens,
                prompt_tokens: prompt_token_count,
                completion_tokens: 0,
            });
        }

        self.ctx.clear_memory();

        let batch = self.ctx.api.batch_get_one(&mut prompt_tokens);
        if let Err(e) = self.ctx.api.decode(self.ctx.ctx, batch) {
            self.ctx.api.sampler_free(sampler);
            self.ctx.clear_memory();
            return Err(InferenceError::LlamaCpp(e));
        }

        let mut next_token = self.ctx.api.sampler_sample(sampler, self.ctx.ctx, -1);
        let mut generated_ids: Vec<u32> = Vec::new();
        let mut stop_reason = StopReason::MaxTokens;
        let mut pos = prompt_tokens.len();

        for _step in 0..max_tokens {
            if self.ctx.api.vocab_is_eog(self.ctx.vocab, next_token) {
                stop_reason = StopReason::StopToken;
                break;
            }
            if stop_tokens.contains(&(next_token as u32)) {
                stop_reason = StopReason::StopToken;
                break;
            }
            generated_ids.push(next_token as u32);
            pos += 1;
            if pos >= self.ctx.n_ctx {
                stop_reason = StopReason::ContextLength;
                break;
            }
            let mut single = [next_token];
            let batch = self.ctx.api.batch_get_one(&mut single);
            if let Err(e) = self.ctx.api.decode(self.ctx.ctx, batch) {
                self.ctx.api.sampler_free(sampler);
                self.ctx.clear_memory();
                return Err(InferenceError::LlamaCpp(e));
            }
            next_token = self.ctx.api.sampler_sample(sampler, self.ctx.ctx, -1);
        }

        self.ctx.api.sampler_free(sampler);
        self.ctx.clear_memory();

        let i32_ids: Vec<i32> = generated_ids.iter().map(|&id| id as i32).collect();
        let text = self.ctx.detokenize(&i32_ids);
        let (final_text, final_reason) = check_stop_sequences(&text, stop_sequences, stop_reason);

        Ok(GenerateResponse {
            text: final_text,
            stop_reason: final_reason,
            prompt_tokens: prompt_token_count,
            completion_tokens: generated_ids.len(),
        })
    }

    /// Renders a chat request to a prompt via the chat-template cascade: the
    /// model's embedded `tokenizer.chat_template`, falling back to chatml.
    ///
    /// `preamble`, when present, is injected ahead of the conversation (as a
    /// leading system message, or prepended to a raw `prompt`) — used to carry
    /// the tool-instruction preamble for `tool_choice: auto`.
    fn render_chat(
        &self,
        request: &ChatRequest,
        preamble: Option<&str>,
    ) -> Result<String, InferenceError> {
        if let Some(prompt) = &request.prompt {
            return Ok(match preamble {
                Some(pre) => format!("{pre}\n\n{prompt}"),
                None => prompt.clone(),
            });
        }
        let user_messages: Vec<(String, String)> = request
            .messages
            .iter()
            .flatten()
            .map(|m| (role_str(m.role).to_string(), m.content.clone()))
            .collect();
        if user_messages.is_empty() {
            return Err(InferenceError::InvalidSpec(
                "chat request has no messages".to_string(),
            ));
        }
        let mut messages: Vec<(String, String)> = Vec::with_capacity(user_messages.len() + 1);
        if let Some(pre) = preamble {
            messages.push(("system".to_string(), pre.to_string()));
        }
        messages.extend(user_messages);
        // Cascade: explicit chat_format name (a built-in llama.cpp template) ->
        // the model's embedded template -> chatml fallback.
        let explicit = request
            .model_config
            .as_ref()
            .and_then(|c| c.chat_format.clone());
        let embedded = self.ctx.chat_template();
        let apply = |tmpl: &str| self.ctx.api.chat_apply_template(tmpl, &messages, true);
        let rendered = if let Some(name) = &explicit {
            apply(name)
        } else if let Some(tmpl) = &embedded {
            apply(tmpl).or_else(|_| apply("chatml"))
        } else {
            apply("chatml")
        };
        rendered.map_err(InferenceError::LlamaCpp)
    }

    /// Builds the full llama.cpp sampler chain from a chat request's knobs, in
    /// the correct order (grammar, penalties, then the truncation chain or a
    /// terminal mirostat sampler). `tfs_z` has no sampler in this llama.cpp and
    /// is ignored for local.
    fn build_sampler_chat(
        &self,
        request: &ChatRequest,
        grammar: Option<&str>,
    ) -> Result<LlamaSampler, InferenceError> {
        let api = &self.ctx.api;
        let chain = api.sampler_chain_init(api.sampler_chain_default_params());

        // Grammar constraint first — masks tokens that violate the grammar. The
        // effective grammar is resolved by the caller (see `resolve_grammar`).
        if let Some(grammar) = grammar {
            let g = api
                .sampler_init_grammar(self.ctx.vocab, grammar, "root")
                .map_err(InferenceError::LlamaCpp)?;
            api.sampler_chain_add(chain, g);
        }

        let temperature = request.temperature.unwrap_or(0.0);
        if temperature <= 0.0 {
            api.sampler_chain_add(chain, api.sampler_init_greedy());
            return Ok(chain);
        }

        let repeat = request.repeat_penalty.unwrap_or(1.0);
        let freq = request.frequency_penalty.unwrap_or(0.0);
        let present = request.presence_penalty.unwrap_or(0.0);
        if (repeat - 1.0).abs() > f32::EPSILON || freq != 0.0 || present != 0.0 {
            let last_n = request.repeat_last_n.map_or(64, |n| n as i32);
            api.sampler_chain_add(
                chain,
                api.sampler_init_penalties(last_n, repeat, freq, present),
            );
        }

        let seed = request.seed.unwrap_or(0xFFFF_FFFF) as u32;

        // Mirostat is terminal: it replaces the truncation chain.
        if let Some(m) = &request.mirostat {
            api.sampler_chain_add(chain, api.sampler_init_temp(temperature));
            let sampler = if m.mode >= 2 {
                api.sampler_init_mirostat_v2(seed, m.tau, m.eta)
            } else {
                api.sampler_init_mirostat(self.ctx.vocab_size as i32, seed, m.tau, m.eta, 100)
            };
            api.sampler_chain_add(chain, sampler);
            return Ok(chain);
        }

        if let Some(k) = request.top_k {
            if k > 0 {
                api.sampler_chain_add(chain, api.sampler_init_top_k(k as i32));
            }
        }
        if let Some(p) = request.typical_p {
            if p < 1.0 {
                api.sampler_chain_add(chain, api.sampler_init_typical(p, 1));
            }
        }
        if let Some(p) = request.top_p {
            if p < 1.0 {
                api.sampler_chain_add(chain, api.sampler_init_top_p(p, 1));
            }
        }
        if let Some(p) = request.min_p {
            if p > 0.0 {
                api.sampler_chain_add(chain, api.sampler_init_min_p(p, 1));
            }
        }
        api.sampler_chain_add(chain, api.sampler_init_temp(temperature));
        api.sampler_chain_add(chain, api.sampler_init_dist(seed));
        Ok(chain)
    }

    /// Build a llama.cpp sampler chain from request parameters.
    ///
    /// - Grammar constraint first (masks invalid tokens)
    /// - Then sampling strategy: greedy or top_k + top_p + temp + dist
    fn build_sampler(
        &self,
        request: &GenerateRequest,
    ) -> Result<LlamaSampler, crate::InferenceError> {
        let chain_params = self.ctx.api.sampler_chain_default_params();
        let chain = self.ctx.api.sampler_chain_init(chain_params);

        // Grammar constraint first — masks tokens that violate the grammar
        if let Some(grammar) = &request.grammar {
            let grammar_sampler = self
                .ctx
                .api
                .sampler_init_grammar(self.ctx.vocab, grammar, "root")
                .map_err(crate::InferenceError::LlamaCpp)?;
            self.ctx.api.sampler_chain_add(chain, grammar_sampler);
        }

        if request.temperature <= 0.0 {
            // Greedy sampling
            self.ctx
                .api
                .sampler_chain_add(chain, self.ctx.api.sampler_init_greedy());
        } else {
            // Stochastic sampling chain
            if request.top_k > 0 {
                self.ctx.api.sampler_chain_add(
                    chain,
                    self.ctx.api.sampler_init_top_k(request.top_k as i32),
                );
            }
            if request.top_p < 1.0 {
                self.ctx
                    .api
                    .sampler_chain_add(chain, self.ctx.api.sampler_init_top_p(request.top_p, 1));
            }
            self.ctx
                .api
                .sampler_chain_add(chain, self.ctx.api.sampler_init_temp(request.temperature));
            let seed = request.seed.unwrap_or(0xFFFF_FFFF) as u32;
            self.ctx
                .api
                .sampler_chain_add(chain, self.ctx.api.sampler_init_dist(seed));
        }

        Ok(chain)
    }

    /// Encode text to token IDs using the model's tokenizer.
    pub(crate) fn encode(&self, text: &str, add_special: bool) -> Vec<u32> {
        let _api_guard = llama_api_lock();
        self.ctx
            .tokenize(text, add_special)
            .into_iter()
            .map(|id| id as u32)
            .collect()
    }

    /// Decode token IDs back to text using the model's tokenizer.
    pub(crate) fn decode(&self, ids: &[u32]) -> String {
        let _api_guard = llama_api_lock();
        let i32_ids: Vec<i32> = ids.iter().map(|&id| id as i32).collect();
        self.ctx.detokenize(&i32_ids)
    }

    /// The vocabulary size of the loaded model.
    pub(crate) fn vocab_size(&self) -> usize {
        self.ctx.vocab_size
    }

    /// The context size of the loaded model.
    pub(crate) fn context_size(&self) -> usize {
        self.ctx.n_ctx
    }
}

/// Check generated text against stop sequences.
///
/// If any stop sequence is found, the text is truncated at the earliest
/// occurrence across all sequences and the stop reason is changed to
/// `StopToken`.
fn check_stop_sequences(
    text: &str,
    stop_sequences: &[String],
    original_reason: StopReason,
) -> (String, StopReason) {
    let mut earliest_pos: Option<usize> = None;

    for seq in stop_sequences {
        if seq.is_empty() {
            continue;
        }
        if let Some(pos) = text.find(seq.as_str()) {
            match earliest_pos {
                None => earliest_pos = Some(pos),
                Some(prev) if pos < prev => earliest_pos = Some(pos),
                _ => {}
            }
        }
    }

    match earliest_pos {
        Some(pos) => (text[..pos].to_string(), StopReason::StopToken),
        None => (text.to_string(), original_reason),
    }
}

/// The wire role name llama.cpp chat templates expect.
fn role_str(role: Role) -> &'static str {
    match role {
        Role::System => "system",
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::Tool => "tool",
    }
}

/// The GBNF grammar to constrain local generation, if any.
///
/// An explicit raw `grammar` (the low-level GBNF escape hatch) takes precedence.
/// Otherwise `response_format` maps to a grammar: `json_object` uses the
/// canonical JSON-object grammar, and `json_schema` is converted via
/// [`crate::grammar::json_schema_to_gbnf`]. Free-form text yields `None`.
fn effective_grammar(request: &ChatRequest) -> Option<String> {
    if let Some(grammar) = &request.grammar {
        return Some(grammar.clone());
    }
    match &request.response_format {
        Some(ResponseFormat::JsonObject) => Some(crate::grammar::JSON_OBJECT_GRAMMAR.to_string()),
        Some(ResponseFormat::JsonSchema { json_schema }) => {
            Some(crate::grammar::json_schema_to_gbnf(&json_schema.schema))
        }
        Some(ResponseFormat::Text) | None => None,
    }
}

/// How the local provider handles tool calling for a chat request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToolMode {
    /// No tools offered, or `tool_choice: none`: plain generation.
    Disabled,
    /// `tool_choice: auto` (the default when tools are present): inject a
    /// tool-instruction preamble and best-effort parse `<tool_call>` blocks; no
    /// grammar constraint.
    Auto,
    /// `tool_choice: required` or a named function: force a tool-call grammar.
    Forced,
}

/// The resolved tool plan: the mode plus, for a named choice, the forced name.
struct ToolPlan {
    mode: ToolMode,
    forced: Option<String>,
}

/// Classify a chat request's tool intent.
///
/// Tools present with no explicit `tool_choice` default to `auto`. `none`
/// disables tools even when they are offered. A named function forces that call.
fn tool_plan(request: &ChatRequest) -> ToolPlan {
    let has_tools = request.tools.as_deref().is_some_and(|t| !t.is_empty());
    if !has_tools {
        return ToolPlan {
            mode: ToolMode::Disabled,
            forced: None,
        };
    }
    match &request.tool_choice {
        None | Some(ToolChoice::Mode(ToolChoiceMode::Auto)) => ToolPlan {
            mode: ToolMode::Auto,
            forced: None,
        },
        Some(ToolChoice::Mode(ToolChoiceMode::None)) => ToolPlan {
            mode: ToolMode::Disabled,
            forced: None,
        },
        Some(ToolChoice::Mode(ToolChoiceMode::Required)) => ToolPlan {
            mode: ToolMode::Forced,
            forced: None,
        },
        Some(ToolChoice::Named(NamedToolChoice::Function { function })) => ToolPlan {
            mode: ToolMode::Forced,
            forced: Some(function.name.clone()),
        },
    }
}

/// The GBNF grammar to constrain generation, honoring tool intent.
///
/// Precedence: an explicit raw `grammar` wins; then a forced tool-call grammar
/// (`tool_choice: required`/named) overrides `response_format`; then
/// `response_format`; else none. `tool_choice: auto` does not constrain
/// generation (tools are parsed best-effort from the output).
fn resolve_grammar(request: &ChatRequest, plan: &ToolPlan) -> Option<String> {
    if request.grammar.is_none() && plan.mode == ToolMode::Forced {
        // `Forced` implies a non-empty tool set (see `tool_plan`), so the
        // `unwrap_or_default` fallback is unreachable.
        let tools = request.tools.as_deref().unwrap_or_default();
        return Some(crate::tool_grammar::tool_call_grammar(
            tools,
            plan.forced.as_deref(),
        ));
    }
    effective_grammar(request)
}

/// A system preamble describing the offered tools and the `<tool_call>` calling
/// convention, injected for `tool_choice: auto` (best-effort).
///
/// This is model-dependent by design: small local models may not comply.
fn tool_preamble(tools: &[Tool]) -> String {
    let mut out = String::from(
        "You can call tools. To call a tool, emit a single block of the form \
         <tool_call>{\"name\": \"<tool_name>\", \"arguments\": { ... }}</tool_call> \
         and nothing else. If no tool is needed, answer the user normally.\n\n\
         Available tools:\n",
    );
    for tool in tools {
        let function = crate::tool_grammar::function_of(tool);
        // Writing to a `String` is infallible.
        let _ = write!(out, "- {}", function.name);
        if let Some(description) = &function.description {
            let _ = write!(out, ": {description}");
        }
        if let Some(parameters) = &function.parameters {
            if let Ok(schema) = serde_json::to_string(parameters) {
                let _ = write!(out, " (arguments JSON schema: {schema})");
            }
        }
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::{FunctionDef, ToolChoiceFunction};

    // -----------------------------------------------------------------------
    // effective_grammar unit tests (no libllama needed)
    // -----------------------------------------------------------------------

    #[test]
    fn effective_grammar_none_for_text() {
        let mut req = ChatRequest {
            prompt: Some("hi".into()),
            ..Default::default()
        };
        assert!(effective_grammar(&req).is_none());
        req.response_format = Some(ResponseFormat::Text);
        assert!(effective_grammar(&req).is_none());
    }

    #[test]
    fn effective_grammar_explicit_grammar_wins() {
        let req = ChatRequest {
            prompt: Some("hi".into()),
            grammar: Some("root ::= \"x\"".into()),
            response_format: Some(ResponseFormat::JsonObject),
            ..Default::default()
        };
        assert_eq!(effective_grammar(&req).as_deref(), Some("root ::= \"x\""));
    }

    #[test]
    fn effective_grammar_json_object_and_schema() {
        let obj = ChatRequest {
            prompt: Some("hi".into()),
            response_format: Some(ResponseFormat::JsonObject),
            ..Default::default()
        };
        assert!(effective_grammar(&obj)
            .unwrap()
            .contains("root   ::= object"));

        let schema = ChatRequest {
            prompt: Some("hi".into()),
            response_format: Some(ResponseFormat::JsonSchema {
                json_schema: crate::wire::JsonSchemaSpec {
                    name: "p".into(),
                    description: None,
                    schema: serde_json::json!({
                        "type": "object",
                        "properties": {"n": {"type": "string"}},
                        "required": ["n"]
                    }),
                    strict: None,
                },
            }),
            ..Default::default()
        };
        let g = effective_grammar(&schema).unwrap();
        assert!(g.starts_with("root ::= obj-"));
        assert!(g.contains("\\\"n\\\""));
    }

    // -----------------------------------------------------------------------
    // tool_plan / resolve_grammar unit tests (no libllama needed)
    // -----------------------------------------------------------------------

    fn weather_tool() -> Tool {
        Tool::Function {
            function: FunctionDef {
                name: "get_weather".into(),
                description: Some("look up weather".into()),
                parameters: Some(serde_json::json!({
                    "type": "object",
                    "properties": {"city": {"type": "string"}},
                    "required": ["city"]
                })),
                strict: None,
            },
        }
    }

    fn chat_with_tools(choice: Option<ToolChoice>) -> ChatRequest {
        ChatRequest {
            prompt: Some("hi".into()),
            tools: Some(vec![weather_tool()]),
            tool_choice: choice,
            ..Default::default()
        }
    }

    #[test]
    fn tool_plan_no_tools_is_disabled() {
        let req = ChatRequest {
            prompt: Some("hi".into()),
            ..Default::default()
        };
        assert_eq!(tool_plan(&req).mode, ToolMode::Disabled);
    }

    #[test]
    fn tool_plan_defaults_to_auto_when_tools_present() {
        let plan = tool_plan(&chat_with_tools(None));
        assert_eq!(plan.mode, ToolMode::Auto);
        assert!(plan.forced.is_none());
    }

    #[test]
    fn tool_plan_none_disables_even_with_tools() {
        let plan = tool_plan(&chat_with_tools(Some(ToolChoice::Mode(
            ToolChoiceMode::None,
        ))));
        assert_eq!(plan.mode, ToolMode::Disabled);
    }

    #[test]
    fn tool_plan_required_is_forced() {
        let plan = tool_plan(&chat_with_tools(Some(ToolChoice::Mode(
            ToolChoiceMode::Required,
        ))));
        assert_eq!(plan.mode, ToolMode::Forced);
        assert!(plan.forced.is_none());
    }

    #[test]
    fn tool_plan_named_is_forced_with_name() {
        let plan = tool_plan(&chat_with_tools(Some(ToolChoice::Named(
            NamedToolChoice::Function {
                function: ToolChoiceFunction {
                    name: "get_weather".into(),
                },
            },
        ))));
        assert_eq!(plan.mode, ToolMode::Forced);
        assert_eq!(plan.forced.as_deref(), Some("get_weather"));
    }

    #[test]
    fn resolve_grammar_none_without_tools_or_format() {
        let req = ChatRequest {
            prompt: Some("hi".into()),
            ..Default::default()
        };
        let plan = tool_plan(&req);
        assert!(resolve_grammar(&req, &plan).is_none());
    }

    #[test]
    fn resolve_grammar_forced_produces_tool_grammar() {
        let req = chat_with_tools(Some(ToolChoice::Mode(ToolChoiceMode::Required)));
        let plan = tool_plan(&req);
        let g = resolve_grammar(&req, &plan).expect("a tool grammar");
        assert!(g.contains("\\\"get_weather\\\""), "tool name literal: {g}");
        assert!(g.contains("\\\"name\\\""), "tool-call shape: {g}");
    }

    #[test]
    fn resolve_grammar_explicit_grammar_wins_over_tools() {
        let mut req = chat_with_tools(Some(ToolChoice::Mode(ToolChoiceMode::Required)));
        req.grammar = Some("root ::= \"x\"".into());
        let plan = tool_plan(&req);
        assert_eq!(
            resolve_grammar(&req, &plan).as_deref(),
            Some("root ::= \"x\"")
        );
    }

    #[test]
    fn resolve_grammar_auto_uses_response_format_not_tools() {
        let mut req = chat_with_tools(Some(ToolChoice::Mode(ToolChoiceMode::Auto)));
        req.response_format = Some(ResponseFormat::JsonObject);
        let plan = tool_plan(&req);
        let g = resolve_grammar(&req, &plan).expect("a grammar");
        // Auto does not force a tool-call grammar; `response_format` applies.
        assert!(g.contains("root   ::= object"), "json-object grammar: {g}");
        assert!(!g.contains("\\\"name\\\""), "not a tool-call grammar: {g}");
    }

    // -----------------------------------------------------------------------
    // check_stop_sequences unit tests (no libllama needed)
    // -----------------------------------------------------------------------

    #[test]
    fn stop_sequences_no_match_returns_original() {
        let (text, reason) =
            check_stop_sequences("Hello world", &["STOP".into()], StopReason::MaxTokens);
        assert_eq!(text, "Hello world");
        assert_eq!(reason, StopReason::MaxTokens);
    }

    #[test]
    fn stop_sequences_match_truncates_text() {
        let (text, reason) =
            check_stop_sequences("Hello STOP world", &["STOP".into()], StopReason::MaxTokens);
        assert_eq!(text, "Hello ");
        assert_eq!(reason, StopReason::StopToken);
    }

    #[test]
    fn stop_sequences_match_at_start_returns_empty() {
        let (text, reason) =
            check_stop_sequences("STOP world", &["STOP".into()], StopReason::MaxTokens);
        assert_eq!(text, "");
        assert_eq!(reason, StopReason::StopToken);
    }

    #[test]
    fn stop_sequences_match_at_end() {
        let (text, reason) =
            check_stop_sequences("Hello world\n\n", &["\n\n".into()], StopReason::MaxTokens);
        assert_eq!(text, "Hello world");
        assert_eq!(reason, StopReason::StopToken);
    }

    #[test]
    fn stop_sequences_earliest_position_wins() {
        let (text, reason) = check_stop_sequences(
            "Hello STOP1 and STOP2 end",
            &["STOP2".into(), "STOP1".into()],
            StopReason::MaxTokens,
        );
        // STOP1 appears at position 6, STOP2 at position 16.
        // Even though STOP2 is listed first in the array, STOP1 has the
        // earliest position in the text, so it wins.
        assert_eq!(text, "Hello ");
        assert_eq!(reason, StopReason::StopToken);
    }

    #[test]
    fn stop_sequences_empty_list_returns_original() {
        let (text, reason) = check_stop_sequences("Hello world", &[], StopReason::ContextLength);
        assert_eq!(text, "Hello world");
        assert_eq!(reason, StopReason::ContextLength);
    }

    #[test]
    fn stop_sequences_empty_string_in_list_skipped() {
        let (text, reason) = check_stop_sequences(
            "Hello world",
            &[String::new(), "STOP".into()],
            StopReason::MaxTokens,
        );
        assert_eq!(text, "Hello world");
        assert_eq!(reason, StopReason::MaxTokens);
    }

    #[test]
    fn stop_sequences_on_empty_text() {
        let (text, reason) = check_stop_sequences("", &["STOP".into()], StopReason::MaxTokens);
        assert_eq!(text, "");
        assert_eq!(reason, StopReason::MaxTokens);
    }

    #[test]
    fn stop_sequences_preserves_cancelled_reason_when_no_match() {
        let (text, reason) =
            check_stop_sequences("Hello world", &["STOP".into()], StopReason::Cancelled);
        assert_eq!(text, "Hello world");
        assert_eq!(reason, StopReason::Cancelled);
    }

    #[test]
    fn stop_sequences_overrides_reason_on_match() {
        // Even if original reason was ContextLength, finding a stop sequence
        // changes it to StopToken
        let (text, reason) =
            check_stop_sequences("Hello STOP", &["STOP".into()], StopReason::ContextLength);
        assert_eq!(text, "Hello ");
        assert_eq!(reason, StopReason::StopToken);
    }

    #[test]
    fn stop_sequences_newline_sequence() {
        let (text, reason) = check_stop_sequences(
            "Line 1\nLine 2\n\nLine 3",
            &["\n\n".into()],
            StopReason::MaxTokens,
        );
        assert_eq!(text, "Line 1\nLine 2");
        assert_eq!(reason, StopReason::StopToken);
    }

    #[test]
    fn stop_sequences_unicode() {
        let (text, reason) =
            check_stop_sequences("Hello 世界 end", &["世界".into()], StopReason::MaxTokens);
        assert_eq!(text, "Hello ");
        assert_eq!(reason, StopReason::StopToken);
    }

    #[test]
    fn stop_sequences_overlapping_prefix() {
        // "STOP" is a prefix of "STOP1"; both match but "STOP" is at the same
        // position (6) and shorter — earliest_pos wins.
        let (text, reason) = check_stop_sequences(
            "Hello STOP1 end",
            &["STOP1".into(), "STOP".into()],
            StopReason::MaxTokens,
        );
        // Both match at position 6; the first one found sets earliest_pos,
        // the second doesn't improve it. Either way, truncation at position 6.
        assert_eq!(text, "Hello ");
        assert_eq!(reason, StopReason::StopToken);
    }

    #[test]
    fn stop_sequences_text_equals_sequence() {
        let (text, reason) = check_stop_sequences("STOP", &["STOP".into()], StopReason::MaxTokens);
        assert_eq!(text, "");
        assert_eq!(reason, StopReason::StopToken);
    }

    #[test]
    fn stop_sequences_repeated_occurrences() {
        // Same sequence appears twice; should truncate at the first occurrence.
        let (text, reason) = check_stop_sequences("aXbXc", &["X".into()], StopReason::MaxTokens);
        assert_eq!(text, "a");
        assert_eq!(reason, StopReason::StopToken);
    }

    #[test]
    fn stop_sequences_iteration_order_does_not_matter() {
        // Verify that listing order doesn't change the result
        let text_input = "AAA BBB CCC";
        let (text_a, _) = check_stop_sequences(
            text_input,
            &["CCC".into(), "BBB".into(), "AAA".into()],
            StopReason::MaxTokens,
        );
        let (text_b, _) = check_stop_sequences(
            text_input,
            &["AAA".into(), "BBB".into(), "CCC".into()],
            StopReason::MaxTokens,
        );
        assert_eq!(text_a, text_b, "order of stop_sequences should not matter");
        assert_eq!(text_a, "");
    }

    #[test]
    fn stop_sequences_all_empty_strings_returns_original() {
        let (text, reason) = check_stop_sequences(
            "Hello world",
            &[String::new(), String::new()],
            StopReason::MaxTokens,
        );
        assert_eq!(text, "Hello world");
        assert_eq!(reason, StopReason::MaxTokens);
    }

    #[test]
    fn stop_sequences_multiline_text() {
        let (text, reason) = check_stop_sequences(
            "line1\nline2\nEND\nline3",
            &["END".into()],
            StopReason::MaxTokens,
        );
        assert_eq!(text, "line1\nline2\n");
        assert_eq!(reason, StopReason::StopToken);
    }
}
