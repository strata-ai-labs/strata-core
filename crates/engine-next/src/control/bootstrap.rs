//! Control-plane bootstrap and load.

use std::collections::BTreeMap;

use crate::api::{ControlDiagnostics, ControlHealthStatus, SpaceCatalogDiagnostics};
use crate::branch::catalog::{BranchCatalogRecord, DEFAULT_BRANCH_GENERATION, SYSTEM_BRANCH_ID};
use crate::branch::BranchName;
use crate::diagnostics::{EngineError, EngineErrorClass, EngineResult};
use crate::persistence::{
    branch_catalog_key, branch_default_key, branch_index_key, branch_pending_index_key,
    branch_pending_key, capability_registry_key, database_identity_key,
    local_instance_identity_key, migration_registry_key, storage_registry_key, CommitPlan,
    ReadSelector, RowAddress, RowClass, RowMutation, StoragePersistence,
};

use super::records::{
    decode_branch_index, decode_branch_record, decode_capability_registry,
    decode_database_identity, decode_default_branch, decode_local_instance_identity,
    decode_migration_registry, decode_pending_branch_index, decode_pending_branch_record,
    decode_storage_registry, encode_branch_index, encode_branch_record, encode_capability_registry,
    encode_database_identity, encode_default_branch, encode_local_instance_identity,
    encode_migration_registry, encode_pending_branch_index, encode_pending_branch_record,
    encode_storage_registry, DatabaseIdentityRecord,
};

#[derive(Clone, Debug)]
pub(crate) struct ControlPlane {
    default_branch: BranchName,
    branches: BTreeMap<BranchName, BranchCatalogRecord>,
    terminal_error: Option<EngineError>,
}

impl ControlPlane {
    pub(crate) fn require_healthy(&self) -> EngineResult<()> {
        match &self.terminal_error {
            Some(error) => Err(error.clone()),
            None => Ok(()),
        }
    }

    pub(crate) fn fail_closed_after_branch_operation_error(&mut self, error: &EngineError) {
        self.terminal_error = Some(EngineError::control_plane_unavailable(format!(
            "engine control plane is unavailable after branch operation catalog update failed: {}",
            error.code()
        )));
    }

    pub(crate) fn default_branch(&self) -> &BranchName {
        &self.default_branch
    }

    pub(crate) fn list_branches(&self) -> Vec<BranchCatalogRecord> {
        self.branches
            .values()
            .filter(|record| record.is_active())
            .cloned()
            .collect()
    }

    pub(crate) fn lookup_branch(&self, name: &BranchName) -> Option<&BranchCatalogRecord> {
        self.branches.get(name).filter(|record| record.is_active())
    }

    pub(crate) fn lookup_any_branch(&self, name: &BranchName) -> Option<&BranchCatalogRecord> {
        self.branches.get(name)
    }

    pub(crate) fn contains_branch(&self, name: &BranchName) -> bool {
        self.lookup_branch(name).is_some()
    }

    pub(crate) fn active_branch_count(&self) -> usize {
        self.branches
            .values()
            .filter(|record| record.is_active())
            .count()
    }

    pub(crate) fn diagnostics(
        &self,
        persistence: &mut StoragePersistence,
        branch: Option<&BranchName>,
    ) -> ControlDiagnostics {
        if self.terminal_error.is_some() {
            return ControlDiagnostics::new(
                ControlHealthStatus::Unavailable,
                ControlHealthStatus::Unavailable,
                ControlHealthStatus::Unavailable,
                self.default_branch.clone(),
                self.active_branch_count(),
                branch.map(|branch| {
                    SpaceCatalogDiagnostics::new(
                        branch.clone(),
                        ControlHealthStatus::Unavailable,
                        None,
                    )
                }),
            );
        }

        let space_catalog = branch.map(|branch| {
            let Some(record) = self.lookup_branch(branch) else {
                return SpaceCatalogDiagnostics::new(
                    branch.clone(),
                    ControlHealthStatus::Missing,
                    None,
                );
            };
            match super::space::validate_required_space_rows_and_count(persistence, record) {
                Ok(count) => SpaceCatalogDiagnostics::new(
                    branch.clone(),
                    ControlHealthStatus::Healthy,
                    Some(count),
                ),
                Err(error) if error.class() == EngineErrorClass::Corruption => {
                    SpaceCatalogDiagnostics::new(branch.clone(), ControlHealthStatus::Corrupt, None)
                }
                Err(_) => SpaceCatalogDiagnostics::new(
                    branch.clone(),
                    ControlHealthStatus::Unavailable,
                    None,
                ),
            }
        });

        ControlDiagnostics::new(
            ControlHealthStatus::Healthy,
            ControlHealthStatus::Healthy,
            ControlHealthStatus::Healthy,
            self.default_branch.clone(),
            self.active_branch_count(),
            space_catalog,
        )
    }

