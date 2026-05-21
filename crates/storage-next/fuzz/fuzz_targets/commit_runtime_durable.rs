#![no_main]

use libfuzzer_sys::fuzz_target;
use strata_storage_next::testkit::check_commit_runtime_durable_contract;

fuzz_target!(|data: &[u8]| {
    if let Err(violation) = check_commit_runtime_durable_contract(data) {
        panic!("commit runtime durable invariant violation: {violation}");
    }
});
