//! Public inference API vocabulary.

pub use crate::registry::{ModelInfo, ModelTask};
pub use crate::runtime::{
    EmbedRequest, EmbedResponse, EmbedRuntimeOutcome, InferenceCapability, InferenceRuntime,
    InferenceRuntimeConfig, ModelCacheStatus, PullModelOutput, RankRequest, RankResponse,
    RankRuntimeOutcome,
};
pub use crate::{
    GenerateRequest, GenerateResponse, InferenceError, InferenceErrorClass, ProviderKind,
    StopReason,
};
