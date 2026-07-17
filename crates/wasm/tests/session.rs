//! Browser-boundary session tests, executed on wasm32-unknown-unknown: each
//! case drives the full executor → engine → storage cache stack through the
//! serialized command surface — the conformance plan's "cache substrate runs
//! on wasm" bar, proven by execution rather than compilation.

#![cfg(target_arch = "wasm32")]

use strata_wasm::StrataSession;
use wasm_bindgen_test::wasm_bindgen_test;

fn execute(session: &mut StrataSession, command: &str) -> serde_json::Value {
    let raw = session
        .execute(command)
        .expect("valid command executes without throwing");
    serde_json::from_str(&raw).expect("envelope is valid JSON")
}

// base64: "greeting" / "hello" / "flag" / "on"
const KEY_GREETING: &str = "Z3JlZXRpbmc=";
const VALUE_HELLO: &str = "aGVsbG8=";
const KEY_FLAG: &str = "ZmxhZw==";
const VALUE_ON: &str = "b24=";

#[wasm_bindgen_test]
fn kv_round_trip_through_the_serialized_surface() {
    let mut session = StrataSession::new().expect("session opens");
    let put = execute(
        &mut session,
        &format!(r#"{{"type":"kv_put","key":"{KEY_GREETING}","value":"{VALUE_HELLO}"}}"#),
    );
    assert!(put.get("error").is_none(), "put must not error: {put}");
    let get = execute(
        &mut session,
        &format!(r#"{{"type":"kv_get","key":"{KEY_GREETING}"}}"#),
    );
    assert!(
        get.to_string().contains(VALUE_HELLO),
        "get must return the stored value: {get}"
    );
}

#[wasm_bindgen_test]
fn invalid_command_json_throws_with_a_teaching_message() {
    let mut session = StrataSession::new().expect("session opens");
    assert!(
        session.execute("{not json").is_err(),
        "malformed JSON must throw"
    );
    assert!(
        session
            .execute(r#"{"type":"kv_levitate","key":"YQ=="}"#)
            .is_err(),
        "unknown command type must throw"
    );
}

#[wasm_bindgen_test]
fn executed_command_failures_are_error_envelopes_not_throws() {
    let mut session = StrataSession::new().expect("session opens");
    // A command that deserializes fine but fails at execution (reserved
    // branch name) must come back as an error envelope, not a throw —
    // throws are reserved for malformed command JSON.
    let envelope = execute(
        &mut session,
        r#"{"type":"branch_create","branch":"_system_"}"#,
    );
    assert!(
        envelope.get("error").is_some(),
        "executed failures surface as envelopes: {envelope}"
    );
}

#[wasm_bindgen_test]
fn branch_scoping_isolates_writes_in_the_browser_session() {
    let mut session = StrataSession::new().expect("session opens");
    assert_eq!(session.branch(), "default");
    let created = execute(&mut session, r#"{"type":"branch_create","branch":"dev"}"#);
    assert!(created.get("error").is_none(), "branch create: {created}");
    session.set_branch("dev").expect("valid branch");
    let put = execute(
        &mut session,
        &format!(r#"{{"type":"kv_put","key":"{KEY_FLAG}","value":"{VALUE_ON}"}}"#),
    );
    assert!(put.get("error").is_none(), "dev put: {put}");

    session.set_branch("default").expect("valid branch");
    let get = execute(
        &mut session,
        &format!(r#"{{"type":"kv_get","key":"{KEY_FLAG}"}}"#),
    );
    assert!(
        !get.to_string().contains(VALUE_ON),
        "default branch must not see dev writes: {get}"
    );

    assert!(
        session.set_branch("_system_").is_err(),
        "reserved branch names must throw"
    );
}
