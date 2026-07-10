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
        let client = Client::new(base_url.clone(), ClientConfig::default())
            .map_err(|error| transport_error(&error))?;
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
