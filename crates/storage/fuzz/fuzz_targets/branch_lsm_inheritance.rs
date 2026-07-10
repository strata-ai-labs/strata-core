#![no_main]

use libfuzzer_sys::fuzz_target;
use strata_storage::testkit::check_branch_lsm_inheritance_contract;

fuzz_target!(|data: &[u8]| {
    if let Err(violation) = check_branch_lsm_inheritance_contract(data) {
        panic!("branch LSM inheritance invariant violation: {violation}");
    }
});
