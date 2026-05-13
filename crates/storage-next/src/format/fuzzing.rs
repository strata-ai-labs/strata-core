use super::{key, manifest, segment_metadata, snapshot, storage_row, wal, watermark};

pub(crate) fn decode_key(bytes: &[u8]) -> bool {
    let physical_key = key::decode_physical_key(bytes).is_ok();
    let internal_key = key::decode_internal_key(bytes).is_ok();
    physical_key || internal_key
}

pub(crate) fn decode_manifest(bytes: &[u8]) -> bool {
    manifest::decode_manifest(bytes).is_ok()
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

pub(crate) fn decode_storage_row(bytes: &[u8]) -> bool {
    storage_row::decode_storage_row(bytes).is_ok()
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
