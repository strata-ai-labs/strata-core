use super::{
    branch_name, product_space, EventService, Executor, ExecutorResult, GraphService, JsonService,
    VectorService,
};

impl Executor {
    pub(super) fn kv_service(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
    ) -> ExecutorResult<strata_engine_next::KvService<'_>> {
        let branch = branch_name(branch, &self.default_branch)?;
        let space = product_space(space)?;
        {
            let branches = self.database.branches()?;
            branches.get(&branch)?;
        }
        Ok(self.database.kv(branch, space)?)
    }

    pub(super) fn json_service(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
    ) -> ExecutorResult<JsonService<'_>> {
        let branch = branch_name(branch, &self.default_branch)?;
        let space = product_space(space)?;
        {
            let branches = self.database.branches()?;
            branches.get(&branch)?;
        }
        Ok(self.database.json(branch, space)?)
    }

    pub(super) fn vector_service(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
    ) -> ExecutorResult<VectorService<'_>> {
        let branch = branch_name(branch, &self.default_branch)?;
        let space = product_space(space)?;
        {
            let branches = self.database.branches()?;
            branches.get(&branch)?;
        }
        Ok(self.database.vector(branch, space)?)
    }

    pub(super) fn event_service(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
    ) -> ExecutorResult<EventService<'_>> {
        let branch = branch_name(branch, &self.default_branch)?;
        let space = product_space(space)?;
        {
            let branches = self.database.branches()?;
            branches.get(&branch)?;
        }
        Ok(self.database.event(branch, space)?)
    }

    pub(super) fn graph_service(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
    ) -> ExecutorResult<GraphService<'_>> {
        let branch = branch_name(branch, &self.default_branch)?;
        let space = product_space(space)?;
        {
            let branches = self.database.branches()?;
            branches.get(&branch)?;
        }
        Ok(self.database.graph(branch, space)?)
    }
}
