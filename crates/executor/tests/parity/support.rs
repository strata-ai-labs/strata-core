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

/// Standard base64 for `Bytes` wire fields.
pub(crate) fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(TABLE[(n >> 18) as usize & 0x3f] as char);
        out.push(TABLE[(n >> 12) as usize & 0x3f] as char);
        out.push(if chunk.len() > 1 {
            TABLE[(n >> 6) as usize & 0x3f] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[n as usize & 0x3f] as char
        } else {
            '='
        });
    }
    out
}

/// Inverse of [`base64_encode`]; panics on malformed input (test-only).
// Byte extraction from the 24-bit accumulator truncates by design, and the
// padding count is over a <=4-byte chunk.
#[allow(clippy::cast_possible_truncation, clippy::naive_bytecount)]
pub(crate) fn base64_decode(text: &str) -> Vec<u8> {
    fn value(c: u8) -> u32 {
        match c {
            b'A'..=b'Z' => u32::from(c - b'A'),
            b'a'..=b'z' => u32::from(c - b'a') + 26,
            b'0'..=b'9' => u32::from(c - b'0') + 52,
            b'+' => 62,
            b'/' => 63,
            _ => panic!("malformed base64 byte {c}"),
        }
    }
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    for chunk in bytes.chunks(4) {
        let pad = chunk.iter().filter(|&&c| c == b'=').count();
        let n = chunk
            .iter()
            .take(4 - pad)
            .fold(0_u32, |acc, &c| (acc << 6) | value(c))
            << (6 * pad);
        out.push((n >> 16) as u8);
        if pad < 2 {
            out.push((n >> 8) as u8);
        }
        if pad < 1 {
            out.push(n as u8);
        }
    }
    out
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
