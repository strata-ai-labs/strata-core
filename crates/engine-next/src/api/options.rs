//! Explicit database open options.

/// Options for explicit cache database open.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CacheOpenOptions {
    _private: (),
}

#[allow(clippy::new_without_default)]
impl CacheOpenOptions {
    /// Creates cache open options.
    #[must_use]
    pub const fn new() -> Self {
        Self { _private: () }
    }
}

/// Options for explicit durable-local database open.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DurableLocalOpenOptions {
    _private: (),
}

#[allow(clippy::new_without_default)]
impl DurableLocalOpenOptions {
    /// Creates durable-local open options.
    #[must_use]
    pub const fn new() -> Self {
        Self { _private: () }
    }
}
