#![no_main]

use libfuzzer_sys::fuzz_target;
use strata_storage_next::testkit::check_commit_runtime_conflict_contract;

fuzz_target!(|data: &[u8]| {
    if let Err(violation) = check_commit_runtime_conflict_contract(data) {
        panic!("commit runtime conflict invariant violation: {violation}");
    }
});
