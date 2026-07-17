#![no_main]

use libfuzzer_sys::fuzz_target;
use strata_storage::testkit::assert_u64_id_roundtrips;

// L2: every canonical WAL-segment and snapshot name a constructor emits must
// classify back to the same u64 id. A decode bug here silently misroutes
// recovery to the wrong durable object.
fuzz_target!(|data: &[u8]| {
    let ids: Vec<u64> = data
        .chunks_exact(8)
        .map(|chunk| u64::from_le_bytes(chunk.try_into().expect("8 bytes")))
        .collect();
    if !ids.is_empty() {
        let _ = assert_u64_id_roundtrips(&ids);
    }
});