    pub(crate) fn next_generation_for_name(&self, name: &BranchName) -> u64 {
        self.lookup_any_branch(name)
            .map_or(DEFAULT_BRANCH_GENERATION, |record| record.generation() + 1)
    }

    pub(crate) fn insert_branch(&mut self, record: BranchCatalogRecord) {
        self.branches.insert(record.name().clone(), record);
    }

    pub(crate) fn begin_branch_operation(
        persistence: &mut StoragePersistence,
        record: &BranchCatalogRecord,
    ) -> EngineResult<()> {
        let names = [record.name().clone()];
        let mutations = vec![
            RowMutation::put(
                control_address(RowClass::BranchControl, branch_pending_index_key()),
                encode_pending_branch_index(&names)?,
            ),
            RowMutation::put(
                control_address(
                    RowClass::BranchControl,
                    branch_pending_key(record.name().as_str()),
                ),
                encode_pending_branch_record(record),
            ),
        ];
        persistence.commit(&CommitPlan::new(SYSTEM_BRANCH_ID, mutations, None))?;
        Ok(())
    }

    pub(crate) fn clear_pending_branch_operation(
        persistence: &mut StoragePersistence,
        record: &BranchCatalogRecord,
    ) -> EngineResult<()> {
        let mutations = vec![
            RowMutation::put(
                control_address(RowClass::BranchControl, branch_pending_index_key()),
                encode_pending_branch_index(&[])?,
            ),
            RowMutation::delete(control_address(
                RowClass::BranchControl,
                branch_pending_key(record.name().as_str()),
            )),
        ];
        persistence.commit(&CommitPlan::new(SYSTEM_BRANCH_ID, mutations, None))?;
        Ok(())
    }

    pub(crate) fn persist_branch_record(
        &mut self,
        persistence: &mut StoragePersistence,
        record: BranchCatalogRecord,
    ) -> EngineResult<()> {
        if record.is_active() {
            super::space::seed_required_space_rows(persistence, &record)?;
        }

        let mut names: Vec<_> = self.branches.keys().cloned().collect();
        names.push(record.name().clone());
        names.sort();
        names.dedup();

        let mutations = vec![
            RowMutation::put(
                control_address(
                    RowClass::BranchControl,
                    branch_catalog_key(record.name().as_str()),
                ),
                encode_branch_record(&record),
            ),
            RowMutation::put(
                control_address(RowClass::BranchControl, branch_index_key()),
                encode_branch_index(&names)?,
            ),
            RowMutation::put(
                control_address(RowClass::BranchControl, branch_pending_index_key()),
                encode_pending_branch_index(&[])?,
            ),
            RowMutation::delete(control_address(
                RowClass::BranchControl,
                branch_pending_key(record.name().as_str()),
            )),
        ];
        persistence.commit(&CommitPlan::new(SYSTEM_BRANCH_ID, mutations, None))?;
        self.insert_branch(record);
        Ok(())
    }

    pub(crate) fn space_registration_mutations(
        persistence: &mut StoragePersistence,
        record: &BranchCatalogRecord,
        space: &crate::data::kv::ProductSpace,
    ) -> EngineResult<Vec<RowMutation>> {
        super::space::registration_mutations(persistence, record, space)
    }
}

pub(crate) fn bootstrap_or_load(
    persistence: &mut StoragePersistence,
    created: bool,
    requested_default: Option<BranchName>,
) -> EngineResult<ControlPlane> {
    if created {
        bootstrap_new_database(
            persistence,
            requested_default.unwrap_or_else(BranchName::default_branch),
        )
    } else {
        load_existing_database(persistence, requested_default.as_ref())
    }
}

