//! Canonical first-run guidance strings.
//!
//! One source for the commands every surface teaches: `strata init`'s
//! `next_steps`, the agents guide, and (mirrored downstream) install.sh and
//! packaging epilogues. Change them here and every surface stays in step.

/// The canonical three commands taught after install.
pub(crate) const NEXT_STEPS: [&str; 3] = [
    "strata ./my-db kv put greeting hello",
    "strata ./my-db kv get greeting",
    "strata --cache",
];
