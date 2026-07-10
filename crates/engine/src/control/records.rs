//! Control-plane row payloads.

use strata_core::{BranchId, CommitVersion, Timestamp};

use crate::branch::catalog::{BranchCatalogRecord, BranchParentRecord, BranchStatus};
use crate::branch::BranchName;
use crate::data::kv::ProductSpace;
use crate::diagnostics::{EngineError, EngineResult};

const PAYLOAD_VERSION: u8 = 1;
const IDENTITY_MAGIC: &[u8] = b"strata.engine.identity";
const LOCAL_INSTANCE_MAGIC: &[u8] = b"strata.engine.local-instance";
const REGISTRY_MAGIC: &[u8] = b"strata.engine.registry";
const CAPABILITY_MAGIC: &[u8] = b"strata.engine.capabilities";
const MIGRATION_MAGIC: &[u8] = b"strata.engine.migrations";
const INDEX_MAGIC: &[u8] = b"strata.engine.branch-index";
const DEFAULT_BRANCH_MAGIC: &[u8] = b"strata.engine.branch-default";
const PENDING_INDEX_MAGIC: &[u8] = b"strata.engine.branch-pending-index";
const BRANCH_MAGIC: &[u8] = b"strata.engine.branch-record";
const PENDING_MAGIC: &[u8] = b"strata.engine.branch-pending";
const SPACE_INDEX_MAGIC: &[u8] = b"strata.engine.space-index";
const SPACE_MAGIC: &[u8] = b"strata.engine.space-record";
const RESERVED_SPACE_MAGIC: &[u8] = b"strata.engine.reserved-space";
const LAYOUT_VERSION: u16 = 1;
const REGISTRY_VERSION: u16 = 1;
const KV_CAPABILITY_VERSION: u16 = 1;
const MIGRATION_REGISTRY_VERSION: u16 = 1;
const CORE_CONTROL_STORAGE_SPACE_IDS: &[u8] = &[0x30, 0x31, 0x32, 0x34];

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DatabaseIdentityRecord {
    layout_version: u16,
}

impl DatabaseIdentityRecord {
    pub(crate) const fn current() -> Self {
        Self {
            layout_version: LAYOUT_VERSION,
        }
    }
}

pub(crate) fn encode_database_identity(record: &DatabaseIdentityRecord) -> Vec<u8> {
    let mut out = versioned_payload(IDENTITY_MAGIC);
    out.extend_from_slice(&record.layout_version.to_be_bytes());
    out
}

pub(crate) fn decode_database_identity(bytes: &[u8]) -> EngineResult<DatabaseIdentityRecord> {
    let mut cursor = Cursor::new(expect_payload(bytes, IDENTITY_MAGIC)?);
    let layout_version = cursor.u16("database identity layout version")?;
    cursor.finish("database identity")?;
    if layout_version != LAYOUT_VERSION {
        return Err(EngineError::incompatible_layout(
            "failed_precondition.engine.layout_version",
            "database identity layout version is not supported",
        ));
    }
    Ok(DatabaseIdentityRecord { layout_version })
}

pub(crate) fn encode_local_instance_identity(record: &DatabaseIdentityRecord) -> Vec<u8> {
    let mut out = versioned_payload(LOCAL_INSTANCE_MAGIC);
    out.extend_from_slice(&record.layout_version.to_be_bytes());
    out
}

pub(crate) fn decode_local_instance_identity(bytes: &[u8]) -> EngineResult<DatabaseIdentityRecord> {
    let mut cursor = Cursor::new(expect_payload(bytes, LOCAL_INSTANCE_MAGIC)?);
    let layout_version = cursor.u16("local instance identity layout version")?;
    cursor.finish("local instance identity")?;
    if layout_version != LAYOUT_VERSION {
        return Err(EngineError::incompatible_layout(
            "failed_precondition.engine.layout_version",
            "local instance identity layout version is not supported",
        ));
    }
    Ok(DatabaseIdentityRecord { layout_version })
}

