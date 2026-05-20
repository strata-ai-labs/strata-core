#![no_main]

use libfuzzer_sys::fuzz_target;
use strata_storage_next::testkit::check_branch_lsm_install_contract;

fuzz_target!(|data: &[u8]| {
    if let Err(violation) = check_branch_lsm_install_contract(data) {
        panic!("branch LSM install invariant violation: {violation}");
    }
});
