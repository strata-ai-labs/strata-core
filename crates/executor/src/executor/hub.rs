//! `hub.*` command handlers: thin dispatch over the `strata-hub`
//! orchestration layer (the executor stays transport-only; resolution,
//! download, verification, reconstitution, and origin recording live in
//! `strata_hub`).

use std::path::PathBuf;

use strata_hub::stratahub_protocol::{BranchName, DatasetName};
use strata_hub::{
    clone_dataset, resolve_hub_url, ClientTransport, CloneError, CloneRequest, HubUrlError,
    HubUrlInputs,
};

use super::{Executor, ExecutorResult, Output};
use crate::ExecutorError;

impl Executor {
    // Clone never touches the session database, but the verb dispatches
    // through the executor like every hub.* command so all frontends share
    // one path (coordination §3.6).
    #[allow(clippy::unused_self)]
    pub(super) fn execute_hub_clone(
        &mut self,
        dataset: &str,
        branch: Option<&str>,
        dest: &str,
        hub_url: Option<String>,
    ) -> ExecutorResult<Output> {
        let resolved = resolve_hub_url(&HubUrlInputs::from_process(hub_url))
            .map_err(|error| hub_url_error(&error))?;
        let transport = ClientTransport::new(resolved.url).map_err(|error| clone_error(&error))?;

        let dataset = DatasetName::parse(dataset).map_err(|error| {
            ExecutorError::invalid_input(
                "invalid_argument.executor.hub_dataset",
                format!("dataset name is invalid: {error}"),
            )
        })?;
        let branch = branch.map(BranchName::parse).transpose().map_err(|error| {
            ExecutorError::invalid_input(
                "invalid_argument.executor.hub_branch",
                format!("branch name is invalid: {error}"),
            )
        })?;

        let outcome = clone_dataset(
            &transport,
            &CloneRequest {
                dataset: dataset.clone(),
                branch,
                dest: PathBuf::from(dest),
            },
            &mut |_progress| {},
        )
        .map_err(|error| clone_error(&error))?;

        Ok(Output::HubCloneResult {
            dataset: dataset.as_str().to_owned(),
            branch: outcome.branch.as_str().to_owned(),
            dest: dest.to_owned(),
            manifest_hash: outcome.manifest_hash.as_str().to_owned(),
            object_count: outcome.object_count,
            total_bytes: outcome.total_bytes,
        })
    }
}

fn hub_url_error(error: &HubUrlError) -> ExecutorError {
    ExecutorError::new(
        crate::ExecutorErrorClass::Unavailable,
        "failed_precondition.executor.hub_url",
        false,
        error.to_string(),
    )
}

fn clone_error(error: &CloneError) -> ExecutorError {
    match error {
        CloneError::Transport { .. } => ExecutorError::new(
            crate::ExecutorErrorClass::Unavailable,
            "unavailable.executor.hub_transport",
            true,
            error.to_string(),
        ),
        _ => ExecutorError::new(
            crate::ExecutorErrorClass::Unavailable,
            "failed_precondition.executor.hub_clone",
            false,
            error.to_string(),
        ),
    }
}