fn bootstrap_new_database(
    persistence: &mut StoragePersistence,
    default_branch: BranchName,
) -> EngineResult<ControlPlane> {
    persistence.create_system_branch_for_new_database()?;

    let default_record = if default_branch == BranchName::default_branch() {
        BranchCatalogRecord::default_record()
    } else {
        BranchCatalogRecord::root(default_branch, DEFAULT_BRANCH_GENERATION)
    };
    persistence.ensure_branch_created(
        default_record.storage_branch_id(),
        DEFAULT_BRANCH_GENERATION,
    )?;
    let names = [default_record.name().clone()];
    let mutations = vec![
        RowMutation::put(
            control_address(RowClass::DatasetIdentity, database_identity_key()),
            encode_database_identity(&DatabaseIdentityRecord::current()),
        ),
        RowMutation::put(
            control_address(RowClass::DatasetIdentity, local_instance_identity_key()),
            encode_local_instance_identity(&DatabaseIdentityRecord::current()),
        ),
        RowMutation::put(
            control_address(RowClass::Registry, storage_registry_key()),
            encode_storage_registry(),
        ),
        RowMutation::put(
            control_address(RowClass::Registry, capability_registry_key()),
            encode_capability_registry(),
        ),
        RowMutation::put(
            control_address(RowClass::Registry, migration_registry_key()),
            encode_migration_registry(),
        ),
        RowMutation::put(
            control_address(RowClass::BranchControl, branch_index_key()),
            encode_branch_index(&names)?,
        ),
        RowMutation::put(
            control_address(RowClass::BranchControl, branch_default_key()),
            encode_default_branch(default_record.name()),
        ),
        RowMutation::put(
            control_address(RowClass::BranchControl, branch_pending_index_key()),
            encode_pending_branch_index(&[])?,
        ),
        RowMutation::put(
            control_address(
                RowClass::BranchControl,
                branch_catalog_key(default_record.name().as_str()),
            ),
            encode_branch_record(&default_record),
        ),
    ];
    super::space::seed_required_space_rows(persistence, &default_record)?;
    persistence.commit(&CommitPlan::new(SYSTEM_BRANCH_ID, mutations, None))?;

    Ok(ControlPlane {
        default_branch: default_record.name().clone(),
        branches: BTreeMap::from([(default_record.name().clone(), default_record)]),
        terminal_error: None,
    })
}

fn load_existing_database(
    persistence: &mut StoragePersistence,
    requested_default: Option<&BranchName>,
) -> EngineResult<ControlPlane> {
    let identity = read_required(
        persistence,
        RowClass::DatasetIdentity,
        database_identity_key(),
    )?;
    decode_database_identity(&identity)?;

    let local_identity = read_required(
        persistence,
        RowClass::DatasetIdentity,
        local_instance_identity_key(),
    )?;
    decode_local_instance_identity(&local_identity)?;

    let registry = read_required(persistence, RowClass::Registry, storage_registry_key())?;
    decode_storage_registry(&registry)?;

    let capabilities = read_required(persistence, RowClass::Registry, capability_registry_key())?;
    decode_capability_registry(&capabilities)?;

    let migrations = read_required(persistence, RowClass::Registry, migration_registry_key())?;
    decode_migration_registry(&migrations)?;

    let default_row = read_required(persistence, RowClass::BranchControl, branch_default_key())?;
    let default_branch = decode_default_branch(&default_row)?;
    if requested_default.is_some_and(|requested| requested != &default_branch) {
        return Err(EngineError::incompatible_layout(
            "failed_precondition.engine.default_branch",
            "requested default branch does not match the persisted database default",
        ));
    }

    let pending = read_required(
        persistence,
        RowClass::BranchControl,
        branch_pending_index_key(),
    )?;
    let pending_names = decode_pending_branch_index(&pending)?;
    if !pending_names.is_empty() {
        if let Some(name) = pending_names.first() {
            let row = read_required(
                persistence,
                RowClass::BranchControl,
                branch_pending_key(name.as_str()),
            )?;
            let _ = decode_pending_branch_record(&row)?;
        }
        return Err(EngineError::corruption(
            "data_loss.engine.branch_create_pending",
            "branch catalog contains an unfinished branch create operation",
        ));
    }

    let branch_index = read_required(persistence, RowClass::BranchControl, branch_index_key())?;
    let branch_names = decode_branch_index(&branch_index)?;
    if branch_names.is_empty() {
        return Err(EngineError::corruption(
            "data_loss.engine.branch_catalog",
            "branch catalog index is empty",
        ));
    }

    let mut branches = BTreeMap::new();
    for name in branch_names {
        if branches.contains_key(&name) {
            return Err(EngineError::corruption(
                "data_loss.engine.branch_catalog",
                "branch catalog index contains a duplicate branch name",
            ));
        }
        let row = read_required(
            persistence,
            RowClass::BranchControl,
            branch_catalog_key(name.as_str()),
        )?;
        let record = decode_branch_record(&row)?;
        if record.name() != &name {
            return Err(EngineError::corruption(
                "data_loss.engine.branch_catalog",
                "branch catalog row name does not match its index entry",
            ));
        }
        if record.is_active() && !persistence.branch_exists(record.storage_branch_id())? {
            return Err(EngineError::corruption(
                "data_loss.engine.branch_catalog",
                "branch catalog references a missing storage branch",
            ));
        }
        if record.is_active() {
            super::space::validate_required_space_rows(persistence, &record)?;
        }
        branches.insert(record.name().clone(), record);
    }

    if !branches
        .get(&default_branch)
        .is_some_and(BranchCatalogRecord::is_active)
    {
        return Err(EngineError::corruption(
            "data_loss.engine.branch_catalog",
            "branch catalog is missing the default branch",
        ));
    }

    Ok(ControlPlane {
        default_branch,
        branches,
        terminal_error: None,
    })
}

