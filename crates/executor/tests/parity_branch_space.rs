//! TCP4.7 axis: branch ↔ space lifecycle, resolution, and naming parity.
//!
//! Branches and spaces are the two scoping axes of every data command and
//! should present one lifecycle contract. Three ledgered divergences today,
//! all #2700: lifecycle channels (idempotent space vs strict branch),
//! unknown-scope resolution on writes (space auto-mints, branch refuses),
//! and naming strictness (branch validates, space barely does).

use serde_json::json;

#[path = "parity/support.rs"]
mod support;

/// Contract that holds today on both axes: deleting the default scope is
/// refused with a typed `invalid_argument` code.
#[test]
fn deleting_the_default_scope_is_refused_on_both_axes() {
    let mut executor = support::executor();
    let space_code = support::run_err_code(
        &mut executor,
        &json!({"type": "space_delete", "space": "default", "force": false}),
    );
    assert_eq!(space_code, "invalid_argument.engine.space_delete_default");

    let branches = support::run(&mut executor, &json!({"type": "branch_list"}));
    let default_branch = branches["data"]["items"][0]["name"]
        .as_str()
        .unwrap_or_else(|| panic!("branch_list carries item names: {branches}"))
        .to_owned();
    let branch_code = support::run_err_code(
        &mut executor,
        &json!({"type": "branch_delete", "branch": default_branch}),
    );
    assert_eq!(branch_code, "invalid_argument.engine.branch_delete");
}

/// PIN #2700 (lifecycle): duplicate create and delete-missing succeed on the
/// space axis but raise typed errors on the branch axis.
#[test]
fn pin_2700_lifecycle_channels_differ_between_axes() {
    support::pinned("branch_space_lifecycle", 2700);
    let mut executor = support::executor();

    support::run(
        &mut executor,
        &json!({"type": "space_create", "space": "dup"}),
    );
    support::run(
        &mut executor,
        &json!({"type": "space_create", "space": "dup"}),
    );

    support::run(
        &mut executor,
        &json!({"type": "branch_create", "branch": "dup"}),
    );
    let duplicate_branch = support::run_err_code(
        &mut executor,
        &json!({"type": "branch_create", "branch": "dup"}),
    );
    assert_eq!(
        duplicate_branch, "already_exists.engine.branch",
        "today: duplicate branch create raises while duplicate space create is \
         idempotent; if this diverges, revisit ledger entry #2700"
    );

    support::run(
        &mut executor,
        &json!({"type": "space_delete", "space": "ghost", "force": false}),
    );
    let missing_branch = support::run_err_code(
        &mut executor,
        &json!({"type": "branch_delete", "branch": "ghost"}),
    );
    assert_eq!(missing_branch, "not_found.engine.branch");
}

/// PIN #2700 (resolution): a write naming an unknown space silently mints
/// it; a write naming an unknown branch is refused.
#[test]
fn pin_2700_unknown_scope_resolution_differs_on_writes() {
    support::pinned("branch_space_resolution", 2700);
    let mut executor = support::executor();

    support::run(
        &mut executor,
        &json!({"type": "kv_put", "key": "YQ==", "value": "b25l", "space": "typo-space"}),
    );
    let exists = support::run(
        &mut executor,
        &json!({"type": "space_exists", "space": "typo-space"}),
    );
    assert_eq!(
        exists["data"].as_bool(),
        Some(true),
        "today: the typo'd space was silently auto-created by the write"
    );

    let unknown_branch = support::run_err_code(
        &mut executor,
        &json!({"type": "kv_put", "key": "YQ==", "value": "b25l", "branch": "ghost-branch"}),
    );
    assert_eq!(unknown_branch, "not_found.engine.branch");
}

/// PIN #2700 (naming): branch names reject whitespace-only and control
/// characters; space names accept both.
#[test]
fn pin_2700_naming_strictness_differs_between_axes() {
    support::pinned("branch_space_naming", 2700);
    let mut executor = support::executor();

    let whitespace_branch = support::run_err_code(
        &mut executor,
        &json!({"type": "branch_create", "branch": " "}),
    );
    assert!(
        whitespace_branch.starts_with("invalid_argument."),
        "branch rejects a whitespace-only name (got {whitespace_branch})"
    );
    support::run(
        &mut executor,
        &json!({"type": "space_create", "space": " "}),
    );

    let control_branch = support::run_err_code(
        &mut executor,
        &json!({"type": "branch_create", "branch": "a\u{0001}b"}),
    );
    assert!(
        control_branch.starts_with("invalid_argument."),
        "branch rejects a control character (got {control_branch})"
    );
    support::run(
        &mut executor,
        &json!({"type": "space_create", "space": "a\u{0001}b"}),
    );
}

/// Ledger guard (entry ⇒ pin): every `branch_space*` ledger entry is pinned
/// by a test in this target.
#[test]
fn every_branch_space_ledger_entry_is_pinned_here() {
    support::assert_ledger_entries_all_pinned(
        "branch_space",
        &[
            ("branch_space_lifecycle", 2700),
            ("branch_space_resolution", 2700),
            ("branch_space_naming", 2700),
        ],
    );
}
