#![no_main]

use libfuzzer_sys::fuzz_target;
use strata_storage::testkit::run_snapshot_service_script;

fuzz_target!(|data: &[u8]| {
    if let Err(violation) = run_snapshot_service_script(data) {
        panic!("snapshot service invariant violation: {violation}");
    }
});
