//! Large-volume cache liveness (#2538 regression, engine layer): every
//! acknowledged write stays readable after the store grows past the
//! active-memtable rotation threshold (~64MB). Before the fix, cache mode
//! silently lost every row committed after the first rotation.

mod common;

use strata_engine::{Database, KvKey, KvValue};

use common::{branch, open_cache_database, open_durable_database, space};

fn run_database_modes(exercise: fn(Database)) {
    exercise(open_cache_database().expect("cache open succeeds"));

    let tempdir = tempfile::tempdir().expect("tempdir");
    exercise(open_durable_database(tempdir.path()).expect("durable open succeeds"));
}

#[test]
fn acknowledged_writes_survive_memtable_rotation_in_cache_and_durable_modes() {
    run_database_modes(exercise_volume_liveness);
}

fn exercise_volume_liveness(mut database: Database) {
    let mut kv = database
        .kv(branch("default"), space("default"))
        .expect("kv service opens");

    // ~75MB total: 2,400 rows of 32KiB in 64-row commits — crosses the 64MB
    // rotation threshold with margin, stays inside the frozen-pool budget,
    // and keeps every commit far below the mutation-count cap.
    let total_rows = 2_400usize;
    let value_bytes = vec![0x51u8; 32 * 1024];
    for batch_start in (0..total_rows).step_by(64) {
        let entries: Vec<(KvKey, KvValue)> = (batch_start..(batch_start + 64).min(total_rows))
            .map(|index| {
                (
                    KvKey::new(format!("volume-{index:06}")).expect("key"),
                    KvValue::new(value_bytes.clone()),
                )
            })
            .collect();
        kv.put_batch(entries).expect("batch acknowledged");
    }

    let mut missing = Vec::new();
    for index in 0..total_rows {
        let key = KvKey::new(format!("volume-{index:06}")).expect("key");
        if kv.get(&key).expect("read succeeds").is_none() {
            missing.push(index);
        }
    }
    assert!(
        missing.is_empty(),
        "acknowledged rows unreadable after rotation: {} missing, first at {:?}",
        missing.len(),
        missing.first()
    );
}
