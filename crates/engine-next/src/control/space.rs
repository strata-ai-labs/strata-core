//! Branch-local core space control rows.

use crate::branch::catalog::BranchCatalogRecord;
use crate::data::kv::ProductSpace;
use crate::diagnostics::{EngineError, EngineErrorClass, EngineResult};
use crate::persistence::{
    reserved_space_key, space_catalog_key, space_index_key, CommitPlan, ReadSelector, RowAddress,
    RowClass, RowMutation, StoragePersistence,
};

use super::records::{
    decode_reserved_system_space, decode_space_index, decode_space_record,
    encode_reserved_system_space, encode_space_index, encode_space_record,
};

pub(crate) const DEFAULT_SPACE: &str = "default";
pub(crate) const SYSTEM_SPACE: &str = "_system_";

pub(crate) fn seed_required_space_rows(
    persistence: &mut StoragePersistence,
    record: &BranchCatalogRecord,
) -> EngineResult<()> {
    let mutations = required_space_mutations(persistence, record)?;
    if mutations.is_empty() {
        return Ok(());
    }
    persistence.commit(&CommitPlan::new(
        record.storage_branch_id(),
        mutations,
        Some(record.generation()),
    ))?;
    Ok(())
}

pub(crate) fn registration_mutations(
    persistence: &mut StoragePersistence,
    record: &BranchCatalogRecord,
    space: &ProductSpace,
) -> EngineResult<Vec<RowMutation>> {
    let mut spaces = read_required_space_index(persistence, record)?;
    if spaces.iter().any(|existing| existing == space) {
        validate_space_catalog_row(persistence, record, space)?;
        return Ok(Vec::new());
    }

    spaces.push(space.clone());
    spaces.sort();
    spaces.dedup();
    Ok(vec![
        RowMutation::put(
            space_address(record, space_index_key()),
            encode_space_index(&spaces)?,
        ),
        RowMutation::put(
            space_address(record, space_catalog_key(space.as_str())),
            encode_space_record(space),
        ),
    ])
}

pub(crate) fn validate_required_space_rows(
    persistence: &mut StoragePersistence,
    record: &BranchCatalogRecord,
) -> EngineResult<()> {
    validate_required_space_rows_and_count(persistence, record).map(|_| ())
}

pub(crate) fn validate_required_space_rows_and_count(
    persistence: &mut StoragePersistence,
    record: &BranchCatalogRecord,
) -> EngineResult<usize> {
    let spaces = read_required_space_index(persistence, record)?;
    let default = default_space()?;
    if !spaces.iter().any(|space| space == &default) {
        return Err(EngineError::corruption(
            "data_loss.engine.space_catalog",
            "space catalog is missing the default space",
        ));
    }
    for space in &spaces {
        validate_space_catalog_row(persistence, record, space)?;
    }
    let reserved = read_required(
        persistence,
        &space_address(record, reserved_space_key(SYSTEM_SPACE)),
    )?;
    decode_reserved_system_space(&reserved)?;
    Ok(spaces.len())
}

fn required_space_mutations(
    persistence: &mut StoragePersistence,
    record: &BranchCatalogRecord,
) -> EngineResult<Vec<RowMutation>> {
    let default = default_space()?;
    let mut spaces = match persistence.read(
        &space_address(record, space_index_key()),
        ReadSelector::Latest,
    )? {
        Some(bytes) => decode_space_index(&bytes)?,
        None => Vec::new(),
    };
    if !spaces.iter().any(|space| space == &default) {
        spaces.push(default.clone());
    }
    spaces.sort();
    spaces.dedup();
    let mut mutations = Vec::new();
    mutations.push(RowMutation::put(
        space_address(record, space_index_key()),
        encode_space_index(&spaces)?,
    ));
    mutations.push(RowMutation::put(
        space_address(record, space_catalog_key(default.as_str())),
        encode_space_record(&default),
    ));
    mutations.push(RowMutation::put(
        space_address(record, reserved_space_key(SYSTEM_SPACE)),
        encode_reserved_system_space(),
    ));
    Ok(mutations)
}

fn read_required_space_index(
    persistence: &mut StoragePersistence,
    record: &BranchCatalogRecord,
) -> EngineResult<Vec<ProductSpace>> {
    let bytes = read_required(persistence, &space_address(record, space_index_key()))?;
    decode_space_index(&bytes)
}

fn validate_space_catalog_row(
    persistence: &mut StoragePersistence,
    record: &BranchCatalogRecord,
    space: &ProductSpace,
) -> EngineResult<()> {
    let bytes = read_required(
        persistence,
        &space_address(record, space_catalog_key(space.as_str())),
    )?;
    let decoded = decode_space_record(&bytes)?;
    if &decoded != space {
        return Err(EngineError::corruption(
            "data_loss.engine.space_catalog",
            "space catalog row name does not match its index entry",
        ));
    }
    Ok(())
}

