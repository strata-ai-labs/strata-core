use super::run_quarantine_service_script;

#[test]
fn quarantine_service_script_accepts_empty_input() {
    let outcome = run_quarantine_service_script(&[]).expect("empty script");

    assert_eq!(outcome.steps_executed(), 0);
}

#[test]
fn quarantine_service_script_exercises_quarantine_reconcile_and_purge() {
    let script = [
        0, 0, 0, 2, 0, 0, 9, 0, // seed source
        1, 0, 0, 2, 0, 0, 0, 1, // quarantine successfully
        6, 0, 0, 0, 0, 0, 0, 0, // reconcile clean inventory
        2, 0, 0, 0, 0, 0, 0, 0, // purge inventory
        4, 0, 1, 1, 0, 0, 3, 0, // insert unlisted object
        6, 0, 1, 0, 0, 0, 0, 0, // reconcile unlisted object
    ];

    let outcome = run_quarantine_service_script(&script).expect("script");

    assert_eq!(outcome.steps_executed(), script.len() / 8);
}
