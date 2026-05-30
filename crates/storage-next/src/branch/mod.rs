//! Branch-aware visibility and inheritance mechanics.

#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "branch facade reexports are removed before operation-family cleanup removes remaining branch-local scaffolding"
    )
)]

pub(crate) mod config;
pub(crate) mod error;
pub(crate) mod facts;
pub(crate) mod identity;
pub(crate) mod pruning;
pub(crate) mod read;
pub(crate) mod state;

#[cfg(test)]
mod tests;
