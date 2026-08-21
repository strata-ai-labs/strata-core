//! Memory-budget provenance on the open summary (#2905): an explicit budget is
//! reported as explicit; an un-budgeted open derives from the host (or falls
//! back to the fixed default where the host reports nothing) — never silently.

mod common;

use common::open_cache_database;
use strata_engine::{CacheOpenOptions, Database, DatabaseOpenOutcome, MemoryBudgetSource};

const MIB: u64 = 1024 * 1024;
const GIB: u64 = 1024 * MIB;

#[test]
fn explicit_budget_is_reported_as_explicit() {
    let db = Database::open_cache(CacheOpenOptions::new().with_memory_budget(64 * MIB))
        .map(DatabaseOpenOutcome::into_database)
        .expect("cache database opens");
    assert_eq!(
        db.open_summary().memory_budget_source(),
        MemoryBudgetSource::Explicit {
            total_bytes: 64 * MIB
        }
    );
}

#[test]
fn unbudgeted_open_derives_from_host_or_falls_back_with_provenance() {
    let db = open_cache_database().expect("cache database opens");
    let source = db.open_summary().memory_budget_source();
    assert!(
        !matches!(source, MemoryBudgetSource::Explicit { .. }),
        "no budget was set, so provenance cannot be explicit: {source:?}"
    );
    assert!(
        source.total_bytes() >= MIB,
        "budget respects the minimum: {source:?}"
    );
    match source {
        MemoryBudgetSource::DerivedFromHost {
            total_bytes,
            usable_host_bytes,
        } => {
            // 25% of usable memory, clamped to [1 MiB, 8 GiB] — a ceiling, not a reservation.
            assert_eq!(total_bytes, (usable_host_bytes / 4).clamp(MIB, 8 * GIB));
            assert!(total_bytes <= 8 * GIB);
        }
        MemoryBudgetSource::FixedDefault { total_bytes } => {
            // Hosts the probe cannot read (macOS/Windows/wasm today) keep the fixed default.
            assert_eq!(total_bytes, 512 * MIB);
        }
        other => panic!("unexpected provenance: {other:?}"),
    }
}
