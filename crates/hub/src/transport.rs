//! The real transport: `stratahub-client` bound to [`HubTransport`]
//! (slice `HB7b`).
//!
//! The client is async; clone orchestration is synchronous. Each call
//! bridges through a dedicated multi-thread runtime owned by the
//! transport, and object downloads go through the client's
//! range-resumable `download_object` (retries + `Range:` resume per
//! its retry policy — the §3.6 resumability invariant).

use stratahub_client::{download_object, Client, ClientConfig, RetryPolicy};
use stratahub_protocol::{BranchName, DatasetName, Hash, Manifest};
use url::Url;

use crate::clone::{CloneError, HubTransport};

/// [`HubTransport`] over a real hub via `stratahub-client`.
pub struct ClientTransport {
    client: Client,
    runtime: tokio::runtime::Runtime,
    retry: RetryPolicy,
    base_url: Url,
}

impl ClientTransport {
    /// Builds a transport for the hub at `base_url` (as produced by
    /// [`crate::resolve_hub_url`]).
    ///
    /// # Errors
    ///
    /// [`CloneError::Transport`] when the URL or HTTP stack refuses
    /// configuration.
    pub fn new(base_url: Url) -> Result<Self, CloneError> {
        let mut config = ClientConfig::default();
        config.request_timeout = request_timeout_from_env(config.request_timeout);
        let client =
            Client::new(base_url.clone(), config).map_err(|error| transport_error(&error))?;
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|error| CloneError::Transport {
                detail: format!("async runtime failed to start: {error}"),
            })?;
        Ok(Self {
            client,
            runtime,
            retry: RetryPolicy::default(),
            base_url,
        })
    }
}

/// Test-facing override for the per-request wall-clock timeout
/// (`STRATA_HUB_TIMEOUT_SECONDS`): sanitizer CI legs run 5-15x slower than
/// native, so the 30s default expires on legitimate mock-hub round trips
/// (#2803). Unset or unparsable values keep the default; production callers
/// never set this.
fn request_timeout_from_env(default: std::time::Duration) -> std::time::Duration {
    std::env::var("STRATA_HUB_TIMEOUT_SECONDS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .map_or(default, std::time::Duration::from_secs)
}

impl HubTransport for ClientTransport {
    fn hub_url(&self) -> String {
        self.base_url.as_str().trim_end_matches('/').to_owned()
    }

    fn default_branch(&self, dataset: &DatasetName) -> Result<BranchName, CloneError> {
        let card = self
            .runtime
            .block_on(self.client.get_dataset(dataset))
            .map_err(|error| transport_error(&error))?;
        Ok(card.summary.default_branch)
    }

    fn resolve_ref(&self, dataset: &DatasetName, branch: &BranchName) -> Result<Hash, CloneError> {
        let reference = self
            .runtime
            .block_on(self.client.resolve_ref(dataset, branch))
            .map_err(|error| transport_error(&error))?;
        Ok(reference.manifest_hash)
    }

    fn get_manifest(&self, hash: &Hash) -> Result<Manifest, CloneError> {
        // The client verifies the bytes hash to `hash` before parsing.
        let (_bytes, manifest) = self
            .runtime
            .block_on(self.client.get_manifest(hash))
            .map_err(|error| transport_error(&error))?;
        Ok(manifest)
    }

    fn get_object(&self, hash: &Hash) -> Result<Vec<u8>, CloneError> {
        let mut body = Vec::new();
        self.runtime
            .block_on(download_object(&self.client, &self.retry, hash, &mut body))
            .map_err(|error| transport_error(&error))?;
        Ok(body)
    }
}

fn transport_error(error: &stratahub_client::ClientError) -> CloneError {
    CloneError::Transport {
        detail: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::request_timeout_from_env;
    use std::time::Duration;

    /// One test for all three cases: the override env var is process-global,
    /// so sequential assertions avoid a set/remove race between tests.
    #[test]
    fn request_timeout_env_override_truth_table() {
        let default = Duration::from_secs(30);
        std::env::remove_var("STRATA_HUB_TIMEOUT_SECONDS");
        assert_eq!(request_timeout_from_env(default), default);

        std::env::set_var("STRATA_HUB_TIMEOUT_SECONDS", "180");
        assert_eq!(request_timeout_from_env(default), Duration::from_secs(180));

        std::env::set_var("STRATA_HUB_TIMEOUT_SECONDS", "not-a-number");
        assert_eq!(request_timeout_from_env(default), default);
        std::env::remove_var("STRATA_HUB_TIMEOUT_SECONDS");
    }
}