pub(crate) fn encode_storage_registry() -> Vec<u8> {
    let mut out = versioned_payload(REGISTRY_MAGIC);
    out.extend_from_slice(&REGISTRY_VERSION.to_be_bytes());
    out.extend_from_slice(CORE_CONTROL_STORAGE_SPACE_IDS);
    out
}

pub(crate) fn decode_storage_registry(bytes: &[u8]) -> EngineResult<()> {
    let mut cursor = Cursor::new(expect_payload(bytes, REGISTRY_MAGIC)?);
    let version = cursor.u16("storage registry version")?;
    let ids = cursor.remaining();
    if version != REGISTRY_VERSION || ids != CORE_CONTROL_STORAGE_SPACE_IDS {
        return Err(EngineError::incompatible_layout(
            "failed_precondition.engine.storage_registry",
            "storage-space registry is not supported",
        ));
    }
    Ok(())
}

pub(crate) fn encode_capability_registry() -> Vec<u8> {
    let mut out = versioned_payload(CAPABILITY_MAGIC);
    out.extend_from_slice(&KV_CAPABILITY_VERSION.to_be_bytes());
    out.extend_from_slice(b"kv");
    out
}

pub(crate) fn decode_capability_registry(bytes: &[u8]) -> EngineResult<()> {
    let mut cursor = Cursor::new(expect_payload(bytes, CAPABILITY_MAGIC)?);
    let version = cursor.u16("capability registry version")?;
    let capability = cursor.remaining();
    if version != KV_CAPABILITY_VERSION || capability != b"kv" {
        return Err(EngineError::incompatible_layout(
            "failed_precondition.engine.capability_registry",
            "capability registry does not advertise KV support",
        ));
    }
    Ok(())
}

pub(crate) fn encode_migration_registry() -> Vec<u8> {
    let mut out = versioned_payload(MIGRATION_MAGIC);
    out.extend_from_slice(&MIGRATION_REGISTRY_VERSION.to_be_bytes());
    out
}

pub(crate) fn decode_migration_registry(bytes: &[u8]) -> EngineResult<()> {
    let mut cursor = Cursor::new(expect_payload(bytes, MIGRATION_MAGIC)?);
    let version = cursor.u16("migration registry version")?;
    cursor.finish("migration registry")?;
    if version != MIGRATION_REGISTRY_VERSION {
        return Err(EngineError::incompatible_layout(
            "failed_precondition.engine.migration_registry",
            "migration registry is not supported",
        ));
    }
    Ok(())
}

pub(crate) fn encode_branch_index(names: &[BranchName]) -> EngineResult<Vec<u8>> {
    encode_name_index(INDEX_MAGIC, names)
}

pub(crate) fn decode_branch_index(bytes: &[u8]) -> EngineResult<Vec<BranchName>> {
    decode_name_index(bytes, INDEX_MAGIC, "branch index")
}

pub(crate) fn encode_default_branch(name: &BranchName) -> Vec<u8> {
    let mut out = versioned_payload(DEFAULT_BRANCH_MAGIC);
    write_name(&mut out, name.as_str());
    out
}

pub(crate) fn decode_default_branch(bytes: &[u8]) -> EngineResult<BranchName> {
    let mut cursor = Cursor::new(expect_payload(bytes, DEFAULT_BRANCH_MAGIC)?);
    let name = BranchName::new(cursor.name("default branch")?)?;
    cursor.finish("default branch")?;
    Ok(name)
}

pub(crate) fn encode_pending_branch_index(names: &[BranchName]) -> EngineResult<Vec<u8>> {
    encode_name_index(PENDING_INDEX_MAGIC, names)
}

pub(crate) fn decode_pending_branch_index(bytes: &[u8]) -> EngineResult<Vec<BranchName>> {
    decode_name_index(bytes, PENDING_INDEX_MAGIC, "pending branch index")
}

