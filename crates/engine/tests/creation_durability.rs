//! Creation durability: a database must be born durable.
//!
//! A directory copy taken while the creating session is still alive contains
//! exactly the bytes a SIGKILL would preserve — page-cache contents survive
//! both, user-space buffers survive neither. If the control-plane seed is
//! still sitting in the buffered WAL when the copy (or kill) happens, the
//! copy is a store whose durable manifest exists but whose control plane
//! does not: permanently unopenable. Creation must therefore not return a
//! usable database before its seed is durable.

mod common;

use strata_engine::{Database, DurableLocalOpenOptions};

use common::{branch, key, space, value};

fn copy_dir(source: &std::path::Path, destination: &std::path::Path) {
    std::fs::create_dir_all(destination).expect("copy mkdir");
    for entry in std::fs::read_dir(source).expect("copy read_dir") {
        let entry = entry.expect("dir entry");
        let target = destination.join(entry.file_name());
        if entry.file_type().expect("file type").is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).expect("copy file");
        }
    }
}

/// The kill-surviving snapshot of a just-created store must reopen as a
/// usable database — not brick with a control-plane data-loss refusal.
#[test]
fn freshly_created_store_survives_a_first_session_kill() {
    let dir = tempfile::tempdir().expect("tmp");
    let original = dir.path().join("db");
    let snapshot = dir.path().join("db-after-kill");

    // Create and HOLD the first session (no clean close, no drop): the copy
    // below sees only what the OS has — exactly the SIGKILL survivor state.
    let _first_session = Database::open_local(&original, DurableLocalOpenOptions::new())
        .expect("create database")
        .into_database();
    copy_dir(&original, &snapshot);

    // The snapshot must be a usable database.
    let mut reopened = Database::open_local(&snapshot, DurableLocalOpenOptions::new())
        .unwrap_or_else(|error| {
            panic!(
                "a store killed during its first session must reopen usable, got: {error:?} \
                 (code {})",
                error.code()
            )
        })
        .into_database();
    let mut kv = reopened
        .kv(branch("default"), space("app"))
        .expect("default branch usable after first-session kill");
    kv.put(key(b"probe"), value(b"1")).expect("post-kill write");
    common::assert_branch_value(&mut reopened, "default", "app", b"probe", b"1");
}
