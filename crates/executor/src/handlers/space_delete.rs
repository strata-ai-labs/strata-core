use std::sync::Arc;

use strata_engine::{is_user_space_delete_constraint, StrataError};

use crate::bridge::Primitives;
use crate::handlers::embed_runtime;
use crate::{BranchId, Error, Output, Result};

pub(crate) fn delete(
    primitives: &Arc<Primitives>,
    branch: BranchId,
    space: String,
    force: bool,
) -> Result<Output> {
    match primitives.space.delete_user_space_with_post_data_cleanup(
        branch.as_str(),
        &space,
        force,
        |branch_id, space| {
            embed_runtime::delete_shadow_embeddings_for_space(primitives, branch_id, space);
            Ok(())
        },
    ) {
        Ok(()) => Ok(Output::Unit),
        Err(error) if is_user_space_delete_constraint(&error) => {
            let StrataError::InvalidOperation { reason, .. } = error else {
                unreachable!("space delete constraint helper only matches invalid operations");
            };
            Err(Error::ConstraintViolation { reason })
        }
        Err(error) => Err(Error::from(error)),
    }
}
