#![no_main]

use libfuzzer_sys::fuzz_target;
use strata_storage_next::testkit::check_lifecycle_retention_fuzz_contract;

fuzz_target!(|data: &[u8]| {
    if let Err(violation) = check_lifecycle_retention_fuzz_contract(data) {
        panic!("lifecycle retention invariant violation: {violation}");
    }
});
