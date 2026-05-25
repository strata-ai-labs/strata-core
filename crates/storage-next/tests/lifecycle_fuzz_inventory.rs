//! Lifecycle fuzz target and corpus inventory.

#![deny(unsafe_code)]

mod common;

use std::fs;

#[test]
fn lifecycle_fuzz_targets_are_registered() {
    let manifest = fs::read_to_string(common::crate_root().join("fuzz/Cargo.toml"))
        .expect("read fuzz manifest");

    for target in [
        "lifecycle_recovery",
        "lifecycle_maintenance",
        "lifecycle_retention",
    ] {
        assert!(manifest.contains(&format!("name = \"{target}\"")));
        assert!(manifest.contains(&format!("fuzz_targets/{target}.rs")));
    }
}

#[test]
fn lifecycle_fuzz_targets_call_distinct_contracts() {
    let root = common::crate_root();
    let contracts = [
        "check_lifecycle_recovery_fuzz_contract",
        "check_lifecycle_maintenance_fuzz_contract",
        "check_lifecycle_retention_fuzz_contract",
    ];
    for (target, contract) in [
        (
            "lifecycle_recovery",
            "check_lifecycle_recovery_fuzz_contract",
        ),
        (
            "lifecycle_maintenance",
            "check_lifecycle_maintenance_fuzz_contract",
        ),
        (
            "lifecycle_retention",
            "check_lifecycle_retention_fuzz_contract",
        ),
    ] {
        let text = fs::read_to_string(root.join(format!("fuzz/fuzz_targets/{target}.rs")))
            .expect("read fuzz target");
        assert!(text.contains(contract), "{target} does not call {contract}");
        for other in contracts {
            if other != contract {
                assert!(
                    !text.contains(other),
                    "{target} should not call unrelated lifecycle fuzz contract {other}"
                );
            }
        }
        assert!(!text.contains("check_lifecycle_scaffold_contract"));
    }
}

#[test]
fn lifecycle_fuzz_corpora_have_non_empty_seed_files() {
    let root = common::crate_root();
    for (target, seeds) in [
        (
            "lifecycle_recovery",
            ["valid_seed", "corrupt_seed", "mixed_seed"],
        ),
        (
            "lifecycle_maintenance",
            ["valid_seed", "fault_seed", "close_seed"],
        ),
        (
            "lifecycle_retention",
            ["valid_seed", "blocked_seed", "purge_seed"],
        ),
    ] {
        let dir = root.join(format!("fuzz/corpus/{target}"));
        for seed in seeds {
            let path = dir.join(seed);
            assert!(path.is_file(), "{} is missing", path.display());
            assert!(fs::metadata(&path).expect("seed metadata").len() > 0);
        }
    }
}

#[test]
fn lifecycle_fuzz_named_seeds_are_structured_route_scripts() {
    let root = common::crate_root();
    for (target, seeds) in [
        (
            "lifecycle_recovery",
            ["valid_seed", "corrupt_seed", "mixed_seed"],
        ),
        (
            "lifecycle_maintenance",
            ["valid_seed", "fault_seed", "close_seed"],
        ),
        (
            "lifecycle_retention",
            ["valid_seed", "blocked_seed", "purge_seed"],
        ),
    ] {
        for seed in seeds {
            let bytes = fs::read(root.join(format!("fuzz/corpus/{target}/{seed}")))
                .expect("read lifecycle fuzz seed");
            assert!(
                bytes.contains(&b'|') && bytes.contains(&b'='),
                "{target}/{seed} should encode a route script body, not a plain label"
            );
        }
    }
}

