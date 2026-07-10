use crate::testkit::run_quarantine_service_script;
use proptest::collection::vec;
use proptest::prelude::{any, Strategy};
use proptest::test_runner::{
    Config, FileFailurePersistence, TestCaseError, TestCaseResult, TestRunner,
};

#[test]
fn quarantine_service_state_machine_preserves_recovery_invariants() {
    let mut runner = TestRunner::new(Config {
        // Check in this file only when proptest finds a real failing seed; an
        // absent file means no historical regression has been captured yet.
        failure_persistence: Some(Box::new(FileFailurePersistence::Direct(
            "proptest-regressions/quarantine_state_machine.txt",
        ))),
        ..Config::default()
    });

    runner
        .run(&quarantine_script_strategy(), run_quarantine_script)
        .expect("quarantine service state machine should preserve recovery invariants");
}

fn quarantine_script_strategy() -> impl Strategy<Value = Vec<[u8; 8]>> {
    // The bytecode interpreter caps payload bytes internally; this range keeps
    // operation histories long enough for retry windows without slowing CI.
    vec(any::<[u8; 8]>(), 1..=96)
}

fn run_quarantine_script(chunks: Vec<[u8; 8]>) -> TestCaseResult {
    let mut script = Vec::with_capacity(chunks.len() * 8);
    for chunk in chunks {
        script.extend_from_slice(&chunk);
    }

    run_quarantine_service_script(&script)
        .map(|_| ())
        .map_err(|violation| TestCaseError::fail(violation.to_string()))
}