fn read_required(
    persistence: &mut StoragePersistence,
    address: &RowAddress,
) -> EngineResult<Vec<u8>> {
    match persistence.read(address, ReadSelector::Latest) {
        Ok(Some(value)) => Ok(value),
        Ok(None) => Err(EngineError::corruption(
            "data_loss.engine.space_catalog",
            "required branch-local space control row is missing",
        )),
        Err(error) if error.class() == EngineErrorClass::NotFound => Err(EngineError::corruption(
            "data_loss.engine.space_catalog",
            "branch-local space control storage branch is missing",
        )),
        Err(error) => Err(error),
    }
}

fn default_space() -> EngineResult<ProductSpace> {
    ProductSpace::new(DEFAULT_SPACE)
}

fn space_address(record: &BranchCatalogRecord, key: Vec<u8>) -> RowAddress {
    RowAddress::new(record.storage_branch_id(), RowClass::SpaceControl, key)
}

#[cfg(test)]
mod tests {
    use super::{
        registration_mutations, seed_required_space_rows, validate_required_space_rows,
        DEFAULT_SPACE, SYSTEM_SPACE,
    };
    use crate::branch::catalog::BranchCatalogRecord;
    use crate::branch::BranchName;
    use crate::control::records::{
        encode_reserved_system_space, encode_space_index, encode_space_record,
    };
    use crate::data::kv::ProductSpace;
    use crate::diagnostics::EngineErrorClass;
    use crate::persistence::{
        reserved_space_key, space_catalog_key, space_index_key, CommitPlan, PersistenceOpenTarget,
        RowAddress, RowClass, RowMutation, StoragePersistence,
    };

    #[test]
    fn required_space_rows_seed_and_validate() {
        let (mut persistence, _) =
            StoragePersistence::open(PersistenceOpenTarget::Cache).expect("cache opens");
        let record = create_branch(&mut persistence, "space-test");

        seed_required_space_rows(&mut persistence, &record).expect("space rows seed");
        validate_required_space_rows(&mut persistence, &record).expect("space rows validate");
    }

    #[test]
    fn registration_mutations_add_user_space_once() {
        let (mut persistence, _) =
            StoragePersistence::open(PersistenceOpenTarget::Cache).expect("cache opens");
        let record = create_branch(&mut persistence, "space-test");
        seed_required_space_rows(&mut persistence, &record).expect("space rows seed");
        let tenant = ProductSpace::new("tenant").expect("valid space");

        let mutations = registration_mutations(&mut persistence, &record, &tenant)
            .expect("tenant registration mutations");
        assert_eq!(mutations.len(), 2);

        persistence
            .commit(&crate::persistence::CommitPlan::new(
                record.storage_branch_id(),
                mutations,
                Some(record.generation()),
            ))
            .expect("space registration commits");
        let mutations = registration_mutations(&mut persistence, &record, &tenant)
            .expect("tenant already registered");
        assert!(mutations.is_empty());
    }

    #[test]
    fn missing_required_space_rows_fail_closed() {
        let (mut persistence, _) =
            StoragePersistence::open(PersistenceOpenTarget::Cache).expect("cache opens");
        let record = create_branch(&mut persistence, "space-test");

        let error = validate_required_space_rows(&mut persistence, &record)
            .expect_err("missing space rows fail");
        assert_eq!(error.class(), EngineErrorClass::Corruption);
        assert_eq!(error.code(), "data_loss.engine.space_catalog");
    }

    #[test]
    fn missing_registered_space_catalog_row_fails_closed() {
        let (mut persistence, _) =
            StoragePersistence::open(PersistenceOpenTarget::Cache).expect("cache opens");
        let record = create_branch(&mut persistence, "space-test");
        seed_required_space_rows(&mut persistence, &record).expect("space rows seed");
        let tenant = ProductSpace::new("tenant").expect("valid space");
        let mutations = registration_mutations(&mut persistence, &record, &tenant)
            .expect("tenant registration mutations");
        persistence
            .commit(&CommitPlan::new(
                record.storage_branch_id(),
                mutations,
                Some(record.generation()),
            ))
            .expect("space registration commits");

        persistence
            .commit(&CommitPlan::new(
                record.storage_branch_id(),
                vec![RowMutation::delete(RowAddress::new(
                    record.storage_branch_id(),
                    RowClass::SpaceControl,
                    space_catalog_key(tenant.as_str()),
                ))],
                Some(record.generation()),
            ))
            .expect("space catalog row deletes");

        let error = validate_required_space_rows(&mut persistence, &record)
            .expect_err("missing registered space row fails");
        assert_eq!(error.class(), EngineErrorClass::Corruption);
        assert_eq!(error.code(), "data_loss.engine.space_catalog");
    }

