//! Shared helpers for cloud chat providers.
use crate::wire::ChatRequest;
use crate::InferenceError;

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
