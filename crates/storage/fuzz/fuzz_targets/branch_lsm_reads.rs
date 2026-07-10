#![no_main]

use libfuzzer_sys::fuzz_target;
use strata_storage::testkit::check_branch_lsm_reads_contract;

fuzz_target!(|data: &[u8]| {
    if let Err(violation) = check_branch_lsm_reads_contract(data) {
        panic!("branch LSM read invariant violation: {violation}");
    }
});
