//! API runtime shell.

use super::StorageRuntimeState;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StorageRuntime {
    state: StorageRuntimeState,
}

impl StorageRuntime {
    #[must_use]
    pub const fn closed() -> Self {
        Self {
            state: StorageRuntimeState::Closed,
        }
    }

    #[must_use]
    pub const fn state(self) -> StorageRuntimeState {
        self.state
    }
}
