//! Control-plane bootstrap and load.

use std::collections::BTreeMap;

use crate::branch::catalog::{
    BranchCatalogRecord, DEFAULT_BRANCH_GENERATION, DEFAULT_BRANCH_ID, SYSTEM_BRANCH_ID,
};
use crate::branch::BranchName;
use crate::diagnostics::{EngineError, EngineErrorClass, EngineResult};
use crate::persistence::{
    branch_catalog_key, branch_index_key, branch_pending_index_key, branch_pending_key,
    capability_registry_key, database_identity_key, storage_registry_key, CommitPlan, ReadSelector,
    RowAddress, RowClass, RowMutation, StoragePersistence,
};

use super::records::{
    decode_branch_index, decode_branch_record, decode_capability_registry,
    decode_database_identity, decode_pending_branch_index, decode_pending_branch_record,
    decode_storage_registry, encode_branch_index, encode_branch_record, encode_capability_registry,
    encode_database_identity, encode_pending_branch_index, encode_pending_branch_record,
    encode_storage_registry, DatabaseIdentityRecord,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ControlPlane {
    branches: BTreeMap<BranchName, BranchCatalogRecord>,
}

impl ControlPlane {
    pub(crate) fn list_branches(&self) -> Vec<BranchCatalogRecord> {
        self.branches.values().cloned().collect()
    }

    pub(crate) fn lookup_branch(&self, name: &BranchName) -> Option<&BranchCatalogRecord> {
        self.branches.get(name)
    }

    pub(crate) fn contains_branch(&self, name: &BranchName) -> bool {
        self.branches.contains_key(name)
    }

    pub(crate) fn insert_branch(&mut self, record: BranchCatalogRecord) {
        self.branches.insert(record.name().clone(), record);
    }

    pub(crate) fn begin_branch_create(
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

    pub(crate) fn clear_pending_branch_create(
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

    pub(crate) fn activate_branch(
        &mut self,
        persistence: &mut StoragePersistence,
        record: BranchCatalogRecord,
    ) -> EngineResult<()> {
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
}

pub(crate) fn bootstrap_or_load(
    persistence: &mut StoragePersistence,
    created: bool,
) -> EngineResult<ControlPlane> {
    if created {
        bootstrap_new_database(persistence)
    } else {
        load_existing_database(persistence)
    }
}

fn bootstrap_new_database(persistence: &mut StoragePersistence) -> EngineResult<ControlPlane> {
    persistence.create_system_branch_for_new_database()?;
    persistence.ensure_branch_created(DEFAULT_BRANCH_ID, DEFAULT_BRANCH_GENERATION)?;

    let default_record = BranchCatalogRecord::default_record();
    let names = [default_record.name().clone()];
    let mutations = vec![
        RowMutation::put(
            control_address(RowClass::DatasetIdentity, database_identity_key()),
            encode_database_identity(&DatabaseIdentityRecord::current()),
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
            control_address(RowClass::BranchControl, branch_index_key()),
            encode_branch_index(&names)?,
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
    persistence.commit(&CommitPlan::new(SYSTEM_BRANCH_ID, mutations, None))?;

    Ok(ControlPlane {
        branches: BTreeMap::from([(default_record.name().clone(), default_record)]),
    })
}

fn load_existing_database(persistence: &mut StoragePersistence) -> EngineResult<ControlPlane> {
    let identity = read_required(
        persistence,
        RowClass::DatasetIdentity,
        database_identity_key(),
    )?;
    decode_database_identity(&identity)?;

    let registry = read_required(persistence, RowClass::Registry, storage_registry_key())?;
    decode_storage_registry(&registry)?;

    let capabilities = read_required(persistence, RowClass::Registry, capability_registry_key())?;
    decode_capability_registry(&capabilities)?;

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
        if !persistence.branch_exists(record.branch_id())? {
            return Err(EngineError::corruption(
                "data_loss.engine.branch_catalog",
                "branch catalog references a missing storage branch",
            ));
        }
        branches.insert(record.name().clone(), record);
    }

    if !branches.contains_key(&BranchName::default_branch()) {
        return Err(EngineError::corruption(
            "data_loss.engine.branch_catalog",
            "branch catalog is missing the default branch",
        ));
    }

    Ok(ControlPlane { branches })
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
    use crate::branch::catalog::{BranchCatalogRecord, DEFAULT_BRANCH_ID, SYSTEM_BRANCH_ID};
    use crate::branch::BranchName;
    use crate::control::records::{encode_branch_index, encode_branch_record};
    use crate::diagnostics::EngineErrorClass;
    use crate::persistence::{
        branch_catalog_key, branch_index_key, database_identity_key, storage_registry_key,
        CommitPlan, PersistenceOpenTarget, RowClass, RowMutation, StoragePersistence,
    };

    #[test]
    fn pending_branch_create_fails_closed_on_load() {
        let (mut persistence, _) =
            StoragePersistence::open(PersistenceOpenTarget::Cache).expect("cache opens");
        bootstrap_new_database(&mut persistence).expect("bootstrap succeeds");
        let record = BranchCatalogRecord::derived(
            BranchName::new("feature").expect("valid branch"),
            DEFAULT_BRANCH_ID,
        );

        ControlPlane::begin_branch_create(&mut persistence, &record).expect("pending row writes");

        let error = load_existing_database(&mut persistence).expect_err("pending row fails load");
        assert_eq!(error.class(), EngineErrorClass::Corruption);
        assert_eq!(error.code(), "data_loss.engine.branch_create_pending");
    }

    #[test]
    fn missing_database_identity_fails_closed_on_load() {
        let (mut persistence, _) =
            StoragePersistence::open(PersistenceOpenTarget::Cache).expect("cache opens");
        bootstrap_new_database(&mut persistence).expect("bootstrap succeeds");
        persistence
            .commit(&CommitPlan::new(
                SYSTEM_BRANCH_ID,
                vec![RowMutation::delete(control_address(
                    RowClass::DatasetIdentity,
                    database_identity_key(),
                ))],
                None,
            ))
            .expect("identity delete writes");

        let error = load_existing_database(&mut persistence).expect_err("missing identity fails");
        assert_eq!(error.class(), EngineErrorClass::Corruption);
        assert_eq!(error.code(), "data_loss.engine.control_plane_missing");
    }

    #[test]
    fn missing_storage_registry_fails_closed_on_load() {
        let (mut persistence, _) =
            StoragePersistence::open(PersistenceOpenTarget::Cache).expect("cache opens");
        bootstrap_new_database(&mut persistence).expect("bootstrap succeeds");
        persistence
            .commit(&CommitPlan::new(
                SYSTEM_BRANCH_ID,
                vec![RowMutation::delete(control_address(
                    RowClass::Registry,
                    storage_registry_key(),
                ))],
                None,
            ))
            .expect("registry delete writes");

        let error = load_existing_database(&mut persistence).expect_err("missing registry fails");
        assert_eq!(error.class(), EngineErrorClass::Corruption);
        assert_eq!(error.code(), "data_loss.engine.control_plane_missing");
    }

    #[test]
    fn corrupt_branch_catalog_row_fails_closed_on_load() {
        let (mut persistence, _) =
            StoragePersistence::open(PersistenceOpenTarget::Cache).expect("cache opens");
        bootstrap_new_database(&mut persistence).expect("bootstrap succeeds");
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
            load_existing_database(&mut persistence).expect_err("corrupt catalog row fails");
        assert_eq!(error.class(), EngineErrorClass::Corruption);
        assert_eq!(error.code(), "data_loss.engine.control_plane");
    }

    #[test]
    fn duplicate_branch_index_entries_fail_closed_on_load() {
        let (mut persistence, _) =
            StoragePersistence::open(PersistenceOpenTarget::Cache).expect("cache opens");
        bootstrap_new_database(&mut persistence).expect("bootstrap succeeds");
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

        let error = load_existing_database(&mut persistence).expect_err("duplicate index fails");
        assert_eq!(error.class(), EngineErrorClass::Corruption);
        assert_eq!(error.code(), "data_loss.engine.branch_catalog");
    }

    #[test]
    fn branch_index_record_name_mismatch_fails_closed_on_load() {
        let (mut persistence, _) =
            StoragePersistence::open(PersistenceOpenTarget::Cache).expect("cache opens");
        bootstrap_new_database(&mut persistence).expect("bootstrap succeeds");
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

        let error = load_existing_database(&mut persistence).expect_err("mismatch fails");
        assert_eq!(error.class(), EngineErrorClass::Corruption);
        assert_eq!(error.code(), "data_loss.engine.branch_catalog");
    }
}
