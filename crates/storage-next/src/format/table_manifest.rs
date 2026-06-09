use super::{ByteReader, FormatError};
use crate::branch::facts::BranchLevel;
use crate::layout::{ObjectLayout, TableObjectClassification};
use crate::object::{ObjectName, MAX_OBJECT_NAME_BYTES};
use crate::table::TableIdentity;
use std::collections::BTreeSet;
use strata_core_next::{BranchId, CommitVersion, Timestamp};

const FORMAT: &str = "table_manifest";
const MAGIC: [u8; 4] = *b"STTM";
const FORMAT_VERSION: u32 = 1;
const MAX_LEVELS: usize = 256;
const MAX_TABLES_PER_LEVEL: usize = 16_384;
const MAX_TOTAL_TABLES: usize = 65_536;
const MAX_INHERITED_LAYERS: usize = 4_096;
const MAX_EXTENSIONS: usize = 64;
const MAX_IDENTITY_BYTES: usize = 256;
const MAX_KEY_BYTES: usize = 4_096;
const MAX_SECTION_KIND_BYTES: usize = 64;
const MAX_SECTION_BYTES: usize = 1_048_576;
const EXTENSION_FLAG_REQUIRED: u8 = 0x01;
const EXTENSION_FLAG_PRESERVE_ON_REWRITE: u8 = 0x02;
const EXTENSION_FLAGS_SUPPORTED: u8 = EXTENSION_FLAG_REQUIRED | EXTENSION_FLAG_PRESERVE_ON_REWRITE;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TableManifest {
    branch_id: BranchId,
    branch_generation: Option<u64>,
    manifest_sequence: u64,
    levels: Vec<TableManifestLevel>,
    inherited_layers: Vec<TableManifestInheritedLayer>,
    extension_sections: Vec<TableManifestExtensionSection>,
}

impl TableManifest {
    pub(crate) fn new(
        branch_id: BranchId,
        branch_generation: Option<u64>,
        manifest_sequence: u64,
        mut levels: Vec<TableManifestLevel>,
        mut inherited_layers: Vec<TableManifestInheritedLayer>,
        mut extension_sections: Vec<TableManifestExtensionSection>,
    ) -> Result<Self, FormatError> {
        levels.sort_by_key(|level| level.level().raw());
        inherited_layers.sort_by_key(TableManifestInheritedLayer::order);
        extension_sections.sort_by(|left, right| left.kind().cmp(right.kind()));
        Self::from_parts(
            branch_id,
            branch_generation,
            manifest_sequence,
            levels,
            inherited_layers,
            extension_sections,
            true,
        )
    }

    fn from_decoded(
        branch_id: BranchId,
        branch_generation: Option<u64>,
        manifest_sequence: u64,
        levels: Vec<TableManifestLevel>,
        inherited_layers: Vec<TableManifestInheritedLayer>,
        extension_sections: Vec<TableManifestExtensionSection>,
    ) -> Result<Self, FormatError> {
        Self::from_parts(
            branch_id,
            branch_generation,
            manifest_sequence,
            levels,
            inherited_layers,
            extension_sections,
            false,
        )
    }

    fn from_parts(
        branch_id: BranchId,
        branch_generation: Option<u64>,
        manifest_sequence: u64,
        levels: Vec<TableManifestLevel>,
        inherited_layers: Vec<TableManifestInheritedLayer>,
        extension_sections: Vec<TableManifestExtensionSection>,
        canonicalized_by_constructor: bool,
    ) -> Result<Self, FormatError> {
        if manifest_sequence == 0 {
            return Err(FormatError::InvalidValue {
                field: "manifest_sequence",
            });
        }
        validate_generation(branch_generation, "branch_generation")?;
        validate_levels(&levels, canonicalized_by_constructor)?;
        validate_inherited_layers(&inherited_layers, canonicalized_by_constructor)?;
        validate_extensions(&extension_sections, canonicalized_by_constructor)?;
        validate_table_objects_for_branch(branch_id, &levels)?;
        for layer in &inherited_layers {
            validate_table_objects_for_branch(layer.source_branch_id(), layer.levels())?;
        }
        validate_global_table_uniqueness(&levels, &inherited_layers)?;
        Ok(Self {
            branch_id,
            branch_generation,
            manifest_sequence,
            levels,
            inherited_layers,
            extension_sections,
        })
    }

    pub(crate) const fn branch_id(&self) -> BranchId {
        self.branch_id
    }

    pub(crate) const fn branch_generation(&self) -> Option<u64> {
        self.branch_generation
    }

    pub(crate) const fn manifest_sequence(&self) -> u64 {
        self.manifest_sequence
    }

    pub(crate) fn levels(&self) -> &[TableManifestLevel] {
        &self.levels
    }

    pub(crate) fn inherited_layers(&self) -> &[TableManifestInheritedLayer] {
        &self.inherited_layers
    }

