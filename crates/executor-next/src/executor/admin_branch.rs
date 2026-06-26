use super::{
    branch_cleanup_item, branch_item, branch_name, output_admin_config, output_admin_describe,
    output_admin_health, output_admin_info, output_admin_metrics, output_space_create,
    output_space_delete, product_space, CommitVersion, Executor, ExecutorResult, Output, PageInfo,
    Timestamp, DEFAULT_BRANCH,
};

impl Executor {
    pub(super) fn execute_ping(&mut self) -> ExecutorResult<Output> {
        let summary = self.database.admin()?.ping();
        Ok(Output::Pong {
            version: summary.version,
        })
    }

    pub(super) fn execute_info(&mut self, branch: Option<&str>) -> ExecutorResult<Output> {
        let branch = branch_name(branch, &self.default_branch)?;
        let mut admin = self.database.admin()?;
        let summary = admin.info(Some(&branch))?;
        Ok(Output::DatabaseInfo(output_admin_info(&summary)))
    }

    pub(super) fn execute_health(&mut self, branch: Option<&str>) -> ExecutorResult<Output> {
        let branch = branch_name(branch, &self.default_branch)?;
        let mut admin = self.database.admin()?;
        let summary = admin.health(Some(&branch));
        Ok(Output::Health(output_admin_health(&summary)))
    }

    pub(super) fn execute_metrics(&mut self, branch: Option<&str>) -> ExecutorResult<Output> {
        let branch = branch_name(branch, &self.default_branch)?;
        let mut admin = self.database.admin()?;
        let summary = admin.metrics(Some(&branch))?;
        Ok(Output::Metrics(output_admin_metrics(&summary)))
    }

    pub(super) fn execute_describe(&mut self, branch: Option<&str>) -> ExecutorResult<Output> {
        let branch = branch_name(branch, &self.default_branch)?;
        let mut admin = self.database.admin()?;
        let summary = admin.describe(Some(&branch))?;
        Ok(Output::Described(output_admin_describe(&summary)))
    }

    pub(super) fn execute_config_get(&mut self) -> ExecutorResult<Output> {
        let admin = self.database.admin()?;
        Ok(Output::Config(output_admin_config(&admin.config())))
    }

    pub(super) fn execute_configure_get_key(&mut self, key: &str) -> ExecutorResult<Output> {
        let admin = self.database.admin()?;
        Ok(Output::ConfigValue(admin.config_value(key)?))
    }

    pub(super) fn execute_space_list(&mut self, branch: Option<&str>) -> ExecutorResult<Output> {
        let branch = branch_name(branch, &self.default_branch)?;
        let mut spaces = self.database.spaces(branch)?;
        Ok(Output::SpaceList {
            items: spaces
                .list()?
                .iter()
                .map(|space| space.as_str().to_owned())
                .collect(),
            page: PageInfo::terminal(),
        })
    }

    pub(super) fn execute_space_create(
        &mut self,
        branch: Option<&str>,
        space: &str,
    ) -> ExecutorResult<Output> {
        let branch = branch_name(branch, &self.default_branch)?;
        let space = product_space(Some(space))?;
        let mut spaces = self.database.spaces(branch)?;
        let outcome = spaces.create(space)?;
        Ok(output_space_create(&outcome))
    }

    pub(super) fn execute_space_exists(
        &mut self,
        branch: Option<&str>,
        space: &str,
    ) -> ExecutorResult<Output> {
        let branch = branch_name(branch, &self.default_branch)?;
        let space = product_space(Some(space))?;
        let mut spaces = self.database.spaces(branch)?;
        Ok(Output::Bool(spaces.exists(&space)?))
    }

    pub(super) fn execute_space_delete(
        &mut self,
        branch: Option<&str>,
        space: &str,
        force: bool,
    ) -> ExecutorResult<Output> {
        let branch = branch_name(branch, &self.default_branch)?;
        let space = product_space(Some(space))?;
        let mut spaces = self.database.spaces(branch)?;
        let outcome = spaces.delete(&space, force)?;
        Ok(output_space_delete(&outcome))
    }

    pub(super) fn execute_branch_list(&mut self) -> ExecutorResult<Output> {
        let branches = self
            .database
            .branches()?
            .list()?
            .iter()
            .map(branch_item)
            .collect();
        Ok(Output::Branches {
            items: branches,
            page: PageInfo::terminal(),
        })
    }

    pub(super) fn execute_branch_get(&mut self, branch: &str) -> ExecutorResult<Output> {
        let branch = branch_name(Some(branch), DEFAULT_BRANCH)?;
        let summary = self.database.branches()?.get(&branch)?;
        Ok(Output::Branch(branch_item(&summary)))
    }

    pub(super) fn execute_branch_create(&mut self, branch: &str) -> ExecutorResult<Output> {
        let branch = branch_name(Some(branch), DEFAULT_BRANCH)?;
        let outcome = self.database.branches()?.create(branch)?;
        Ok(Output::Branch(branch_item(outcome.branch())))
    }

    pub(super) fn execute_branch_fork_current(
        &mut self,
        source: &str,
        branch: &str,
    ) -> ExecutorResult<Output> {
        let source = branch_name(Some(source), DEFAULT_BRANCH)?;
        let branch = branch_name(Some(branch), DEFAULT_BRANCH)?;
        let outcome = self.database.branches()?.fork_current(&source, branch)?;
        Ok(Output::Branch(branch_item(outcome.branch())))
    }

    pub(super) fn execute_branch_fork_at_version(
        &mut self,
        source: &str,
        branch: &str,
        version: u64,
    ) -> ExecutorResult<Output> {
        let source = branch_name(Some(source), DEFAULT_BRANCH)?;
        let branch = branch_name(Some(branch), DEFAULT_BRANCH)?;
        let outcome = self.database.branches()?.fork_at_version(
            &source,
            branch,
            CommitVersion::new(version),
        )?;
        Ok(Output::Branch(branch_item(outcome.branch())))
    }

    pub(super) fn execute_branch_fork_at_timestamp(
        &mut self,
        source: &str,
        branch: &str,
        timestamp: u64,
    ) -> ExecutorResult<Output> {
        let source = branch_name(Some(source), DEFAULT_BRANCH)?;
        let branch = branch_name(Some(branch), DEFAULT_BRANCH)?;
        let outcome = self.database.branches()?.fork_at_timestamp(
            &source,
            branch,
            Timestamp::from_micros(timestamp),
        )?;
        Ok(Output::Branch(branch_item(outcome.branch())))
    }

    pub(super) fn execute_branch_delete(&mut self, branch: &str) -> ExecutorResult<Output> {
        let branch = branch_name(Some(branch), DEFAULT_BRANCH)?;
        let outcome = self.database.branches()?.delete(&branch)?;
        Ok(Output::BranchDeleteResult {
            branch: branch_item(outcome.branch()),
            generation_before: outcome.generation_before(),
            generation_after: outcome.generation_after(),
            cleanup: outcome.cleanup().map(branch_cleanup_item),
        })
    }
}
