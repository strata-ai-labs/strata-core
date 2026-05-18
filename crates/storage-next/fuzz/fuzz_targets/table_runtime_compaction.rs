#![no_main]

use libfuzzer_sys::fuzz_target;
use strata_storage_next::testkit::check_table_runtime_compaction_contract;

fuzz_target!(|data: &[u8]| {
    if let Err(violation) = check_table_runtime_compaction_contract(data) {
        panic!("table runtime compaction invariant violation: {violation}");
    }
});