fn read_required(
    persistence: &mut StoragePersistence,
    row_class: RowClass,
    key: Vec<u8>,
) -> EngineResult<Vec<u8>> {
    match persistence.read(&control_address(row_class, key), ReadSelector::Latest) {
        Ok(Some(value)) => Ok(value),
        Ok(None) => Err(EngineError::corruption(
            "data_loss.engine.control_plane_missing",
            "required control-plane row is missing",
        )),
        Err(error) if error.class() == EngineErrorClass::NotFound => Err(EngineError::corruption(
            "data_loss.engine.control_plane_missing",
            "required control-plane storage branch is missing",
        )),
        Err(error) => Err(error),
    }
}

fn control_address(row_class: RowClass, key: Vec<u8>) -> RowAddress {
    RowAddress::new(SYSTEM_BRANCH_ID, row_class, key)
}

#[cfg(test)]
mod tests {
    use super::{bootstrap_new_database, control_address, load_existing_database, ControlPlane};
    use crate::api::ControlHealthStatus;
    use crate::branch::catalog::{BranchCatalogRecord, SYSTEM_BRANCH_ID};
    use crate::branch::BranchName;
    use crate::control::records::{
        encode_branch_index, encode_branch_record, encode_default_branch,
        encode_migration_registry, encode_reserved_system_space,
    };
    use crate::diagnostics::EngineErrorClass;
    use crate::persistence::{
        branch_catalog_key, branch_default_key, branch_index_key, capability_registry_key,
        database_identity_key, local_instance_identity_key, migration_registry_key,
        reserved_space_key, storage_registry_key, CommitPlan, PersistenceOpenTarget, RowClass,
        RowMutation, StoragePersistence,
    };

    #[test]
    fn pending_branch_create_fails_closed_on_load() {
        let (mut persistence, _) =
            StoragePersistence::open(PersistenceOpenTarget::Cache).expect("cache opens");
        bootstrap_default(&mut persistence);
        let record =
            BranchCatalogRecord::root(BranchName::new("feature").expect("valid branch"), 1);

        ControlPlane::begin_branch_operation(&mut persistence, &record)
            .expect("pending row writes");

        let error =
            load_existing_database(&mut persistence, None).expect_err("pending row fails load");
        assert_eq!(error.class(), EngineErrorClass::Corruption);
        assert_eq!(error.code(), "data_loss.engine.branch_create_pending");
    }