    #[test]
    fn mismatched_registered_space_catalog_row_fails_closed() {
        let (mut persistence, _) =
            StoragePersistence::open(PersistenceOpenTarget::Cache).expect("cache opens");
        let record = create_branch(&mut persistence, "space-test");
        seed_required_space_rows(&mut persistence, &record).expect("space rows seed");
        let tenant = ProductSpace::new("tenant").expect("valid space");
        let other = ProductSpace::new("other").expect("valid space");
        let mutations = registration_mutations(&mut persistence, &record, &tenant)
            .expect("tenant registration mutations");
        persistence
            .commit(&CommitPlan::new(
                record.storage_branch_id(),
                mutations,
                Some(record.generation()),
            ))
            .expect("space registration commits");

        persistence
            .commit(&CommitPlan::new(
                record.storage_branch_id(),
                vec![RowMutation::put(
                    RowAddress::new(
                        record.storage_branch_id(),
                        RowClass::SpaceControl,
                        space_catalog_key(tenant.as_str()),
                    ),
                    encode_space_record(&other),
                )],
                Some(record.generation()),
            ))
            .expect("space catalog row corrupts");

        let error = validate_required_space_rows(&mut persistence, &record)
            .expect_err("mismatched registered space row fails");
        assert_eq!(error.class(), EngineErrorClass::Corruption);
        assert_eq!(error.code(), "data_loss.engine.space_catalog");
    }

    #[test]
    fn missing_default_space_index_entry_fails_closed() {
        let (mut persistence, _) =
            StoragePersistence::open(PersistenceOpenTarget::Cache).expect("cache opens");
        let record = create_branch(&mut persistence, "space-test");
        seed_required_space_rows(&mut persistence, &record).expect("space rows seed");

        persistence
            .commit(&CommitPlan::new(
                record.storage_branch_id(),
                vec![RowMutation::put(
                    RowAddress::new(
                        record.storage_branch_id(),
                        RowClass::SpaceControl,
                        space_index_key(),
                    ),
                    encode_space_index(&[]).expect("empty index encodes"),
                )],
                Some(record.generation()),
            ))
            .expect("space index corrupts");

        let error = validate_required_space_rows(&mut persistence, &record)
            .expect_err("missing default space fails");
        assert_eq!(error.class(), EngineErrorClass::Corruption);
        assert_eq!(error.code(), "data_loss.engine.space_catalog");
    }

    #[test]
    fn missing_reserved_system_space_fact_fails_closed() {
        let (mut persistence, _) =
            StoragePersistence::open(PersistenceOpenTarget::Cache).expect("cache opens");
        let record = create_branch(&mut persistence, "space-test");
        seed_required_space_rows(&mut persistence, &record).expect("space rows seed");

        persistence
            .commit(&CommitPlan::new(
                record.storage_branch_id(),
                vec![RowMutation::delete(RowAddress::new(
                    record.storage_branch_id(),
                    RowClass::SpaceControl,
                    reserved_space_key(SYSTEM_SPACE),
                ))],
                Some(record.generation()),
            ))
            .expect("reserved space row deletes");

        let error = validate_required_space_rows(&mut persistence, &record)
            .expect_err("missing reserved space fact fails");
        assert_eq!(error.class(), EngineErrorClass::Corruption);
        assert_eq!(error.code(), "data_loss.engine.space_catalog");
    }

    #[test]
    fn user_managed_reserved_system_space_fact_fails_closed() {
        let (mut persistence, _) =
            StoragePersistence::open(PersistenceOpenTarget::Cache).expect("cache opens");
        let record = create_branch(&mut persistence, "space-test");
        seed_required_space_rows(&mut persistence, &record).expect("space rows seed");
        let mut reserved = encode_reserved_system_space();
        *reserved.last_mut().expect("reserved flag exists") = 1;

        persistence
            .commit(&CommitPlan::new(
                record.storage_branch_id(),
                vec![RowMutation::put(
                    RowAddress::new(
                        record.storage_branch_id(),
                        RowClass::SpaceControl,
                        reserved_space_key(SYSTEM_SPACE),
                    ),
                    reserved,
                )],
                Some(record.generation()),
            ))
            .expect("reserved space row corrupts");

        let error = validate_required_space_rows(&mut persistence, &record)
            .expect_err("user-managed reserved space fact fails");
        assert_eq!(error.class(), EngineErrorClass::Corruption);
        assert_eq!(error.code(), "data_loss.engine.space_catalog");
    }

    #[test]
    fn reserved_space_names_are_documented_constants() {
        assert_eq!(DEFAULT_SPACE, "default");
        assert_eq!(SYSTEM_SPACE, "_system_");
    }

    fn create_branch(persistence: &mut StoragePersistence, name: &str) -> BranchCatalogRecord {
        let record = BranchCatalogRecord::root(BranchName::new(name).expect("valid branch"), 1);
        persistence
            .create_branch(record.storage_branch_id(), record.generation())
            .expect("storage branch created");
        record
    }
}
