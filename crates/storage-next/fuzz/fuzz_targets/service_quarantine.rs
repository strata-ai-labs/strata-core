#![no_main]

use libfuzzer_sys::fuzz_target;
use strata_storage_next::testkit::run_quarantine_service_script;

fuzz_target!(|data: &[u8]| {
    // Panicking is intentional in fuzz targets: libFuzzer needs a crash to
    // persist the operation stream that violated the service model.
    if let Err(violation) = run_quarantine_service_script(data) {
        panic!("quarantine service invariant violation: {violation}");
    }
});
