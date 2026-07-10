#[cfg(test)]
mod tests {
    use super::{
        check_branch_lsm_inheritance_contract, check_branch_lsm_install_contract,
        check_branch_lsm_reads_contract, check_branch_lsm_scaffold_contract,
    };

    #[test]
    fn dedicated_branch_lsm_fuzz_contracts_exercise_their_surfaces() {
        let script = b"branch-lsm-dedicated-fuzz-contract-seed";

        check_branch_lsm_reads_contract(script).expect("branch read fuzz contract");
        check_branch_lsm_inheritance_contract(script).expect("branch inheritance fuzz contract");
        check_branch_lsm_install_contract(script).expect("branch install fuzz contract");
    }

    #[test]
    fn branch_lsm_scaffold_contract_checks_generated_scripts() {
        let outcome = check_branch_lsm_scaffold_contract(b"branch-lsm-scaffold-seed")
            .expect("branch scaffold contract");
        assert_ne!(outcome.latest_point_read_cases(), 0);
        assert_ne!(outcome.inherited_latest_read_cases(), 0);
        assert_ne!(outcome.compaction_output_install_cases(), 0);
        assert_ne!(outcome.snapshot_single_branch_install_cases(), 0);
    }
}
