#![no_main]

use libfuzzer_sys::fuzz_target;
use strata_storage::testkit::check_commit_runtime_timeline_contract;

fuzz_target!(|data: &[u8]| {
    if let Err(violation) = check_commit_runtime_timeline_contract(data) {
        panic!("commit runtime timeline invariant violation: {violation}");
    }
});
