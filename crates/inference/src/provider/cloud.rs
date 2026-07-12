//! Shared helpers for cloud chat providers.
use crate::wire::{ChatRequest, Role};
use crate::InferenceError;

/// Chat turns as (role, content). A raw `prompt` becomes a single user turn.
pub(crate) fn chat_turns(request: &ChatRequest) -> Vec<(Role, String)> {
    if let Some(prompt) = &request.prompt {
        return vec![(Role::User, prompt.clone())];
    }
    request
        .messages
        .iter()
        .flatten()
        .map(|m| (m.role, m.content.clone()))
        .collect()
}

/// Rejects knobs no cloud provider can honor. GBNF `grammar` is a llama.cpp
/// constraint; cloud callers should use `response_format` instead.
pub(crate) fn reject_local_only(
    request: &ChatRequest,
    provider: &str,
) -> Result<(), InferenceError> {
    if request.grammar.is_some() {
        return Err(InferenceError::Provider(format!(
            "{provider}: GBNF `grammar` is local-only; use `response_format` for cloud models"
        )));
    }
    Ok(())
}
