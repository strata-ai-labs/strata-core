use super::*;
#[path = "tests/canonical.rs"]
mod canonical;
#[path = "tests/constructor_object.rs"]
mod constructor_object;
#[path = "tests/corruption.rs"]
mod corruption;
#[path = "tests/inheritance_provenance_extension.rs"]
mod inheritance_provenance_extension;

#[derive(Clone, Copy, Debug)]
enum ManifestMutation {
    Magic,
    Version,
    Checksum,
    Count,
    ObjectName,
    Order,
    Range,
    Status,
    SectionFlags,
}

impl ManifestMutation {
    fn apply(self, bytes: &mut [u8], seed: u8) {
        match self {
            Self::Magic => bytes[0] = b'X',
            Self::Version => {
                bytes[4..8].copy_from_slice(&2_u32.to_le_bytes());
                refresh_crc(bytes);
            }
            Self::Checksum => {
                let last = bytes.len() - 1;
                bytes[last] ^= seed;
            }
            Self::Count => {
                write_u32(
                    bytes,
                    level_count_offset(),
                    bounded_u32(MAX_LEVELS) + u32::from(seed),
                );
                refresh_crc(bytes);
            }
            Self::ObjectName => {
                let marker = format!("tables/{}", branch(seed));
                let offset = find_bytes(bytes, marker.as_bytes());
                bytes[offset] = b'x';
                refresh_crc(bytes);
            }
            Self::Order => {
                let object = format!("tables/{}/l0000/g-l0", branch(seed));
                let offset = table_order_offset_after_object(bytes, &object);
                write_u32(bytes, offset, 9);
                refresh_crc(bytes);
            }
            Self::Range => {
                let offset = find_bytes(bytes, b"g-a");
                bytes[offset] = b'z';
                refresh_crc(bytes);
            }
            Self::Status => {
                let offset = find_bytes(bytes, branch(seed.wrapping_add(1)).as_bytes());
                let status_offset = offset + BranchId::BYTE_LEN + 1 + 8 + 8;
                bytes[status_offset] = 0xff;
                refresh_crc(bytes);
            }
            Self::SectionFlags => {
                let offset = find_extension_flag_index(bytes, "audit.fact");
                bytes[offset] = 0xff;
                refresh_crc(bytes);
            }
        }
    }
}

fn round_trip(manifest: &TableManifest) -> TableManifest {
    let bytes = encode_table_manifest(manifest).expect("encode");
    decode_table_manifest(&bytes).expect("decode")
}

fn manifest_with_single_table(table: TableManifestTableRef) -> TableManifest {
    TableManifest::new(
        branch(0x11),
        None,
        1,
        vec![TableManifestLevel::new(BranchLevel::ZERO, vec![table]).expect("level")],
        vec![],
        vec![],
    )
    .expect("manifest")
}

fn inherited_manifest() -> TableManifest {
    let first = inherited_layer_with_tables(
        0,
        branch(0x22),
        vec![
            TableManifestLevel::new(
                BranchLevel::ZERO,
                vec![table_ref(branch(0x22), "ancestor-l0", 0, b"a", b"b")],
            )
            .expect("ancestor l0"),
            TableManifestLevel::new(
                BranchLevel::new(1),
                vec![table_ref_at_level(
                    branch(0x22),
                    1,
                    "ancestor-l1",
                    0,
                    b"m",
                    b"n",
                )],
            )
            .expect("ancestor l1"),
        ],
    );
    let second = inherited_layer(
        1,
        branch(0x33),
        CommitVersion::new(20),
        TableManifestInheritedLayerStatus::Materializing,
    );
    TableManifest::new(
        branch(0x11),
        Some(3),
        7,
        vec![],
        vec![second, first],
        vec![],
    )
    .expect("inherited manifest")
}

fn extension_manifest() -> TableManifest {
    TableManifest::new(
        branch(0x11),
        None,
        1,
        vec![],
        vec![],
        vec![
            TableManifestExtensionSection::optional("audit.fact", true, b"abc".to_vec())
                .expect("extension"),
        ],
    )
    .expect("manifest")
}

fn generated_manifest(seed: u8) -> TableManifest {
    let owner = branch(seed);
    let source = branch(seed.wrapping_add(1));
    let inherited = TableManifestInheritedLayer::new(
        0,
        source,
        Some(u64::from(seed) + 1),
        CommitVersion::new(u64::from(seed) + 10),
        TableManifestInheritedLayerStatus::Active,
        vec![TableManifestLevel::new(
            BranchLevel::ZERO,
            vec![table_ref(source, "g-inherited", 0, b"i-a", b"i-b")],
        )
        .expect("inherited level")],
    )
    .expect("inherited layer");
    TableManifest::new(
        owner,
        Some(u64::from(seed) + 2),
        u64::from(seed) + 3,
        vec![
            TableManifestLevel::new(
                BranchLevel::ZERO,
                vec![table_ref(owner, "g-l0", 0, b"g-a", b"g-b")],
            )
            .expect("l0"),
            TableManifestLevel::new(
                BranchLevel::new(1),
                vec![table_ref_at_level(owner, 1, "g-l1", 0, b"m-a", b"m-b")],
            )
            .expect("l1"),
        ],
        vec![inherited],
        vec![
            TableManifestExtensionSection::optional("audit.fact", true, [seed]).expect("extension"),
        ],
    )
    .expect("generated manifest")
}