pub(crate) fn encode_branch_record(record: &BranchCatalogRecord) -> Vec<u8> {
    let mut out = versioned_payload(BRANCH_MAGIC);
    encode_branch_body(&mut out, record);
    out
}

pub(crate) fn decode_branch_record(bytes: &[u8]) -> EngineResult<BranchCatalogRecord> {
    decode_branch_like(bytes, BRANCH_MAGIC, "branch catalog")
}

pub(crate) fn encode_pending_branch_record(record: &BranchCatalogRecord) -> Vec<u8> {
    let mut out = versioned_payload(PENDING_MAGIC);
    encode_branch_body(&mut out, record);
    out
}

pub(crate) fn decode_pending_branch_record(bytes: &[u8]) -> EngineResult<BranchCatalogRecord> {
    decode_branch_like(bytes, PENDING_MAGIC, "pending branch record")
}

pub(crate) fn encode_space_index(spaces: &[ProductSpace]) -> EngineResult<Vec<u8>> {
    let count = u16::try_from(spaces.len()).map_err(|_| {
        EngineError::invalid_input(
            "invalid_argument.engine.space_catalog",
            "space catalog contains too many entries",
        )
    })?;
    let mut out = versioned_payload(SPACE_INDEX_MAGIC);
    out.extend_from_slice(&count.to_be_bytes());
    for space in spaces {
        write_name(&mut out, space.as_str());
    }
    Ok(out)
}

pub(crate) fn decode_space_index(bytes: &[u8]) -> EngineResult<Vec<ProductSpace>> {
    let mut cursor = Cursor::new(expect_payload(bytes, SPACE_INDEX_MAGIC)?);
    let count = usize::from(cursor.u16("space index")?);
    let mut spaces = Vec::with_capacity(count);
    for _ in 0..count {
        let name = cursor.name("space index")?;
        spaces.push(ProductSpace::new(name).map_err(|_| {
            EngineError::corruption(
                "data_loss.engine.space_catalog",
                "space index contains an invalid product space",
            )
        })?);
    }
    cursor.finish("space index")?;
    for window in spaces.windows(2) {
        if window[0] >= window[1] {
            return Err(EngineError::corruption(
                "data_loss.engine.space_catalog",
                "space index entries must be sorted and unique",
            ));
        }
    }
    Ok(spaces)
}

pub(crate) fn encode_space_record(space: &ProductSpace) -> Vec<u8> {
    let mut out = versioned_payload(SPACE_MAGIC);
    write_name(&mut out, space.as_str());
    out
}

pub(crate) fn decode_space_record(bytes: &[u8]) -> EngineResult<ProductSpace> {
    let mut cursor = Cursor::new(expect_payload(bytes, SPACE_MAGIC)?);
    let name = cursor.name("space record")?;
    let space = ProductSpace::new(name).map_err(|_| {
        EngineError::corruption(
            "data_loss.engine.space_catalog",
            "space record contains an invalid product space",
        )
    })?;
    cursor.finish("space record")?;
    Ok(space)
}

pub(crate) fn encode_reserved_system_space() -> Vec<u8> {
    let mut out = versioned_payload(RESERVED_SPACE_MAGIC);
    write_name(&mut out, crate::control::space::SYSTEM_SPACE);
    out.push(0);
    out
}

pub(crate) fn decode_reserved_system_space(bytes: &[u8]) -> EngineResult<()> {
    let mut cursor = Cursor::new(expect_payload(bytes, RESERVED_SPACE_MAGIC)?);
    let name = cursor.name("reserved space")?;
    let user_managed = cursor.u8("reserved space user-managed flag")?;
    cursor.finish("reserved space")?;
    if name != crate::control::space::SYSTEM_SPACE || user_managed != 0 {
        return Err(EngineError::corruption(
            "data_loss.engine.space_catalog",
            "reserved system space facts are invalid",
        ));
    }
    Ok(())
}