    pub(crate) fn extension_sections(&self) -> &[TableManifestExtensionSection] {
        &self.extension_sections
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TableManifestLevel {
    level: BranchLevel,
    tables: Vec<TableManifestTableRef>,
}

impl TableManifestLevel {
    pub(crate) fn new(
        level: BranchLevel,
        mut tables: Vec<TableManifestTableRef>,
    ) -> Result<Self, FormatError> {
        canonicalize_level_tables(level, &mut tables);
        Self::from_parts(level, tables, true)
    }

    fn from_decoded(
        level: BranchLevel,
        tables: Vec<TableManifestTableRef>,
    ) -> Result<Self, FormatError> {
        Self::from_parts(level, tables, false)
    }

    fn from_parts(
        level: BranchLevel,
        tables: Vec<TableManifestTableRef>,
        canonicalized_by_constructor: bool,
    ) -> Result<Self, FormatError> {
        if usize::from(level.raw()) >= MAX_LEVELS {
            return Err(FormatError::InvalidValue { field: "level" });
        }
        validate_level_tables(level, &tables, canonicalized_by_constructor)?;
        Ok(Self { level, tables })
    }

    pub(crate) const fn level(&self) -> BranchLevel {
        self.level
    }

    pub(crate) fn tables(&self) -> &[TableManifestTableRef] {
        &self.tables
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TableManifestTableRef {
    table_identity: TableIdentity,
    object: ObjectName,
    order: u32,
    facts: TableManifestTableFacts,
    bounds: TableManifestTableBounds,
    provenance: TableManifestTableProvenance,
}

impl TableManifestTableRef {
    pub(crate) fn new(
        table_identity: TableIdentity,
        object: ObjectName,
        order: u32,
        facts: TableManifestTableFacts,
        bounds: TableManifestTableBounds,
        provenance: TableManifestTableProvenance,
    ) -> Result<Self, FormatError> {
        validate_table_object_name(&object)?;
        Ok(Self {
            table_identity,
            object,
            order,
            facts,
            bounds,
            provenance,
        })
    }

    pub(crate) const fn table_identity(&self) -> &TableIdentity {
        &self.table_identity
    }

    pub(crate) const fn object(&self) -> &ObjectName {
        &self.object
    }

    pub(crate) const fn order(&self) -> u32 {
        self.order
    }

    pub(crate) const fn facts(&self) -> &TableManifestTableFacts {
        &self.facts
    }

    pub(crate) const fn bounds(&self) -> &TableManifestTableBounds {
        &self.bounds
    }

    pub(crate) const fn provenance(&self) -> &TableManifestTableProvenance {
        &self.provenance
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TableManifestTableFacts {
    byte_count: u64,
    row_count: u64,
    data_block_count: u32,
    commit_min: CommitVersion,
    commit_max: CommitVersion,
    timestamp_min: Option<Timestamp>,
    timestamp_max: Option<Timestamp>,
}

impl TableManifestTableFacts {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        byte_count: u64,
        row_count: u64,
        data_block_count: u32,
        commit_min: CommitVersion,
        commit_max: CommitVersion,
        timestamp_min: Option<Timestamp>,
        timestamp_max: Option<Timestamp>,
    ) -> Result<Self, FormatError> {
        if byte_count == 0 {
            return Err(FormatError::InvalidValue {
                field: "byte_count",
            });
        }
        if row_count == 0 {
            return Err(FormatError::InvalidValue { field: "row_count" });
        }
        if data_block_count == 0 || u64::from(data_block_count) > row_count {
            return Err(FormatError::InvalidValue {
                field: "data_block_count",
            });
        }
        if commit_min > commit_max {
            return Err(FormatError::InvalidValue {
                field: "commit_range",
            });
        }
        match (timestamp_min, timestamp_max) {
            (Some(min), Some(max)) if min > max => {
                return Err(FormatError::InvalidValue {
                    field: "timestamp_range",
                });
            }
            (Some(_), None) | (None, Some(_)) => {
                return Err(FormatError::InvalidValue {
                    field: "timestamp_range",
                });
            }
            (Some(_), Some(_)) | (None, None) => {}
        }
        Ok(Self {
            byte_count,
            row_count,
            data_block_count,
            commit_min,
            commit_max,
            timestamp_min,
            timestamp_max,
        })
    }

    pub(crate) const fn byte_count(&self) -> u64 {
        self.byte_count
    }

    pub(crate) const fn row_count(&self) -> u64 {
        self.row_count
    }

    pub(crate) const fn data_block_count(&self) -> u32 {
        self.data_block_count
    }

    pub(crate) const fn commit_min(&self) -> CommitVersion {
        self.commit_min
    }

    pub(crate) const fn commit_max(&self) -> CommitVersion {
        self.commit_max
    }

    pub(crate) const fn timestamp_min(&self) -> Option<Timestamp> {
        self.timestamp_min
    }

    pub(crate) const fn timestamp_max(&self) -> Option<Timestamp> {
        self.timestamp_max
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TableManifestTableBounds {
    physical_first: Vec<u8>,
    physical_last: Vec<u8>,
    internal_first: Vec<u8>,
    internal_last: Vec<u8>,
}

impl TableManifestTableBounds {
    pub(crate) fn new(
        physical_first: impl Into<Vec<u8>>,
        physical_last: impl Into<Vec<u8>>,
        internal_first: impl Into<Vec<u8>>,
        internal_last: impl Into<Vec<u8>>,
    ) -> Result<Self, FormatError> {
        let bounds = Self {
            physical_first: physical_first.into(),
            physical_last: physical_last.into(),
            internal_first: internal_first.into(),
            internal_last: internal_last.into(),
        };
        validate_key_range(
            &bounds.physical_first,
            &bounds.physical_last,
            "physical_bounds",
        )?;
        validate_key_range(
            &bounds.internal_first,
            &bounds.internal_last,
            "internal_bounds",
        )?;
        Ok(bounds)
    }

    pub(crate) fn physical_first(&self) -> &[u8] {
        &self.physical_first
    }

    pub(crate) fn physical_last(&self) -> &[u8] {
        &self.physical_last
    }

    pub(crate) fn internal_first(&self) -> &[u8] {
        &self.internal_first
    }

    pub(crate) fn internal_last(&self) -> &[u8] {
        &self.internal_last
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum TableManifestTableProvenance {
    Flush,
    SnapshotInstall,
    Compaction,
    MaterializationReplacement {
        source_branch_id: BranchId,
        fork_version: CommitVersion,
    },
    Recovered,
}

impl TableManifestTableProvenance {
    pub(crate) fn materialization_replacement(
        source_branch_id: BranchId,
        fork_version: CommitVersion,
    ) -> Result<Self, FormatError> {
        if fork_version == CommitVersion::ZERO {
            return Err(FormatError::InvalidValue {
                field: "materialization_fork_version",
            });
        }
        Ok(Self::MaterializationReplacement {
            source_branch_id,
            fork_version,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TableManifestInheritedLayer {
    order: u32,
    source_branch_id: BranchId,
    source_branch_generation: Option<u64>,
    fork_version: CommitVersion,
    status: TableManifestInheritedLayerStatus,
    levels: Vec<TableManifestLevel>,
}

impl TableManifestInheritedLayer {
    pub(crate) fn new(
        order: u32,
        source_branch_id: BranchId,
        source_branch_generation: Option<u64>,
        fork_version: CommitVersion,
        status: TableManifestInheritedLayerStatus,
        mut levels: Vec<TableManifestLevel>,
    ) -> Result<Self, FormatError> {
        levels.sort_by_key(|level| level.level().raw());
        Self::from_parts(
            order,
            source_branch_id,
            source_branch_generation,
            fork_version,
            status,
            levels,
            true,
        )
    }

    fn from_decoded(
        order: u32,
        source_branch_id: BranchId,
        source_branch_generation: Option<u64>,
        fork_version: CommitVersion,
        status: TableManifestInheritedLayerStatus,
        levels: Vec<TableManifestLevel>,
    ) -> Result<Self, FormatError> {
        Self::from_parts(
            order,
            source_branch_id,
            source_branch_generation,
            fork_version,
            status,
            levels,
            false,
        )
    }

    fn from_parts(
        order: u32,
        source_branch_id: BranchId,
        source_branch_generation: Option<u64>,
        fork_version: CommitVersion,
        status: TableManifestInheritedLayerStatus,
        levels: Vec<TableManifestLevel>,
        canonicalized_by_constructor: bool,
    ) -> Result<Self, FormatError> {
        validate_generation(source_branch_generation, "source_branch_generation")?;
        if fork_version == CommitVersion::ZERO {
            return Err(FormatError::InvalidValue {
                field: "inherited_fork_version",
            });
        }
        validate_levels(&levels, canonicalized_by_constructor)?;
        Ok(Self {
            order,
            source_branch_id,
            source_branch_generation,
            fork_version,
            status,
            levels,
        })
    }

    pub(crate) const fn order(&self) -> u32 {
        self.order
    }

    pub(crate) const fn source_branch_id(&self) -> BranchId {
        self.source_branch_id
    }

    pub(crate) const fn source_branch_generation(&self) -> Option<u64> {
        self.source_branch_generation
    }

    pub(crate) const fn fork_version(&self) -> CommitVersion {
        self.fork_version
    }

    pub(crate) const fn status(&self) -> TableManifestInheritedLayerStatus {
        self.status
    }

    pub(crate) fn levels(&self) -> &[TableManifestLevel] {
        &self.levels
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TableManifestInheritedLayerStatus {
    Active,
    Materializing,
    Materialized,
    Unavailable,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TableManifestExtensionSection {
    kind: String,
    preserve_on_rewrite: bool,
    payload: Vec<u8>,
}

impl TableManifestExtensionSection {
    pub(crate) fn optional(
        kind: impl Into<String>,
        preserve_on_rewrite: bool,
        payload: impl Into<Vec<u8>>,
    ) -> Result<Self, FormatError> {
        let kind = kind.into();
        let payload = payload.into();
        validate_section_kind(&kind)?;
        if payload.len() > MAX_SECTION_BYTES {
            return Err(FormatError::InvalidLength {
                field: "extension_payload",
            });
        }
        Ok(Self {
            kind,
            preserve_on_rewrite,
            payload,
        })
    }

    pub(crate) fn kind(&self) -> &str {
        &self.kind
    }

    pub(crate) const fn preserve_on_rewrite(&self) -> bool {
        self.preserve_on_rewrite
    }

    pub(crate) fn payload(&self) -> &[u8] {
        &self.payload
    }
}

pub(crate) fn encode_table_manifest(manifest: &TableManifest) -> Result<Vec<u8>, FormatError> {
    validate_levels(manifest.levels(), false)?;
    validate_inherited_layers(manifest.inherited_layers(), false)?;
    validate_extensions(manifest.extension_sections(), false)?;
    validate_table_objects_for_branch(manifest.branch_id(), manifest.levels())?;
    for layer in manifest.inherited_layers() {
        validate_table_objects_for_branch(layer.source_branch_id(), layer.levels())?;
    }
    validate_global_table_uniqueness(manifest.levels(), manifest.inherited_layers())?;

    let mut bytes = Vec::new();
    bytes.extend_from_slice(&MAGIC);
    bytes.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    bytes.extend_from_slice(manifest.branch_id().as_bytes());
    write_optional_u64(
        &mut bytes,
        "branch_generation",
        manifest.branch_generation(),
    )?;
    bytes.extend_from_slice(&manifest.manifest_sequence().to_le_bytes());
    write_count(&mut bytes, "level_count", manifest.levels().len())?;
    write_count(
        &mut bytes,
        "inherited_layer_count",
        manifest.inherited_layers().len(),
    )?;
    write_count(
        &mut bytes,
        "extension_count",
        manifest.extension_sections().len(),
    )?;

    for level in manifest.levels() {
        encode_level(&mut bytes, level)?;
    }
    for layer in manifest.inherited_layers() {
        encode_inherited_layer(&mut bytes, layer)?;
    }
    for section in manifest.extension_sections() {
        encode_extension_section(&mut bytes, section)?;
    }

    let crc = crc32fast::hash(&bytes);
    bytes.extend_from_slice(&crc.to_le_bytes());
    Ok(bytes)
}

pub(crate) fn decode_table_manifest(bytes: &[u8]) -> Result<TableManifest, FormatError> {
    if bytes.len() < minimum_manifest_len() {
        return Err(FormatError::InsufficientBytes {
            format: FORMAT,
            needed: minimum_manifest_len(),
            actual: bytes.len(),
        });
    }

    let checksum_offset = bytes.len() - 4;
    let stored_crc = u32::from_le_bytes(
        bytes[checksum_offset..]
            .try_into()
            .map_err(|_| FormatError::InvalidLength { field: "crc32" })?,
    );
    let mut reader = ByteReader::new(FORMAT, &bytes[..checksum_offset]);
    if reader.read_exact(4)? != MAGIC {
        return Err(FormatError::InvalidMagic { format: FORMAT });
    }

    let version = reader.read_u32_le()?;
    match version {
        FORMAT_VERSION => {}
        0 => {
            return Err(FormatError::PreV1Format {
                format: FORMAT,
                version,
            });
        }
        version => {
            return Err(FormatError::FutureFormat {
                format: FORMAT,
                version,
                max_supported: FORMAT_VERSION,
            });
        }
    }

    let computed_crc = crc32fast::hash(&bytes[..checksum_offset]);
    if stored_crc != computed_crc {
        return Err(FormatError::ChecksumMismatch {
            format: FORMAT,
            expected: stored_crc,
            computed: computed_crc,
        });
    }

    let branch_id = BranchId::from_bytes(read_array_16(&mut reader, "branch_id")?);
    let branch_generation = read_optional_u64(&mut reader, "branch_generation")?;
    let manifest_sequence = reader.read_u64_le()?;
    let level_count = read_count(&mut reader, "level_count", MAX_LEVELS)?;
    let inherited_layer_count =
        read_count(&mut reader, "inherited_layer_count", MAX_INHERITED_LAYERS)?;
    let extension_count = read_count(&mut reader, "extension_count", MAX_EXTENSIONS)?;

    let mut levels = Vec::with_capacity(level_count);
    for _ in 0..level_count {
        levels.push(decode_level(&mut reader)?);
    }

    let mut inherited_layers = Vec::with_capacity(inherited_layer_count);
    for _ in 0..inherited_layer_count {
        inherited_layers.push(decode_inherited_layer(&mut reader)?);
    }

    let mut extension_sections = Vec::with_capacity(extension_count);
    for _ in 0..extension_count {
        extension_sections.push(decode_extension_section(&mut reader)?);
    }

    reader.finish()?;
    TableManifest::from_decoded(
        branch_id,
        branch_generation,
        manifest_sequence,
        levels,
        inherited_layers,
        extension_sections,
    )
}

fn encode_level(bytes: &mut Vec<u8>, level: &TableManifestLevel) -> Result<(), FormatError> {
    bytes.push(level.level().raw());
    write_count(bytes, "table_count", level.tables().len())?;
    for table in level.tables() {
        encode_table_ref(bytes, table)?;
    }
    Ok(())
}

fn decode_level(reader: &mut ByteReader<'_>) -> Result<TableManifestLevel, FormatError> {
    let level = BranchLevel::new(reader.read_u8()?);
    let table_count = read_count(reader, "table_count", MAX_TABLES_PER_LEVEL)?;
    if table_count > 0 {
        ensure_remaining_for_count(reader, table_count, minimum_table_ref_len(), "table_count")?;
    }
    let mut tables = Vec::with_capacity(table_count);
    for _ in 0..table_count {
        tables.push(decode_table_ref(reader)?);
    }
    TableManifestLevel::from_decoded(level, tables)
}

fn encode_table_ref(bytes: &mut Vec<u8>, table: &TableManifestTableRef) -> Result<(), FormatError> {
    write_string(
        bytes,
        "table_identity",
        table.table_identity().as_str(),
        MAX_IDENTITY_BYTES,
    )?;
    write_string(
        bytes,
        "table_object",
        table.object().as_str(),
        MAX_OBJECT_NAME_BYTES,
    )?;
    bytes.extend_from_slice(&table.order().to_le_bytes());
    encode_facts(bytes, table.facts());
    encode_bounds(bytes, table.bounds())?;
    encode_provenance(bytes, table.provenance());
    Ok(())
}

fn decode_table_ref(reader: &mut ByteReader<'_>) -> Result<TableManifestTableRef, FormatError> {
    let identity = read_bounded_string(reader, "table_identity", MAX_IDENTITY_BYTES)?;
    let identity = TableIdentity::new(identity).map_err(|_| FormatError::InvalidValue {
        field: "table_identity",
    })?;
    let object = read_bounded_string(reader, "table_object", MAX_OBJECT_NAME_BYTES)?;
    let object = ObjectName::new(object).map_err(|_| FormatError::InvalidValue {
        field: "table_object",
    })?;
    let order = reader.read_u32_le()?;
    let facts = decode_facts(reader)?;
    let bounds = decode_bounds(reader)?;
    let provenance = decode_provenance(reader)?;
    TableManifestTableRef::new(identity, object, order, facts, bounds, provenance)
}

fn encode_facts(bytes: &mut Vec<u8>, facts: &TableManifestTableFacts) {
    bytes.extend_from_slice(&facts.byte_count().to_le_bytes());
    bytes.extend_from_slice(&facts.row_count().to_le_bytes());
    bytes.extend_from_slice(&facts.data_block_count().to_le_bytes());
    bytes.extend_from_slice(&facts.commit_min().as_u64().to_le_bytes());
    bytes.extend_from_slice(&facts.commit_max().as_u64().to_le_bytes());
    match (facts.timestamp_min(), facts.timestamp_max()) {
        (Some(min), Some(max)) => {
            bytes.push(1);
            bytes.extend_from_slice(&min.as_micros().to_le_bytes());
            bytes.extend_from_slice(&max.as_micros().to_le_bytes());
        }
        (None, None) => {
            bytes.push(0);
            bytes.extend_from_slice(&0_u64.to_le_bytes());
            bytes.extend_from_slice(&0_u64.to_le_bytes());
        }
        (Some(_), None) | (None, Some(_)) => {
            unreachable!("facts validation rejects partial timestamp ranges")
        }
    }
}

fn decode_facts(reader: &mut ByteReader<'_>) -> Result<TableManifestTableFacts, FormatError> {
    let byte_count = reader.read_u64_le()?;
    let row_count = reader.read_u64_le()?;
    let data_block_count = reader.read_u32_le()?;
    let commit_min = CommitVersion::new(reader.read_u64_le()?);
    let commit_max = CommitVersion::new(reader.read_u64_le()?);
    let timestamps_present = read_bool(reader, "timestamp_range_present")?;
    let timestamp_min = Timestamp::from_micros(reader.read_u64_le()?);
    let timestamp_max = Timestamp::from_micros(reader.read_u64_le()?);
    let (timestamp_min, timestamp_max) = if timestamps_present {
        (Some(timestamp_min), Some(timestamp_max))
    } else if timestamp_min == Timestamp::EPOCH && timestamp_max == Timestamp::EPOCH {
        (None, None)
    } else {
        return Err(FormatError::InvalidValue {
            field: "timestamp_range",
        });
    };
    TableManifestTableFacts::new(
        byte_count,
        row_count,
        data_block_count,
        commit_min,
        commit_max,
        timestamp_min,
        timestamp_max,
    )
}

fn encode_bounds(
    bytes: &mut Vec<u8>,
    bounds: &TableManifestTableBounds,
) -> Result<(), FormatError> {
    write_bytes(
        bytes,
        "physical_first",
        bounds.physical_first(),
        MAX_KEY_BYTES,
    )?;
    write_bytes(
        bytes,
        "physical_last",
        bounds.physical_last(),
        MAX_KEY_BYTES,
    )?;
    write_bytes(
        bytes,
        "internal_first",
        bounds.internal_first(),
        MAX_KEY_BYTES,
    )?;
    write_bytes(
        bytes,
        "internal_last",
        bounds.internal_last(),
        MAX_KEY_BYTES,
    )?;
    Ok(())
}

fn decode_bounds(reader: &mut ByteReader<'_>) -> Result<TableManifestTableBounds, FormatError> {
    let physical_first = read_bounded_bytes(reader, "physical_first", MAX_KEY_BYTES)?;
    let physical_last = read_bounded_bytes(reader, "physical_last", MAX_KEY_BYTES)?;
    let internal_first = read_bounded_bytes(reader, "internal_first", MAX_KEY_BYTES)?;
    let internal_last = read_bounded_bytes(reader, "internal_last", MAX_KEY_BYTES)?;
    TableManifestTableBounds::new(physical_first, physical_last, internal_first, internal_last)
}

fn encode_provenance(bytes: &mut Vec<u8>, provenance: &TableManifestTableProvenance) {
    match provenance {
        TableManifestTableProvenance::Flush => bytes.push(0),
        TableManifestTableProvenance::SnapshotInstall => bytes.push(1),
        TableManifestTableProvenance::Compaction => bytes.push(2),
        TableManifestTableProvenance::MaterializationReplacement {
            source_branch_id,
            fork_version,
        } => {
            bytes.push(3);
            bytes.extend_from_slice(source_branch_id.as_bytes());
            bytes.extend_from_slice(&fork_version.as_u64().to_le_bytes());
        }
        TableManifestTableProvenance::Recovered => bytes.push(4),
    }
}

fn decode_provenance(
    reader: &mut ByteReader<'_>,
) -> Result<TableManifestTableProvenance, FormatError> {
    match reader.read_u8()? {
        0 => Ok(TableManifestTableProvenance::Flush),
        1 => Ok(TableManifestTableProvenance::SnapshotInstall),
        2 => Ok(TableManifestTableProvenance::Compaction),
        3 => {
            let source_branch_id =
                BranchId::from_bytes(read_array_16(reader, "materialization_source_branch_id")?);
            let fork_version = CommitVersion::new(reader.read_u64_le()?);
            TableManifestTableProvenance::materialization_replacement(
                source_branch_id,
                fork_version,
            )
        }
        4 => Ok(TableManifestTableProvenance::Recovered),
        _ => Err(FormatError::InvalidValue {
            field: "table_provenance",
        }),
    }
}

fn encode_inherited_layer(
    bytes: &mut Vec<u8>,
    layer: &TableManifestInheritedLayer,
) -> Result<(), FormatError> {
    bytes.extend_from_slice(&layer.order().to_le_bytes());
    bytes.extend_from_slice(layer.source_branch_id().as_bytes());
    write_optional_u64(
        bytes,
        "source_branch_generation",
        layer.source_branch_generation(),
    )?;
    bytes.extend_from_slice(&layer.fork_version().as_u64().to_le_bytes());
    bytes.push(encode_layer_status(layer.status()));
    write_count(bytes, "inherited_level_count", layer.levels().len())?;
    for level in layer.levels() {
        encode_level(bytes, level)?;
    }
    Ok(())
}

fn decode_inherited_layer(
    reader: &mut ByteReader<'_>,
) -> Result<TableManifestInheritedLayer, FormatError> {
    let order = reader.read_u32_le()?;
    let source_branch_id = BranchId::from_bytes(read_array_16(reader, "source_branch_id")?);
    let source_branch_generation = read_optional_u64(reader, "source_branch_generation")?;
    let fork_version = CommitVersion::new(reader.read_u64_le()?);
    let status = decode_layer_status(reader.read_u8()?)?;
    let level_count = read_count(reader, "inherited_level_count", MAX_LEVELS)?;
    let mut levels = Vec::with_capacity(level_count);
    for _ in 0..level_count {
        levels.push(decode_level(reader)?);
    }
    TableManifestInheritedLayer::from_decoded(
        order,
        source_branch_id,
        source_branch_generation,
        fork_version,
        status,
        levels,
    )
}

fn encode_layer_status(status: TableManifestInheritedLayerStatus) -> u8 {
    match status {
        TableManifestInheritedLayerStatus::Active => 0,
        TableManifestInheritedLayerStatus::Materializing => 1,
        TableManifestInheritedLayerStatus::Materialized => 2,
        TableManifestInheritedLayerStatus::Unavailable => 3,
    }
}

fn decode_layer_status(value: u8) -> Result<TableManifestInheritedLayerStatus, FormatError> {
    match value {
        0 => Ok(TableManifestInheritedLayerStatus::Active),
        1 => Ok(TableManifestInheritedLayerStatus::Materializing),
        2 => Ok(TableManifestInheritedLayerStatus::Materialized),
        3 => Ok(TableManifestInheritedLayerStatus::Unavailable),
        _ => Err(FormatError::InvalidValue {
            field: "inherited_layer_status",
        }),
    }
}

fn encode_extension_section(
    bytes: &mut Vec<u8>,
    section: &TableManifestExtensionSection,
) -> Result<(), FormatError> {
    write_string(
        bytes,
        "extension_kind",
        section.kind(),
        MAX_SECTION_KIND_BYTES,
    )?;
    let flags = if section.preserve_on_rewrite() {
        EXTENSION_FLAG_PRESERVE_ON_REWRITE
    } else {
        0
    };
    bytes.push(flags);
    write_bytes(
        bytes,
        "extension_payload",
        section.payload(),
        MAX_SECTION_BYTES,
    )
}

fn decode_extension_section(
    reader: &mut ByteReader<'_>,
) -> Result<TableManifestExtensionSection, FormatError> {
    let kind = read_bounded_string(reader, "extension_kind", MAX_SECTION_KIND_BYTES)?;
    let flags = reader.read_u8()?;
    if flags & !EXTENSION_FLAGS_SUPPORTED != 0 || flags & EXTENSION_FLAG_REQUIRED != 0 {
        return Err(FormatError::UnsupportedFlags {
            format: FORMAT,
            flags: u32::from(flags),
        });
    }
    let payload = read_bounded_bytes(reader, "extension_payload", MAX_SECTION_BYTES)?;
    TableManifestExtensionSection::optional(
        kind,
        flags & EXTENSION_FLAG_PRESERVE_ON_REWRITE != 0,
        payload,
    )
}

fn canonicalize_level_tables(level: BranchLevel, tables: &mut [TableManifestTableRef]) {
    if level == BranchLevel::ZERO {
        tables.sort_by_key(TableManifestTableRef::order);
    } else {
        tables.sort_by(|left, right| {
            left.bounds()
                .physical_first()
                .cmp(right.bounds().physical_first())
                .then_with(|| left.order().cmp(&right.order()))
        });
    }
}

fn validate_levels(
    levels: &[TableManifestLevel],
    canonicalized_by_constructor: bool,
) -> Result<(), FormatError> {
    if levels.len() > MAX_LEVELS {
        return Err(FormatError::InvalidLength {
            field: "level_count",
        });
    }
    let mut seen = BTreeSet::new();
    let mut previous_level = None;
    for level in levels {
        if !seen.insert(level.level().raw()) {
            return Err(FormatError::InvalidValue { field: "level" });
        }
        if !canonicalized_by_constructor {
            if let Some(previous) = previous_level {
                if previous >= level.level().raw() {
                    return Err(FormatError::InvalidValue {
                        field: "level_order",
                    });
                }
            }
            previous_level = Some(level.level().raw());
        }
        validate_level_tables(level.level(), level.tables(), canonicalized_by_constructor)?;
    }
    Ok(())
}

fn validate_level_tables(
    level: BranchLevel,
    tables: &[TableManifestTableRef],
    canonicalized_by_constructor: bool,
) -> Result<(), FormatError> {
    if tables.len() > MAX_TABLES_PER_LEVEL {
        return Err(FormatError::InvalidLength {
            field: "table_count",
        });
    }
    let mut seen_orders = BTreeSet::new();
    let mut previous_physical_last: Option<&[u8]> = None;
    let mut previous_l1_first: Option<&[u8]> = None;
    for (index, table) in tables.iter().enumerate() {
        let expected_order = u32::try_from(index).map_err(|_| FormatError::InvalidLength {
            field: "table_order",
        })?;
        if !seen_orders.insert(table.order()) || table.order() != expected_order {
            return Err(FormatError::InvalidValue {
                field: "table_order",
            });
        }
        if level != BranchLevel::ZERO {
            if let Some(previous_first) = previous_l1_first {
                if !canonicalized_by_constructor
                    && previous_first >= table.bounds().physical_first()
                {
                    return Err(FormatError::InvalidValue {
                        field: "physical_range_order",
                    });
                }
            }
            if let Some(previous_last) = previous_physical_last {
                if previous_last >= table.bounds().physical_first() {
                    return Err(FormatError::InvalidValue {
                        field: "physical_range_overlap",
                    });
                }
            }
            previous_l1_first = Some(table.bounds().physical_first());
            previous_physical_last = Some(table.bounds().physical_last());
        }
    }
    Ok(())
}

fn validate_inherited_layers(
    layers: &[TableManifestInheritedLayer],
    canonicalized_by_constructor: bool,
) -> Result<(), FormatError> {
    if layers.len() > MAX_INHERITED_LAYERS {
        return Err(FormatError::InvalidLength {
            field: "inherited_layer_count",
        });
    }
    let mut seen_orders = BTreeSet::new();
    let mut seen_sources = BTreeSet::new();
    for (index, layer) in layers.iter().enumerate() {
        let expected_order = u32::try_from(index).map_err(|_| FormatError::InvalidLength {
            field: "inherited_layer_order",
        })?;
        if !seen_orders.insert(layer.order()) || layer.order() != expected_order {
            return Err(FormatError::InvalidValue {
                field: "inherited_layer_order",
            });
        }
        if !seen_sources.insert((
            *layer.source_branch_id().as_bytes(),
            layer.fork_version().as_u64(),
        )) {
            return Err(FormatError::InvalidValue {
                field: "inherited_layer_source",
            });
        }
        validate_levels(layer.levels(), canonicalized_by_constructor)?;
    }
    Ok(())
}

fn validate_extensions(
    sections: &[TableManifestExtensionSection],
    canonicalized_by_constructor: bool,
) -> Result<(), FormatError> {
    if sections.len() > MAX_EXTENSIONS {
        return Err(FormatError::InvalidLength {
            field: "extension_count",
        });
    }
    let mut seen = BTreeSet::new();
    let mut previous_kind: Option<&str> = None;
    for section in sections {
        validate_section_kind(section.kind())?;
        if !seen.insert(section.kind()) {
            return Err(FormatError::InvalidValue {
                field: "extension_kind",
            });
        }
        if !canonicalized_by_constructor {
            if let Some(previous) = previous_kind {
                if previous >= section.kind() {
                    return Err(FormatError::InvalidValue {
                        field: "extension_order",
                    });
                }
            }
            previous_kind = Some(section.kind());
        }
    }
    Ok(())
}

fn validate_global_table_uniqueness(
    levels: &[TableManifestLevel],
    inherited_layers: &[TableManifestInheritedLayer],
) -> Result<(), FormatError> {
    let mut identities = BTreeSet::new();
    let mut objects = BTreeSet::new();
    let mut total_tables = 0usize;
    for table in levels.iter().flat_map(TableManifestLevel::tables).chain(
        inherited_layers
            .iter()
            .flat_map(TableManifestInheritedLayer::levels)
            .flat_map(TableManifestLevel::tables),
    ) {
        total_tables = total_tables.saturating_add(1);
        if total_tables > MAX_TOTAL_TABLES {
            return Err(FormatError::InvalidLength {
                field: "total_table_count",
            });
        }
        if !identities.insert(table.table_identity().as_str()) {
            return Err(FormatError::InvalidValue {
                field: "table_identity",
            });
        }
        if !objects.insert(table.object().as_str()) {
            return Err(FormatError::InvalidValue {
                field: "table_object",
            });
        }
    }
    Ok(())
}

fn validate_table_objects_for_branch(
    branch_id: BranchId,
    levels: &[TableManifestLevel],
) -> Result<(), FormatError> {
    let expected_branch = branch_id.to_string();
    for level in levels {
        for table in level.tables() {
            let (actual_branch, _) = table_object_branch_and_level(table.object())?;
            if actual_branch != expected_branch {
                return Err(FormatError::InvalidValue {
                    field: "table_object_branch",
                });
            }
        }
    }
    Ok(())
}

fn validate_generation(value: Option<u64>, field: &'static str) -> Result<(), FormatError> {
    if value == Some(0) {
        Err(FormatError::InvalidValue { field })
    } else {
        Ok(())
    }
}

fn validate_table_object_name(object: &ObjectName) -> Result<(), FormatError> {
    table_object_branch_and_level(object).map(|_| ())
}

fn table_object_branch_and_level(object: &ObjectName) -> Result<(&str, u32), FormatError> {
    match ObjectLayout::classify_table_object(object) {
        Ok(Some(TableObjectClassification::Data {
            branch_id, level, ..
        })) => Ok((branch_id, level)),
        _ => Err(FormatError::InvalidValue {
            field: "table_object",
        }),
    }
}

fn validate_key_range(first: &[u8], last: &[u8], field: &'static str) -> Result<(), FormatError> {
    if first.is_empty()
        || last.is_empty()
        || first.len() > MAX_KEY_BYTES
        || last.len() > MAX_KEY_BYTES
    {
        return Err(FormatError::InvalidLength { field });
    }
    if first > last {
        return Err(FormatError::InvalidValue { field });
    }
    Ok(())
}

fn validate_section_kind(kind: &str) -> Result<(), FormatError> {
    if kind.is_empty() || kind.len() > MAX_SECTION_KIND_BYTES {
        return Err(FormatError::InvalidLength {
            field: "extension_kind",
        });
    }
    if !kind
        .bytes()
        .all(|byte| matches!(byte, b'a'..=b'z' | b'0'..=b'9' | b'.' | b'-' | b'_'))
    {
        return Err(FormatError::InvalidValue {
            field: "extension_kind",
        });
    }
    for (prefix, suffix) in [
        ("gra", "ph"),
        ("vec", "tor"),
        ("js", "on"),
        ("strata", "hub"),
        ("mer", "ge"),
        ("cher", "ry"),
        ("rev", "ert"),
    ] {
        if contains_joined(kind, prefix, suffix) {
            return Err(FormatError::InvalidValue {
                field: "extension_kind",
            });
        }
    }
    Ok(())
}

fn contains_joined(haystack: &str, prefix: &str, suffix: &str) -> bool {
    haystack
        .match_indices(prefix)
        .any(|(index, matched)| haystack[index + matched.len()..].starts_with(suffix))
}

fn minimum_manifest_len() -> usize {
    4 + 4 + BranchId::BYTE_LEN + 1 + 8 + 8 + 4 + 4 + 4 + 4
}

fn minimum_table_ref_len() -> usize {
    4 + 4 + 4 + 8 + 8 + 4 + 8 + 8 + 1 + 8 + 8 + 4 * 4 + 1
}

fn read_array_16(
    reader: &mut ByteReader<'_>,
    field: &'static str,
) -> Result<[u8; 16], FormatError> {
    let bytes = reader.read_exact(16)?;
    <[u8; 16]>::try_from(bytes).map_err(|_| FormatError::InvalidLength { field })
}

fn write_count(bytes: &mut Vec<u8>, field: &'static str, count: usize) -> Result<(), FormatError> {
    let count = u32::try_from(count).map_err(|_| FormatError::InvalidLength { field })?;
    bytes.extend_from_slice(&count.to_le_bytes());
    Ok(())
}

fn read_count(
    reader: &mut ByteReader<'_>,
    field: &'static str,
    max: usize,
) -> Result<usize, FormatError> {
    let count =
        usize::try_from(reader.read_u32_le()?).map_err(|_| FormatError::InvalidLength { field })?;
    if count > max {
        return Err(FormatError::InvalidLength { field });
    }
    Ok(count)
}

fn ensure_remaining_for_count(
    reader: &ByteReader<'_>,
    count: usize,
    minimum_item_len: usize,
    field: &'static str,
) -> Result<(), FormatError> {
    let minimum_needed = count
        .checked_mul(minimum_item_len)
        .ok_or(FormatError::InvalidLength { field })?;
    if minimum_needed > reader.remaining() {
        return Err(FormatError::InsufficientBytes {
            format: FORMAT,
            needed: minimum_needed,
            actual: reader.remaining(),
        });
    }
    Ok(())
}

fn write_optional_u64(
    bytes: &mut Vec<u8>,
    field: &'static str,
    value: Option<u64>,
) -> Result<(), FormatError> {
    validate_generation(value, field)?;
    if let Some(value) = value {
        bytes.push(1);
        bytes.extend_from_slice(&value.to_le_bytes());
    } else {
        bytes.push(0);
        bytes.extend_from_slice(&0_u64.to_le_bytes());
    }
    Ok(())
}

fn read_optional_u64(
    reader: &mut ByteReader<'_>,
    field: &'static str,
) -> Result<Option<u64>, FormatError> {
    let present = read_bool(reader, field)?;
    let value = reader.read_u64_le()?;
    if present {
        if value == 0 {
            return Err(FormatError::InvalidValue { field });
        }
        Ok(Some(value))
    } else if value == 0 {
        Ok(None)
    } else {
        Err(FormatError::InvalidValue { field })
    }
}

fn read_bool(reader: &mut ByteReader<'_>, field: &'static str) -> Result<bool, FormatError> {
    match reader.read_u8()? {
        0 => Ok(false),
        1 => Ok(true),
        value => Err(FormatError::InvalidBool { field, value }),
    }
}

fn write_string(
    bytes: &mut Vec<u8>,
    field: &'static str,
    value: &str,
    max_len: usize,
) -> Result<(), FormatError> {
    if value.is_empty() || value.len() > max_len {
        return Err(FormatError::InvalidLength { field });
    }
    write_count(bytes, field, value.len())?;
    bytes.extend_from_slice(value.as_bytes());
    Ok(())
}

fn read_bounded_string(
    reader: &mut ByteReader<'_>,
    field: &'static str,
    max_len: usize,
) -> Result<String, FormatError> {
    let bytes = read_bounded_bytes(reader, field, max_len)?;
    std::str::from_utf8(&bytes)
        .map_err(|_| FormatError::InvalidUtf8 { field })
        .map(str::to_owned)
}

fn write_bytes(
    bytes: &mut Vec<u8>,
    field: &'static str,
    value: &[u8],
    max_len: usize,
) -> Result<(), FormatError> {
    if value.len() > max_len {
        return Err(FormatError::InvalidLength { field });
    }
    write_count(bytes, field, value.len())?;
    bytes.extend_from_slice(value);
    Ok(())
}

fn read_bounded_bytes(
    reader: &mut ByteReader<'_>,
    field: &'static str,
    max_len: usize,
) -> Result<Vec<u8>, FormatError> {
    let len = read_count(reader, field, max_len)?;
    Ok(reader.read_exact(len)?.to_vec())
}

#[cfg(test)]
#[path = "table_manifest/tests.rs"]
mod tests;