fn inherited_layer(
    order: u32,
    source: BranchId,
    fork_version: CommitVersion,
    status: TableManifestInheritedLayerStatus,
) -> TableManifestInheritedLayer {
    inherited_layer_with_tables(
        order,
        source,
        vec![TableManifestLevel::new(
            BranchLevel::ZERO,
            vec![table_ref(source, &format!("layer-{order}"), 0, b"a", b"b")],
        )
        .expect("level")],
    )
    .with_status(status)
    .with_fork_version(fork_version)
}

fn inherited_layer_with_tables(
    order: u32,
    source: BranchId,
    levels: Vec<TableManifestLevel>,
) -> TableManifestInheritedLayer {
    TableManifestInheritedLayer::new(
        order,
        source,
        Some(u64::from(order) + 1),
        CommitVersion::new(u64::from(order + 1) * 10),
        TableManifestInheritedLayerStatus::Active,
        levels,
    )
    .expect("inherited layer")
}

trait TestLayerMutation {
    fn with_status(self, status: TableManifestInheritedLayerStatus) -> Self;
    fn with_fork_version(self, fork_version: CommitVersion) -> Self;
}

impl TestLayerMutation for TableManifestInheritedLayer {
    fn with_status(self, status: TableManifestInheritedLayerStatus) -> Self {
        TableManifestInheritedLayer::new(
            self.order(),
            self.source_branch_id(),
            self.source_branch_generation(),
            self.fork_version(),
            status,
            self.levels().to_vec(),
        )
        .expect("status mutation")
    }

    fn with_fork_version(self, fork_version: CommitVersion) -> Self {
        TableManifestInheritedLayer::new(
            self.order(),
            self.source_branch_id(),
            self.source_branch_generation(),
            fork_version,
            self.status(),
            self.levels().to_vec(),
        )
        .expect("fork mutation")
    }
}

fn decoded_level(level: BranchLevel, tables: Vec<TableManifestTableRef>) -> TableManifestLevel {
    TableManifestLevel::from_decoded(level, tables).expect("decoded level")
}

fn assert_inherited_status_round_trips(status: TableManifestInheritedLayerStatus) {
    let layer = inherited_layer(0, branch(0x22), CommitVersion::new(10), status);
    let manifest =
        TableManifest::new(branch(0x11), None, 1, vec![], vec![layer], vec![]).expect("manifest");

    assert_eq!(round_trip(&manifest).inherited_layers()[0].status(), status);
}

fn assert_provenance_round_trips(provenance: &TableManifestTableProvenance) {
    let table = table_ref_with_provenance(
        branch(0x11),
        0,
        "provenance",
        0,
        b"a",
        b"b",
        provenance.clone(),
    );
    let manifest = manifest_with_single_table(table);

    assert_eq!(
        round_trip(&manifest).levels()[0].tables()[0].provenance(),
        provenance
    );
}

fn assert_table_object_rejected(object: &str) {
    assert!(matches!(
        TableManifestTableRef::new(
            TableIdentity::new("object").expect("identity"),
            ObjectName::new(object).expect("object"),
            0,
            table_facts(),
            table_bounds(b"a", b"b"),
            TableManifestTableProvenance::Flush,
        ),
        Err(FormatError::InvalidValue {
            field: "table_object"
        })
    ));
}

#[allow(clippy::needless_pass_by_value)]
fn assert_invalid_value<T: std::fmt::Debug>(
    result: Result<T, FormatError>,
    expected_field: &'static str,
) {
    assert!(
        matches!(result, Err(FormatError::InvalidValue { field }) if field == expected_field),
        "expected InvalidValue({expected_field}), got {result:?}",
    );
}

#[allow(clippy::needless_pass_by_value)]
fn assert_invalid_length<T: std::fmt::Debug>(
    result: Result<T, FormatError>,
    expected_field: &'static str,
) {
    assert!(
        matches!(result, Err(FormatError::InvalidLength { field }) if field == expected_field),
        "expected InvalidLength({expected_field}), got {result:?}",
    );
}

fn sample_manifest() -> TableManifest {
    let owned = TableManifestLevel::new(
        BranchLevel::ZERO,
        vec![
            table_ref(branch(0x11), "l0b", 1, b"b0", b"b9"),
            table_ref(branch(0x11), "l0a", 0, b"a0", b"a9"),
        ],
    )
    .expect("owned level");
    let inherited_level = TableManifestLevel::new(
        BranchLevel::new(1),
        vec![table_ref_at_level(
            branch(0x22),
            1,
            "inherited",
            0,
            b"p0",
            b"p9",
        )],
    )
    .expect("inherited level");
    let inherited = TableManifestInheritedLayer::new(
        0,
        branch(0x22),
        Some(9),
        CommitVersion::new(7),
        TableManifestInheritedLayerStatus::Materializing,
        vec![inherited_level],
    )
    .expect("inherited layer");
    TableManifest::new(
        branch(0x11),
        Some(3),
        1,
        vec![owned],
        vec![inherited],
        vec![
            TableManifestExtensionSection::optional("audit.fact", true, b"abc".to_vec())
                .expect("extension"),
        ],
    )
    .expect("manifest")
}