fn decode_branch_like(
    bytes: &[u8],
    magic: &[u8],
    description: &'static str,
) -> EngineResult<BranchCatalogRecord> {
    let mut cursor = Cursor::new(expect_payload(bytes, magic)?);
    let name = BranchName::new(cursor.name(description)?)?;
    let branch_id = cursor.branch_id("branch id")?;
    let storage_branch_id = cursor.branch_id("storage branch id")?;
    let generation = cursor.u64("branch generation")?;
    let status = match cursor.u8("branch status")? {
        0 => BranchStatus::Active,
        1 => BranchStatus::Deleted,
        _ => {
            return Err(EngineError::corruption(
                "data_loss.engine.branch_catalog",
                "branch record has an invalid status",
            ))
        }
    };
    let parent = decode_parent(&mut cursor, description)?;
    let created_at = cursor.optional_commit_version("created version")?;
    let deleted_at = cursor.optional_commit_version("deleted version")?;
    let state_revision = cursor.u64("state revision")?;
    cursor.finish(description)?;
    Ok(BranchCatalogRecord::new(
        name,
        branch_id,
        storage_branch_id,
        generation,
        status,
        parent,
        created_at,
        deleted_at,
        state_revision,
    ))
}

fn encode_branch_body(out: &mut Vec<u8>, record: &BranchCatalogRecord) {
    write_name(out, record.name().as_str());
    out.extend_from_slice(record.branch_id().as_bytes());
    out.extend_from_slice(record.storage_branch_id().as_bytes());
    out.extend_from_slice(&record.generation().to_be_bytes());
    out.push(match record.status() {
        BranchStatus::Active => 0,
        BranchStatus::Deleted => 1,
    });
    match record.parent() {
        Some(parent) => {
            out.push(1);
            write_name(out, parent.name().as_str());
            out.extend_from_slice(parent.branch_id().as_bytes());
            out.extend_from_slice(&parent.generation().to_be_bytes());
            out.extend_from_slice(&parent.fork_version().as_u64().to_be_bytes());
            write_optional_timestamp(out, parent.fork_timestamp());
        }
        None => out.push(0),
    }
    write_optional_commit_version(out, record.created_at());
    write_optional_commit_version(out, record.deleted_at());
    out.extend_from_slice(&record.state_revision().to_be_bytes());
}

fn decode_parent(
    cursor: &mut Cursor<'_>,
    description: &'static str,
) -> EngineResult<Option<BranchParentRecord>> {
    match cursor.u8("parent flag")? {
        0 => Ok(None),
        1 => {
            let name = BranchName::new(cursor.name(description)?)?;
            let branch_id = cursor.branch_id("parent branch id")?;
            let generation = cursor.u64("parent generation")?;
            let fork_version = CommitVersion::new(cursor.u64("fork version")?);
            let fork_timestamp = cursor.optional_timestamp("fork timestamp")?;
            Ok(Some(BranchParentRecord::new(
                name,
                branch_id,
                generation,
                fork_version,
                fork_timestamp,
            )))
        }
        _ => Err(EngineError::corruption(
            "data_loss.engine.branch_catalog",
            "branch record has an invalid parent flag",
        )),
    }
}

fn versioned_payload(magic: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(magic.len() + 2);
    out.extend_from_slice(magic);
    out.push(0);
    out.push(PAYLOAD_VERSION);
    out
}

fn expect_payload<'a>(bytes: &'a [u8], magic: &[u8]) -> EngineResult<&'a [u8]> {
    let header_len = magic.len() + 2;
    if bytes.len() < header_len || &bytes[..magic.len()] != magic || bytes[magic.len()] != 0 {
        return Err(EngineError::corruption(
            "data_loss.engine.control_plane",
            "control-plane row has an unknown payload family",
        ));
    }
    let version = bytes[magic.len() + 1];
    if version != PAYLOAD_VERSION {
        return Err(EngineError::incompatible_layout(
            "failed_precondition.engine.control_payload_version",
            "control-plane row payload version is not supported",
        ));
    }
    Ok(&bytes[header_len..])
}

