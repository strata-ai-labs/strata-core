#![no_main]

use libfuzzer_sys::fuzz_target;
use strata_storage::testkit::classify_object_text;

// L2: arbitrary object-name text routed through every classifier must never
// panic — only accept, reject, or leave unclassified. Names reach this path
// from a backend `list` during recovery, so a panic here is a recovery crash.
fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);
    let _ = classify_object_text(&text);
});
