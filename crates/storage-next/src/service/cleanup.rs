//! Shared L4 cleanup classification helpers.

use crate::backend::{
    BackendError, BackendErrorKind, DeleteDurability, DeleteError, DeleteOutcome, DeleteStatus,
};

pub(crate) fn durable_cleanup_succeeded(outcome: &DeleteOutcome) -> bool {
    match outcome.status() {
        DeleteStatus::Deleted => outcome.durability() == DeleteDurability::Durable,
        DeleteStatus::AlreadyMissing => true,
    }
}

pub(crate) fn durable_cleanup_failure(outcome: &DeleteOutcome) -> DeleteError {
    DeleteError::removed_durability_unconfirmed(
        outcome.object(),
        BackendError::new(
            BackendErrorKind::UnsupportedOperation,
            "durable cleanup requires a durable delete outcome",
        ),
    )
}