/// Every named lifecycle fuzz seed must begin with the 6-byte binary header
/// `\xfa LFZ <route> <sub>`. The route byte feeds the modulo-fallback
/// routing in `*_fuzz_contract::from_script` so distinct seeds reach
/// distinct routes even when libfuzzer mutates the text body to bytes that
/// no longer match the substring decoders. The sub byte distinguishes seed
/// files within the same target so a mutator preserving the first byte
/// still has a second routing dimension to explore.
#[test]
fn lifecycle_fuzz_named_seeds_carry_binary_route_header() {
    const MAGIC: [u8; 4] = [0xfa, b'L', b'F', b'Z'];
    let root = common::crate_root();
    let mut sub_bytes = Vec::new();
    for (target, seeds) in [
        (
            "lifecycle_recovery",
            ["valid_seed", "corrupt_seed", "mixed_seed"],
        ),
        (
            "lifecycle_maintenance",
            ["valid_seed", "fault_seed", "close_seed"],
        ),
        (
            "lifecycle_retention",
            ["valid_seed", "blocked_seed", "purge_seed"],
        ),
    ] {
        for seed in seeds {
            let path = root.join(format!("fuzz/corpus/{target}/{seed}"));
            let bytes = fs::read(&path).expect("read lifecycle fuzz seed");
            assert!(
                bytes.len() >= 6,
                "{} is shorter than the 6-byte binary header",
                path.display()
            );
            assert_eq!(
                &bytes[..4],
                &MAGIC,
                "{} is missing the 0xfa LFZ binary magic header",
                path.display()
            );
            let route = bytes[4];
            assert!(
                route <= 7,
                "{} has out-of-range route byte 0x{:02x}",
                path.display(),
                route
            );
            sub_bytes.push((path, bytes[5]));
        }
    }
    // Sub bytes must be distinct across all named seeds so libfuzzer mutators
    // that preserve the magic still see route+sub diversity in the corpus.
    let mut seen = std::collections::BTreeSet::new();
    for (path, sub) in &sub_bytes {
        assert!(
            seen.insert(*sub),
            "duplicate sub-route byte 0x{:02x} at {}",
            sub,
            path.display()
        );
    }
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn lifecycle_recovery_fuzz_seed_hits_valid_and_corrupt_routes() {
    let outcome = strata_storage_next::testkit::check_lifecycle_recovery_fuzz_contract(
        &seed_bytes("lifecycle_recovery", "mixed_seed"),
    )
    .expect("recovery fuzz contract");

    assert!(outcome.recovered_visibility_match_cases() > 0);
    assert!(outcome.lossy_degraded_health_check_cases() > 0);
    assert_eq!(outcome.input_maintenance_route_cases(), 0);
    assert_eq!(outcome.input_reclaim_route_cases(), 0);
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn lifecycle_maintenance_fuzz_seed_hits_task_and_close_routes() {
    let outcome = strata_storage_next::testkit::check_lifecycle_maintenance_fuzz_contract(
        &seed_bytes("lifecycle_maintenance", "close_seed"),
    )
    .expect("maintenance fuzz contract");

    assert!(outcome.input_maintenance_route_cases() > 0);
    assert!(outcome.close_idempotence_check_cases() > 0);
    assert_eq!(outcome.input_reclaim_route_cases(), 0);
    assert_eq!(outcome.recovered_visibility_match_cases(), 0);
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn lifecycle_maintenance_fuzz_regression_keeps_materialization_branches_distinct() {
    let mut script = b"lifecycle-maintenance-valid-eeed\n".to_vec();
    script[4] = b'x';
    script[5] = b'x';
    assert_eq!(script[4], script[5]);
    assert_eq!(script[28], script[29]);
    let outcome = strata_storage_next::testkit::check_lifecycle_maintenance_fuzz_contract(&script)
        .expect("maintenance fuzz contract");

    assert!(outcome.input_maintenance_route_cases() > 0);
    assert!(outcome.close_idempotence_check_cases() > 0);
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn lifecycle_retention_fuzz_seed_hits_delete_and_defer_routes() {
    let outcome = strata_storage_next::testkit::check_lifecycle_retention_fuzz_contract(
        &seed_bytes("lifecycle_retention", "purge_seed"),
    )
    .expect("retention fuzz contract");

    assert!(outcome.input_reclaim_route_cases() > 0);
    assert!(outcome.deletion_subset_check_cases() > 0);
    assert_eq!(outcome.input_maintenance_route_cases(), 0);
    assert_eq!(outcome.recovered_visibility_match_cases(), 0);
}

#[cfg(not(all(feature = "testkit", not(target_arch = "wasm32"))))]
#[test]
fn lifecycle_fuzz_seed_contracts_require_testkit() {}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
fn seed_bytes(target: &str, name: &str) -> Vec<u8> {
    fs::read(common::crate_root().join(format!("fuzz/corpus/{target}/{name}")))
        .expect("read lifecycle fuzz seed")
}
