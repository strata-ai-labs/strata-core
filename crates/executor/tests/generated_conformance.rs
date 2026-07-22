//! TCP4.1 generated conformance suite.
//!
//! `conformance_cases.rs` is emitted by `strata-idl generate-tests` from the
//! resolved IDL (one test per command per applicable family); `check-tests`
//! fails CI when it drifts from the IDL. `support.rs` is hand-written and
//! holds the assertion logic. See `crates/executor/idl/v1/README.md`.
//!
//! Gated on `inference` + `testkit`: inference commands' wire types need the
//! feature to exist, and their replays need the deterministic fake service.
//! The CI "Executor IDL drift gates" lane runs this target with both.
#![cfg(all(feature = "inference", feature = "testkit"))]

#[path = "generated/support.rs"]
mod support;

// Generated output is byte-pinned by `check-tests`; rustfmt must not reflow
// it or the freshness guard and the formatter would fight forever.
#[rustfmt::skip]
#[path = "generated/conformance_cases.rs"]
mod conformance_cases;