fn encode_name_index(magic: &[u8], names: &[BranchName]) -> EngineResult<Vec<u8>> {
    let count = u16::try_from(names.len()).map_err(|_| {
        EngineError::invalid_input(
            "invalid_argument.engine.branch_catalog",
            "branch catalog contains too many entries",
        )
    })?;
    let mut out = versioned_payload(magic);
    out.extend_from_slice(&count.to_be_bytes());
    for name in names {
        write_name(&mut out, name.as_str());
    }
    Ok(out)
}

fn decode_name_index(
    bytes: &[u8],
    magic: &[u8],
    description: &'static str,
) -> EngineResult<Vec<BranchName>> {
    let mut cursor = Cursor::new(expect_payload(bytes, magic)?);
    let count = usize::from(cursor.u16(description)?);
    let mut names = Vec::with_capacity(count);
    for _ in 0..count {
        names.push(BranchName::new(cursor.name(description)?)?);
    }
    cursor.finish(description)?;
    Ok(names)
}

fn write_name(out: &mut Vec<u8>, name: &str) {
    let name_len = u16::try_from(name.len()).expect("validated control name length");
    out.extend_from_slice(&name_len.to_be_bytes());
    out.extend_from_slice(name.as_bytes());
}

fn write_optional_commit_version(out: &mut Vec<u8>, version: Option<CommitVersion>) {
    match version {
        Some(version) => {
            out.push(1);
            out.extend_from_slice(&version.as_u64().to_be_bytes());
        }
        None => out.push(0),
    }
}

fn write_optional_timestamp(out: &mut Vec<u8>, timestamp: Option<Timestamp>) {
    match timestamp {
        Some(timestamp) => {
            out.push(1);
            out.extend_from_slice(&timestamp.as_micros().to_be_bytes());
        }
        None => out.push(0),
    }
}

