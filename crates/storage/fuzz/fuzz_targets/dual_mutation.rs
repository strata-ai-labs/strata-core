//! TCP4.6c dual-mutation target (dbsqlfuzz mold): the input co-mutates the
//! operation stream AND the on-disk bytes of a real durable store across
//! close → damage → reopen epochs; every reopen is judged by the recovery
//! oracle (refuse loud, or be a truthful prefix of acknowledged history).

#![no_main]

use libfuzzer_sys::fuzz_target;
use strata_storage::testkit::check_dual_mutation_contract;

fuzz_target!(|data: &[u8]| {
    let scratch = tempfile::tempdir().expect("dual-mutation scratch dir");
    if let Err(violation) = check_dual_mutation_contract(scratch.path(), data) {
        panic!("dual-mutation invariant violation: {violation}");
    }
});
