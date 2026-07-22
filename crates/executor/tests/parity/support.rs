//! Shared support for the TCP4.7 cross-surface parity harness.
//!
//! Each `parity_*.rs` target replays wire-shaped commands against a scratch
//! cache executor and asserts that parallel surfaces obey one contract —
//! except where the shrink-only divergence ledger
//! (`idl/v1/cross-surface-divergences.yaml`) records a filed bug, in which
//! case a PIN test asserts today's divergent behavior exactly. A landed fix
//! breaks its pin, forcing the ledger entry's deletion and the flip to the
//! real parity assertion.

// Helpers are shared by every parity_*.rs target; not every target uses
// every helper, so per-target dead-code analysis is expected to find slack.
#![allow(dead_code)]

use std::path::PathBuf;

use serde_json::Value;
use strata_executor::{Command, Executor};

const LEDGER_REL: &str = "idl/v1/cross-surface-divergences.yaml";

pub(crate) fn executor() -> Executor {
    Executor::open_cache().expect("open scratch cache executor")
}

fn command(wire: &Value) -> Command {
    serde_json::from_value(wire.clone())
        .unwrap_or_else(|err| panic!("wire JSON must parse as Command ({wire}): {err}"))
}

/// Executes a wire-shaped command that must succeed; returns the serialized
/// output (`{"type": ..., "data": ...}`).
pub(crate) fn run(executor: &mut Executor, wire: &Value) -> Value {
    let output = executor
        .execute(command(wire))
        .unwrap_or_else(|err| panic!("command must succeed ({wire}): {err}"));
    serde_json::to_value(&output).expect("serialize output")
}

/// Executes a wire-shaped command that must fail; returns the stable error
/// code (rule 29: assert codes, never prose).
pub(crate) fn run_err_code(executor: &mut Executor, wire: &Value) -> String {
    match executor.execute(command(wire)) {
        Ok(output) => panic!(
            "command must fail ({wire}), got: {}",
            serde_json::to_value(&output).expect("serialize output")
        ),
        Err(error) => serde_json::to_value(error.status()).expect("serialize status")["code"]
            .as_str()
            .expect("error status carries a code")
            .to_owned(),
    }
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Divergence {
    pub(crate) axis: String,
    #[allow(dead_code)] // documentation fields, read by humans and the guard
    pub(crate) surfaces: Vec<String>,
    #[allow(dead_code)]
    pub(crate) behavior: String,
    pub(crate) issue: u64,
}

pub(crate) fn ledger() -> Vec<Divergence> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(LEDGER_REL);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("read divergence ledger {}: {err}", path.display()));
    serde_yaml::from_str(&text)
        .unwrap_or_else(|err| panic!("parse divergence ledger {}: {err}", path.display()))
}

/// Declares that the calling test pins the ledgered divergence `(axis,
/// issue)` — the pin-implies-entry direction of the ledger guard.
pub(crate) fn pinned(axis: &str, issue: u64) {
    assert!(
        ledger()
            .iter()
            .any(|entry| entry.axis == axis && entry.issue == issue),
        "pin test for axis `{axis}` (#{issue}) has no ledger entry — add it to {LEDGER_REL}"
    );
}

/// The entry-implies-pin direction: every ledger entry whose axis starts with
/// `owned_prefix` must appear in this file's static pin inventory, so a
/// leftover entry after a fix (or a typo'd axis) fails loudly.
pub(crate) fn assert_ledger_entries_all_pinned(owned_prefix: &str, pins: &[(&str, u64)]) {
    for entry in ledger() {
        if entry.axis.starts_with(owned_prefix) {
            assert!(
                pins.iter()
                    .any(|(axis, issue)| *axis == entry.axis && *issue == entry.issue),
                "ledger entry axis `{}` (#{}) has no pin test in this target — \
                 either add the pin or delete the stale entry",
                entry.axis,
                entry.issue
            );
        }
    }
}