    #[test]
    fn missing_database_identity_fails_closed_on_load() {
        let (mut persistence, _) =
            StoragePersistence::open(PersistenceOpenTarget::Cache).expect("cache opens");
        bootstrap_default(&mut persistence);
        delete_control_row(
            &mut persistence,
            RowClass::DatasetIdentity,
            database_identity_key(),
        );

        let error =
            load_existing_database(&mut persistence, None).expect_err("missing identity fails");
        assert_eq!(error.class(), EngineErrorClass::Corruption);
        assert_eq!(error.code(), "data_loss.engine.control_plane_missing");
    }

    #[test]
    fn corrupt_database_identity_fails_closed_on_load() {
        let (mut persistence, _) =
            StoragePersistence::open(PersistenceOpenTarget::Cache).expect("cache opens");
        bootstrap_default(&mut persistence);
        put_control_row(
            &mut persistence,
            RowClass::DatasetIdentity,
            database_identity_key(),
            vec![0xff],
        );

        let error =
            load_existing_database(&mut persistence, None).expect_err("corrupt identity fails");
        assert_eq!(error.class(), EngineErrorClass::Corruption);
        assert_eq!(error.code(), "data_loss.engine.control_plane");
    }

    #[test]
    fn missing_local_instance_identity_fails_closed_on_load() {
        let (mut persistence, _) =
            StoragePersistence::open(PersistenceOpenTarget::Cache).expect("cache opens");
        bootstrap_default(&mut persistence);
        delete_control_row(
            &mut persistence,
            RowClass::DatasetIdentity,
            local_instance_identity_key(),
        );

        let error = load_existing_database(&mut persistence, None)
            .expect_err("missing local identity fails");
        assert_eq!(error.class(), EngineErrorClass::Corruption);
        assert_eq!(error.code(), "data_loss.engine.control_plane_missing");
    }

    #[test]
    fn corrupt_local_instance_identity_fails_closed_on_load() {
        let (mut persistence, _) =
            StoragePersistence::open(PersistenceOpenTarget::Cache).expect("cache opens");
        bootstrap_default(&mut persistence);
        put_control_row(
            &mut persistence,
            RowClass::DatasetIdentity,
            local_instance_identity_key(),
            vec![0xff],
        );

        let error = load_existing_database(&mut persistence, None)
            .expect_err("corrupt local identity fails");
        assert_eq!(error.class(), EngineErrorClass::Corruption);
        assert_eq!(error.code(), "data_loss.engine.control_plane");
    }

    #[test]
    fn missing_storage_registry_fails_closed_on_load() {
        let (mut persistence, _) =
            StoragePersistence::open(PersistenceOpenTarget::Cache).expect("cache opens");
        bootstrap_default(&mut persistence);
        delete_control_row(&mut persistence, RowClass::Registry, storage_registry_key());

        let error =
            load_existing_database(&mut persistence, None).expect_err("missing registry fails");
        assert_eq!(error.class(), EngineErrorClass::Corruption);
        assert_eq!(error.code(), "data_loss.engine.control_plane_missing");
    }

    #[test]
    fn corrupt_storage_registry_fails_closed_on_load() {
        let (mut persistence, _) =
            StoragePersistence::open(PersistenceOpenTarget::Cache).expect("cache opens");
        bootstrap_default(&mut persistence);
        put_control_row(
            &mut persistence,
            RowClass::Registry,
            storage_registry_key(),
            vec![0xff],
        );

        let error =
            load_existing_database(&mut persistence, None).expect_err("corrupt registry fails");
        assert_eq!(error.class(), EngineErrorClass::Corruption);
        assert_eq!(error.code(), "data_loss.engine.control_plane");
    }

    #[test]
    fn missing_capability_registry_fails_closed_on_load() {
        let (mut persistence, _) =
            StoragePersistence::open(PersistenceOpenTarget::Cache).expect("cache opens");
        bootstrap_default(&mut persistence);
        delete_control_row(
            &mut persistence,
            RowClass::Registry,
            capability_registry_key(),
        );

        let error = load_existing_database(&mut persistence, None)
            .expect_err("missing capability registry fails");
        assert_eq!(error.class(), EngineErrorClass::Corruption);
        assert_eq!(error.code(), "data_loss.engine.control_plane_missing");
    }