struct Cursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Cursor<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn remaining(&mut self) -> &'a [u8] {
        let remaining = &self.bytes[self.offset..];
        self.offset = self.bytes.len();
        remaining
    }

    fn u8(&mut self, field: &'static str) -> EngineResult<u8> {
        self.take(1, field).map(|bytes| bytes[0])
    }

    fn u16(&mut self, field: &'static str) -> EngineResult<u16> {
        let bytes = self.take(2, field)?;
        Ok(u16::from_be_bytes([bytes[0], bytes[1]]))
    }

    fn u64(&mut self, field: &'static str) -> EngineResult<u64> {
        let bytes = self.take(8, field)?;
        Ok(u64::from_be_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]))
    }

    fn optional_commit_version(
        &mut self,
        field: &'static str,
    ) -> EngineResult<Option<CommitVersion>> {
        match self.u8(field)? {
            0 => Ok(None),
            1 => Ok(Some(CommitVersion::new(self.u64(field)?))),
            _ => Err(EngineError::corruption(
                "data_loss.engine.control_plane",
                format!("control-plane row has an invalid optional {field} flag"),
            )),
        }
    }

    fn optional_timestamp(&mut self, field: &'static str) -> EngineResult<Option<Timestamp>> {
        match self.u8(field)? {
            0 => Ok(None),
            1 => Ok(Some(Timestamp::from_micros(self.u64(field)?))),
            _ => Err(EngineError::corruption(
                "data_loss.engine.control_plane",
                format!("control-plane row has an invalid optional {field} flag"),
            )),
        }
    }

    fn branch_id(&mut self, field: &'static str) -> EngineResult<BranchId> {
        let bytes = self.take(BranchId::BYTE_LEN, field)?;
        BranchId::try_from_slice(bytes).map_err(|_| {
            EngineError::corruption("data_loss.engine.branch_id", "branch id payload is invalid")
        })
    }

    fn name(&mut self, field: &'static str) -> EngineResult<String> {
        let len = usize::from(self.u16(field)?);
        let bytes = self.take(len, field)?;
        String::from_utf8(bytes.to_vec()).map_err(|_| {
            EngineError::corruption(
                "data_loss.engine.control_name",
                "control-plane name is not UTF-8",
            )
        })
    }

    fn finish(&self, description: &'static str) -> EngineResult<()> {
        if self.offset == self.bytes.len() {
            Ok(())
        } else {
            Err(EngineError::corruption(
                "data_loss.engine.control_plane",
                format!("{description} row has trailing bytes"),
            ))
        }
    }

    fn take(&mut self, len: usize, field: &'static str) -> EngineResult<&'a [u8]> {
        let end = self.offset.saturating_add(len);
        if end > self.bytes.len() {
            return Err(EngineError::corruption(
                "data_loss.engine.control_plane",
                format!("control-plane row is truncated at {field}"),
            ));
        }
        let bytes = &self.bytes[self.offset..end];
        self.offset = end;
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        decode_branch_index, decode_branch_record, decode_capability_registry,
        decode_database_identity, decode_local_instance_identity, decode_migration_registry,
        decode_reserved_system_space, decode_space_index, decode_space_record,
        decode_storage_registry, encode_branch_index, encode_branch_record,
        encode_capability_registry, encode_database_identity, encode_local_instance_identity,
        encode_migration_registry, encode_reserved_system_space, encode_space_index,
        encode_space_record, encode_storage_registry, DatabaseIdentityRecord, CAPABILITY_MAGIC,
        CORE_CONTROL_STORAGE_SPACE_IDS, IDENTITY_MAGIC, MIGRATION_MAGIC, REGISTRY_MAGIC,
    };
    use crate::branch::catalog::BranchCatalogRecord;
    use crate::branch::BranchName;
    use crate::data::kv::ProductSpace;
    use crate::diagnostics::EngineErrorClass;

    #[test]
    fn database_identity_rejects_truncated_payload() {
        let mut payload = encode_database_identity(&DatabaseIdentityRecord::current());
        payload.pop();
        let error = decode_database_identity(&payload).expect_err("truncated payload must fail");
        assert_eq!(error.class(), EngineErrorClass::Corruption);
    }

    #[test]
    fn control_payload_rejects_unknown_version() {
        let mut payload = encode_database_identity(&DatabaseIdentityRecord::current());
        payload[IDENTITY_MAGIC.len() + 1] = 2;
        let error =
            decode_database_identity(&payload).expect_err("unknown payload version must fail");
        assert_eq!(error.class(), EngineErrorClass::IncompatibleLayout);
        assert_eq!(
            error.code(),
            "failed_precondition.engine.control_payload_version"
        );
    }

    #[test]
    fn local_instance_identity_round_trips() {
        let record = DatabaseIdentityRecord::current();
        let decoded = decode_local_instance_identity(&encode_local_instance_identity(&record))
            .expect("local identity decodes");
        assert_eq!(decoded, record);
    }

    #[test]
    fn storage_registry_rejects_unknown_future_version() {
        let mut payload = encode_storage_registry();
        let offset = REGISTRY_MAGIC.len() + 2;
        payload[offset..offset + 2].copy_from_slice(&2_u16.to_be_bytes());
        let error = decode_storage_registry(&payload).expect_err("future registry must fail");
        assert_eq!(error.class(), EngineErrorClass::IncompatibleLayout);
        assert_eq!(error.code(), "failed_precondition.engine.storage_registry");
    }

    #[test]
    fn capability_registry_round_trips_and_rejects_unknown_future_version() {
        decode_capability_registry(&encode_capability_registry()).expect("capabilities decode");

        let mut payload = encode_capability_registry();
        let offset = CAPABILITY_MAGIC.len() + 2;
        payload[offset..offset + 2].copy_from_slice(&2_u16.to_be_bytes());
        let error =
            decode_capability_registry(&payload).expect_err("future capabilities must fail");
        assert_eq!(error.class(), EngineErrorClass::IncompatibleLayout);
        assert_eq!(
            error.code(),
            "failed_precondition.engine.capability_registry"
        );
    }

    #[test]
    fn migration_registry_round_trips_and_rejects_unknown_future_version() {
        decode_migration_registry(&encode_migration_registry()).expect("migrations decode");

        let mut payload = encode_migration_registry();
        let offset = MIGRATION_MAGIC.len() + 2;
        payload[offset..offset + 2].copy_from_slice(&2_u16.to_be_bytes());
        let error = decode_migration_registry(&payload).expect_err("future migrations must fail");
        assert_eq!(error.class(), EngineErrorClass::IncompatibleLayout);
        assert_eq!(
            error.code(),
            "failed_precondition.engine.migration_registry"
        );
    }

    #[test]
    fn storage_registry_records_core_control_ids() {
        assert_eq!(CORE_CONTROL_STORAGE_SPACE_IDS, &[0x30, 0x31, 0x32, 0x34]);
        decode_storage_registry(&encode_storage_registry()).expect("registry decodes");
    }

    #[test]
    fn branch_record_round_trips() {
        let record =
            BranchCatalogRecord::root(BranchName::new("default").expect("valid branch"), 1);
        let decoded =
            decode_branch_record(&encode_branch_record(&record)).expect("record must decode");
        assert_eq!(decoded, record);
    }

    #[test]
    fn branch_index_rejects_count_overflow() {
        let names: Vec<_> = (0..=u16::MAX)
            .map(|index| BranchName::new(format!("branch-{index}")).expect("valid branch"))
            .collect();
        let error = encode_branch_index(&names).expect_err("oversized index must fail");
        assert_eq!(error.class(), EngineErrorClass::InvalidInput);
        assert_eq!(error.code(), "invalid_argument.engine.branch_catalog");
    }

    #[test]
    fn branch_index_decode_rejects_trailing_bytes() {
        let names = [BranchName::new("default").expect("valid branch")];
        let mut payload = encode_branch_index(&names).expect("index encodes");
        payload.push(0xff);
        let error = decode_branch_index(&payload).expect_err("trailing bytes must fail");
        assert_eq!(error.class(), EngineErrorClass::Corruption);
    }

    #[test]
    fn space_index_round_trips_sorted_spaces() {
        let spaces = [
            ProductSpace::new("default").expect("valid space"),
            ProductSpace::new("tenant").expect("valid space"),
        ];
        let decoded =
            decode_space_index(&encode_space_index(&spaces).expect("space index encodes"))
                .expect("space index decodes");
        assert_eq!(decoded, spaces);
    }

    #[test]
    fn space_index_rejects_unsorted_or_duplicate_spaces() {
        let default = ProductSpace::new("default").expect("valid space");
        let tenant = ProductSpace::new("tenant").expect("valid space");

        for spaces in [[tenant, default.clone()], [default.clone(), default]] {
            let error = decode_space_index(&encode_space_index(&spaces).expect("encoded"))
                .expect_err("malformed space index rejected");
            assert_eq!(error.class(), EngineErrorClass::Corruption);
            assert_eq!(error.code(), "data_loss.engine.space_catalog");
        }
    }

    #[test]
    fn space_record_round_trips() {
        let space = ProductSpace::new("default").expect("valid space");
        let decoded =
            decode_space_record(&encode_space_record(&space)).expect("space record decodes");
        assert_eq!(decoded, space);
    }

    #[test]
    fn reserved_system_space_record_round_trips() {
        decode_reserved_system_space(&encode_reserved_system_space())
            .expect("reserved system space decodes");
    }

    #[test]
    fn reserved_system_space_record_rejects_user_managed_system_space() {
        let mut payload = encode_reserved_system_space();
        let flag = payload.last_mut().expect("reserved flag present");
        *flag = 1;

        let error = decode_reserved_system_space(&payload)
            .expect_err("user-managed reserved system space must fail");
        assert_eq!(error.class(), EngineErrorClass::Corruption);
        assert_eq!(error.code(), "data_loss.engine.space_catalog");
    }
}
