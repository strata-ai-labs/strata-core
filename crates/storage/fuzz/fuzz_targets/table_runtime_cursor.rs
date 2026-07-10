#![no_main]

use libfuzzer_sys::fuzz_target;
use strata_storage::testkit::check_table_runtime_cursor_contract;

fuzz_target!(|data: &[u8]| {
    if let Err(violation) = check_table_runtime_cursor_contract(data) {
        panic!("table runtime cursor invariant violation: {violation}");
    }
});