    #[test]
    fn missing_migration_registry_fails_closed_on_load() {
        let (mut persistence, _) =
            StoragePersistence::open(PersistenceOpenTarget::Cache).expect("cache opens");
        bootstrap_default(&mut persistence);
        delete_control_row(
            &mut persistence,
            RowClass::Registry,
            migration_registry_key(),
        );

        let error = load_existing_database(&mut persistence, None)
            .expect_err("missing migration registry fails");
        assert_eq!(error.class(), EngineErrorClass::Corruption);
        assert_eq!(error.code(), "data_loss.engine.control_plane_missing");
    }

    #[test]
    fn unsupported_migration_registry_fails_closed_on_load() {
        let (mut persistence, _) =
            StoragePersistence::open(PersistenceOpenTarget::Cache).expect("cache opens");
        bootstrap_default(&mut persistence);
        let mut migration = encode_migration_registry();
        let offset = b"strata.engine.migrations".len() + 2;
        migration[offset..offset + 2].copy_from_slice(&2_u16.to_be_bytes());
        put_control_row(
            &mut persistence,
            RowClass::Registry,
            migration_registry_key(),
            migration,
        );

        let error = load_existing_database(&mut persistence, None)
            .expect_err("unsupported migration registry fails");
        assert_eq!(error.class(), EngineErrorClass::IncompatibleLayout);
        assert_eq!(
            error.code(),
            "failed_precondition.engine.migration_registry"
        );
    }

    #[test]
    fn missing_default_branch_row_fails_closed_on_load() {
        let (mut persistence, _) =
            StoragePersistence::open(PersistenceOpenTarget::Cache).expect("cache opens");
        bootstrap_default(&mut persistence);
        delete_control_row(
            &mut persistence,
            RowClass::BranchControl,
            branch_default_key(),
        );

        let error = load_existing_database(&mut persistence, None)
            .expect_err("missing default branch row fails");
        assert_eq!(error.class(), EngineErrorClass::Corruption);
        assert_eq!(error.code(), "data_loss.engine.control_plane_missing");
    }

    #[test]
    fn missing_default_branch_catalog_record_fails_closed_on_load() {
        let (mut persistence, _) =
            StoragePersistence::open(PersistenceOpenTarget::Cache).expect("cache opens");
        bootstrap_default(&mut persistence);
        delete_control_row(
            &mut persistence,
            RowClass::BranchControl,
            branch_catalog_key(BranchName::default_branch().as_str()),
        );

        let error = load_existing_database(&mut persistence, None)
            .expect_err("missing default branch catalog fails");
        assert_eq!(error.class(), EngineErrorClass::Corruption);
        assert_eq!(error.code(), "data_loss.engine.control_plane_missing");
    }

    #[test]
    fn default_branch_missing_from_branch_index_fails_closed_on_load() {
        let (mut persistence, _) =
            StoragePersistence::open(PersistenceOpenTarget::Cache).expect("cache opens");
        bootstrap_default(&mut persistence);
        let feature = BranchName::new("feature").expect("valid branch");
        put_control_row(
            &mut persistence,
            RowClass::BranchControl,
            branch_default_key(),
            encode_default_branch(&feature),
        );

        let error = load_existing_database(&mut persistence, None)
            .expect_err("missing default branch catalog entry fails");
        assert_eq!(error.class(), EngineErrorClass::Corruption);
        assert_eq!(error.code(), "data_loss.engine.branch_catalog");
    }

    #[test]
    fn diagnostics_report_corrupt_space_catalog_without_exposing_rows() {
        let (mut persistence, _) =
            StoragePersistence::open(PersistenceOpenTarget::Cache).expect("cache opens");
        let control = bootstrap_new_database(&mut persistence, BranchName::default_branch())
            .expect("bootstrap succeeds");
        let mut reserved = encode_reserved_system_space();
        *reserved.last_mut().expect("reserved flag exists") = 1;
        let default_record = control
            .lookup_branch(&BranchName::default_branch())
            .expect("default branch loaded")
            .clone();
        persistence
            .commit(&CommitPlan::new(
                default_record.storage_branch_id(),
                vec![RowMutation::put(
                    crate::persistence::RowAddress::new(
                        default_record.storage_branch_id(),
                        RowClass::SpaceControl,
                        reserved_space_key(crate::control::space::SYSTEM_SPACE),
                    ),
                    reserved,
                )],
                Some(default_record.generation()),
            ))
            .expect("reserved space row corrupts");

        let diagnostics =
            control.diagnostics(&mut persistence, Some(&BranchName::default_branch()));
        let space = diagnostics
            .space_catalog()
            .expect("space diagnostics present");
        assert_eq!(space.status(), ControlHealthStatus::Corrupt);
        assert_eq!(space.space_count(), None);
    }