fn table_ref(
    branch: BranchId,
    identity: &str,
    order: u32,
    physical_first: &[u8],
    physical_last: &[u8],
) -> TableManifestTableRef {
    table_ref_at_level(branch, 0, identity, order, physical_first, physical_last)
}

fn table_ref_at_level(
    branch: BranchId,
    level: u8,
    identity: &str,
    order: u32,
    physical_first: &[u8],
    physical_last: &[u8],
) -> TableManifestTableRef {
    let object = format!("tables/{branch}/l{level:04}/{identity}");
    table_ref_with_object(identity, &object, order, physical_first, physical_last)
}

fn table_ref_with_object(
    identity: &str,
    object: &str,
    order: u32,
    physical_first: &[u8],
    physical_last: &[u8],
) -> TableManifestTableRef {
    let facts = table_facts();
    let bounds = table_bounds(physical_first, physical_last);
    let provenance = if identity == "inherited" {
        TableManifestTableProvenance::materialization_replacement(
            branch(0x33),
            CommitVersion::new(6),
        )
        .expect("provenance")
    } else {
        TableManifestTableProvenance::Flush
    };
    TableManifestTableRef::new(
        TableIdentity::new(identity).expect("identity"),
        ObjectName::new(object).expect("object"),
        order,
        facts,
        bounds,
        provenance,
    )
    .expect("table ref")
}

fn table_ref_with_provenance(
    branch: BranchId,
    level: u8,
    identity: &str,
    order: u32,
    physical_first: &[u8],
    physical_last: &[u8],
    provenance: TableManifestTableProvenance,
) -> TableManifestTableRef {
    let object = format!("tables/{branch}/l{level:04}/{identity}");
    TableManifestTableRef::new(
        TableIdentity::new(identity).expect("identity"),
        ObjectName::new(object).expect("object"),
        order,
        table_facts(),
        table_bounds(physical_first, physical_last),
        provenance,
    )
    .expect("table ref")
}

fn table_facts() -> TableManifestTableFacts {
    TableManifestTableFacts::new(
        128,
        4,
        1,
        CommitVersion::new(1),
        CommitVersion::new(4),
        Some(Timestamp::from_micros(10)),
        Some(Timestamp::from_micros(40)),
    )
    .expect("facts")
}

fn table_bounds(physical_first: &[u8], physical_last: &[u8]) -> TableManifestTableBounds {
    TableManifestTableBounds::new(
        physical_first.to_vec(),
        physical_last.to_vec(),
        [physical_first, b":i0"].concat(),
        [physical_last, b":i9"].concat(),
    )
    .expect("bounds")
}

fn branch(byte: u8) -> BranchId {
    BranchId::from_bytes([byte; BranchId::BYTE_LEN])
}

fn refresh_crc(bytes: &mut [u8]) {
    let checksum_offset = bytes.len() - 4;
    let crc = crc32fast::hash(&bytes[..checksum_offset]);
    bytes[checksum_offset..].copy_from_slice(&crc.to_le_bytes());
}

fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn bounded_u32(value: usize) -> u32 {
    u32::try_from(value).expect("test bound fits in u32")
}

fn remove_payload_byte(bytes: &mut Vec<u8>, offset: usize) {
    assert!(offset < bytes.len() - 4);
    bytes.remove(offset);
}

fn level_count_offset() -> usize {
    4 + 4 + BranchId::BYTE_LEN + 1 + 8 + 8
}

fn inherited_count_offset() -> usize {
    level_count_offset() + 4
}

fn extension_count_offset() -> usize {
    inherited_count_offset() + 4
}

fn find_bytes(bytes: &[u8], needle: &[u8]) -> usize {
    bytes
        .windows(needle.len())
        .position(|window| window == needle)
        .expect("needle in bytes")
}

fn find_len_prefixed_string(bytes: &[u8], needle: &str) -> usize {
    let value_offset = find_bytes(bytes, needle.as_bytes());
    value_offset - 4
}

fn table_order_offset_after_object(bytes: &[u8], object: &str) -> usize {
    find_bytes(bytes, object.as_bytes()) + object.len()
}

fn table_provenance_offset_after_object(bytes: &[u8], object: &str) -> usize {
    let mut offset = table_order_offset_after_object(bytes, object) + 4;
    offset += 8 + 8 + 4 + 8 + 8 + 1 + 8 + 8;
    for _ in 0..4 {
        let len = usize::try_from(u32::from_le_bytes(
            bytes[offset..offset + 4].try_into().expect("length prefix"),
        ))
        .expect("u32 length fits usize");
        offset += 4 + len;
    }
    offset
}

fn find_extension_flag_index(bytes: &[u8], needle: &str) -> usize {
    bytes
        .windows(needle.len())
        .position(|window| window == needle.as_bytes())
        .map(|start| start + needle.len())
        .expect("extension kind")
}
