use super::{
    branch_catalog_manifest, key, manifest, pending_releases_manifest, quarantine,
    retained_history_extension, segment_metadata, snapshot, snapshot_rows, storage_row, table,
    table_manifest, wal, watermark,
};

pub(crate) fn decode_key(bytes: &[u8]) -> bool {
    let physical_key = key::decode_physical_key(bytes).is_ok();
    let internal_key = key::decode_internal_key(bytes).is_ok();
    physical_key || internal_key
}

pub(crate) fn decode_manifest(bytes: &[u8]) -> bool {
    manifest::decode_manifest(bytes).is_ok()
}

pub(crate) fn decode_branch_catalog_manifest(bytes: &[u8]) -> bool {
    branch_catalog_manifest::decode_branch_catalog_manifest(bytes).is_ok()
}

pub(crate) fn decode_pending_releases_manifest(bytes: &[u8]) -> bool {
    pending_releases_manifest::decode_pending_releases_manifest(bytes).is_ok()
}

pub(crate) fn decode_quarantine_inventory(bytes: &[u8]) -> bool {
    quarantine::decode_quarantine_inventory(bytes).is_ok()
}

pub(crate) fn decode_segment_metadata(bytes: &[u8]) -> bool {
    segment_metadata::decode_segment_metadata(bytes).is_ok()
}

pub(crate) fn decode_snapshot_envelope(bytes: &[u8]) -> bool {
    let header = snapshot::decode_snapshot_header(bytes).is_ok();
    let section = snapshot::decode_snapshot_section_ref(bytes).is_ok();
    let container = snapshot::visit_snapshot_container_sections(bytes, 4096, |_| Ok(())).is_ok();
    header || section || container
}

pub(crate) fn decode_snapshot_row_payload(bytes: &[u8]) -> bool {
    snapshot_rows::decode_snapshot_row_payload(bytes).is_ok()
}

pub(crate) fn decode_snapshot_timeline_payload(bytes: &[u8]) -> bool {
    super::decode_snapshot_timeline_payload(bytes).is_ok()
}

pub(crate) fn decode_retained_history_extension_payload(bytes: &[u8]) -> bool {
    retained_history_extension::decode_retained_history_extension_payload(bytes).is_ok()
}

pub(crate) fn decode_storage_row(bytes: &[u8]) -> bool {
    storage_row::decode_storage_row(bytes).is_ok()
}

pub(crate) fn decode_table_artifact(bytes: &[u8]) -> bool {
    table::decode_immutable_table(bytes).is_ok()
}

pub(crate) fn decode_table_block(bytes: &[u8]) -> bool {
    table::decode_table_block_frame(bytes).is_ok_and(|(_frame, consumed)| consumed == bytes.len())
}

/// W2.6 (B1): the trusted (no-CRC) block decode must never panic on arbitrary
/// bytes, and must accept AT LEAST everything the checked decode accepts (it
/// relaxes only the checksum, never a structural check).
pub(crate) fn decode_table_block_trusted(bytes: &[u8]) -> bool {
    let trusted = table::decode_table_block_frame_trusted(bytes, table::TableBlockKind::Data)
        .is_ok_and(|(_frame, consumed)| consumed == bytes.len());
    let checked = table::decode_table_block_frame_as(bytes, table::TableBlockKind::Data)
        .is_ok_and(|(_frame, consumed)| consumed == bytes.len());
    assert!(
        trusted || !checked,
        "checked decode accepted a data frame the trusted decode rejected"
    );
    trusted
}

pub(crate) fn decode_table_manifest(bytes: &[u8]) -> bool {
    let Ok(manifest) = table_manifest::decode_table_manifest(bytes) else {
        return false;
    };
    let Ok(encoded) = table_manifest::encode_table_manifest(&manifest) else {
        return false;
    };
    table_manifest::decode_table_manifest(&encoded).is_ok_and(|decoded| decoded == manifest)
}

pub(crate) fn decode_wal_commit_payload(bytes: &[u8]) -> bool {
    wal::decode_wal_commit_payload(bytes).is_ok()
}

pub(crate) fn decode_wal_record(bytes: &[u8]) -> bool {
    let record = wal::decode_wal_record(bytes).is_ok();
    let envelope = wal::decode_wal_record_envelope(bytes).is_ok();
    record || envelope
}

pub(crate) fn decode_wal_segment_header(bytes: &[u8]) -> bool {
    wal::decode_wal_segment_header(bytes, None).is_ok()
}

pub(crate) fn decode_watermark(bytes: &[u8]) -> bool {
    watermark::decode_snapshot_watermark(bytes).is_ok()
}