    #[test]
    fn corrupt_branch_catalog_row_fails_closed_on_load() {
        let (mut persistence, _) =
            StoragePersistence::open(PersistenceOpenTarget::Cache).expect("cache opens");
        bootstrap_default(&mut persistence);
        persistence
            .commit(&CommitPlan::new(
                SYSTEM_BRANCH_ID,
                vec![RowMutation::put(
                    control_address(
                        RowClass::BranchControl,
                        branch_catalog_key(BranchName::default_branch().as_str()),
                    ),
                    vec![0xff],
                )],
                None,
            ))
            .expect("corrupt catalog row writes");

        let error =
            load_existing_database(&mut persistence, None).expect_err("corrupt catalog row fails");
        assert_eq!(error.class(), EngineErrorClass::Corruption);
        assert_eq!(error.code(), "data_loss.engine.control_plane");
    }

    #[test]
    fn duplicate_branch_index_entries_fail_closed_on_load() {
        let (mut persistence, _) =
            StoragePersistence::open(PersistenceOpenTarget::Cache).expect("cache opens");
        bootstrap_default(&mut persistence);
        let default = BranchName::new("default").expect("valid branch");
        let names = [default.clone(), default];
        persistence
            .commit(&CommitPlan::new(
                SYSTEM_BRANCH_ID,
                vec![RowMutation::put(
                    control_address(RowClass::BranchControl, branch_index_key()),
                    encode_branch_index(&names).expect("index encodes"),
                )],
                None,
            ))
            .expect("corrupt index writes");

        let error =
            load_existing_database(&mut persistence, None).expect_err("duplicate index fails");
        assert_eq!(error.class(), EngineErrorClass::Corruption);
        assert_eq!(error.code(), "data_loss.engine.branch_catalog");
    }

    #[test]
    fn branch_index_record_name_mismatch_fails_closed_on_load() {
        let (mut persistence, _) =
            StoragePersistence::open(PersistenceOpenTarget::Cache).expect("cache opens");
        bootstrap_default(&mut persistence);
        let default = BranchCatalogRecord::default_record();
        let feature = BranchName::new("feature").expect("valid branch");
        let names = [default.name().clone(), feature.clone()];
        persistence
            .commit(&CommitPlan::new(
                SYSTEM_BRANCH_ID,
                vec![
                    RowMutation::put(
                        control_address(RowClass::BranchControl, branch_index_key()),
                        encode_branch_index(&names).expect("index encodes"),
                    ),
                    RowMutation::put(
                        control_address(
                            RowClass::BranchControl,
                            branch_catalog_key(feature.as_str()),
                        ),
                        encode_branch_record(&default),
                    ),
                ],
                None,
            ))
            .expect("corrupt catalog writes");

        let error = load_existing_database(&mut persistence, None).expect_err("mismatch fails");
        assert_eq!(error.class(), EngineErrorClass::Corruption);
        assert_eq!(error.code(), "data_loss.engine.branch_catalog");
    }

    fn bootstrap_default(persistence: &mut StoragePersistence) {
        bootstrap_new_database(persistence, BranchName::default_branch())
            .expect("bootstrap succeeds");
    }

    fn delete_control_row(persistence: &mut StoragePersistence, row_class: RowClass, key: Vec<u8>) {
        persistence
            .commit(&CommitPlan::new(
                SYSTEM_BRANCH_ID,
                vec![RowMutation::delete(control_address(row_class, key))],
                None,
            ))
            .expect("control row delete writes");
    }

    fn put_control_row(
        persistence: &mut StoragePersistence,
        row_class: RowClass,
        key: Vec<u8>,
        value: Vec<u8>,
    ) {
        persistence
            .commit(&CommitPlan::new(
                SYSTEM_BRANCH_ID,
                vec![RowMutation::put(control_address(row_class, key), value)],
                None,
            ))
            .expect("control row put writes");
    }
}
