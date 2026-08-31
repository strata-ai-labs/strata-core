//! Fail-closed decoder entry points for the fuzz targets — upgraded by
//! TCP4.6b from crash-only to **round-trip fidelity**: when arbitrary bytes
//! DECODE, the decoded value must survive re-encode + re-decode identically.
//! A round-trip mismatch (or a re-decode failure of freshly-encoded bytes)
//! PANICS — under libFuzzer a panic is a finding, while returning `false` is
//! just a rejection the target discards, which made even a lossy codec
//! invisible to the pre-4.6b targets. Value loss now fails a target, not just
//! panics (the #2688/#2689 class, at the codec layer).
//!
//! Encode refusal of a decoded value is decoder-leniency asymmetry (the
//! decoder accepted bytes describing a value the system could never write),
//! not value loss — recorded as a rejection pending a per-format audit.
//!
//! Exempt from round-trip, fail-closed only (each with its own oracle):
//! `snapshot_envelope` (three structural probes, no single value),
//! `table_artifact`/`table_block`/`table_block_trusted` (framed containers
//! whose encoders are write-path-coupled; `table_block_trusted` carries the
//! trusted-⊇-checked acceptance oracle), `table_block_indexed_seek` (a seek
//! probe over derived offsets), and the WAL record *envelope* half of
//! `decode_wal_record` (its encoder takes write-path context; the record half
//! round-trips).

use std::convert::Infallible;

use super::{
    branch_catalog_manifest, key, manifest, pending_releases_manifest, quarantine,
    retained_history_extension, segment_metadata, snapshot, snapshot_rows, snapshot_timeline,
    storage_row, table, table_manifest, wal, watermark,
};

/// The round-trip fidelity oracle (TCP4.6b). `decoded` came from arbitrary
/// input bytes; re-encode it and decode again — the value must be identical.
fn roundtrip<T, EncodeError, DecodeError>(
    decoded: &T,
    encode: impl FnOnce(&T) -> Result<Vec<u8>, EncodeError>,
    decode: impl FnOnce(&[u8]) -> Result<T, DecodeError>,
) -> bool
where
    T: PartialEq + std::fmt::Debug,
    DecodeError: std::fmt::Debug,
{
    let Ok(encoded) = encode(decoded) else {
        // Decoder-leniency asymmetry, not value loss: a rejection, not a finding.
        return false;
    };
    match decode(&encoded) {
        Ok(second) if &second == decoded => true,
        Ok(second) => {
            panic!("codec round-trip changed the value:\n first: {decoded:?}\nsecond: {second:?}")
        }
        Err(error) => {
            panic!("re-encoded bytes failed to decode: {error:?}\n value: {decoded:?}")
        }
    }
}

pub(crate) fn decode_key(bytes: &[u8]) -> bool {
    let physical = match key::decode_physical_key(bytes) {
        Ok(value) => roundtrip(
            &value,
            |k| Ok::<_, Infallible>(key::encode_physical_key(k)),
            key::decode_physical_key,
        ),
        Err(_) => false,
    };
    let internal = match key::decode_internal_key(bytes) {
        Ok(value) => roundtrip(
            &value,
            |k| Ok::<_, Infallible>(key::encode_internal_key(k)),
            key::decode_internal_key,
        ),
        Err(_) => false,
    };
    physical || internal
}

pub(crate) fn decode_manifest(bytes: &[u8]) -> bool {
    match manifest::decode_manifest(bytes) {
        Ok(value) => roundtrip(&value, manifest::encode_manifest, manifest::decode_manifest),
        Err(_) => false,
    }
}

pub(crate) fn decode_branch_catalog_manifest(bytes: &[u8]) -> bool {
    match branch_catalog_manifest::decode_branch_catalog_manifest(bytes) {
        Ok(value) => roundtrip(
            &value,
            branch_catalog_manifest::encode_branch_catalog_manifest,
            branch_catalog_manifest::decode_branch_catalog_manifest,
        ),
        Err(_) => false,
    }
}

pub(crate) fn decode_pending_releases_manifest(bytes: &[u8]) -> bool {
    match pending_releases_manifest::decode_pending_releases_manifest(bytes) {
        Ok(value) => roundtrip(
            &value,
            pending_releases_manifest::encode_pending_releases_manifest,
            pending_releases_manifest::decode_pending_releases_manifest,
        ),
        Err(_) => false,
    }
}

pub(crate) fn decode_quarantine_inventory(bytes: &[u8]) -> bool {
    match quarantine::decode_quarantine_inventory(bytes) {
        Ok(value) => roundtrip(
            &value,
            quarantine::encode_quarantine_inventory,
            quarantine::decode_quarantine_inventory,
        ),
        Err(_) => false,
    }
}

pub(crate) fn decode_segment_metadata(bytes: &[u8]) -> bool {
    match segment_metadata::decode_segment_metadata(bytes) {
        Ok(value) => roundtrip(
            &value,
            |metadata| Ok::<_, Infallible>(segment_metadata::encode_segment_metadata(metadata)),
            segment_metadata::decode_segment_metadata,
        ),
        Err(_) => false,
    }
}

pub(crate) fn decode_snapshot_envelope(bytes: &[u8]) -> bool {
    let header = snapshot::decode_snapshot_header(bytes).is_ok();
    let section = snapshot::decode_snapshot_section_ref(bytes).is_ok();
    let container = snapshot::visit_snapshot_container_sections(bytes, 4096, |_| Ok(())).is_ok();
    header || section || container
}

pub(crate) fn decode_snapshot_row_payload(bytes: &[u8]) -> bool {
    match snapshot_rows::decode_snapshot_row_payload(bytes) {
        Ok(rows) => roundtrip(
            &rows,
            |rows| {
                snapshot_rows::encode_snapshot_row_section(rows)
                    .map(|section| section.payload().to_vec())
            },
            snapshot_rows::decode_snapshot_row_payload,
        ),
        Err(_) => false,
    }
}

pub(crate) fn decode_snapshot_timeline_payload(bytes: &[u8]) -> bool {
    match snapshot_timeline::decode_snapshot_timeline_payload(bytes) {
        Ok(groups) => roundtrip(
            &groups,
            |groups| {
                snapshot_timeline::encode_snapshot_timeline_section(groups)
                    .map(|section| section.payload().to_vec())
            },
            snapshot_timeline::decode_snapshot_timeline_payload,
        ),
        Err(_) => false,
    }
}

pub(crate) fn decode_retained_history_extension_payload(bytes: &[u8]) -> bool {
    match retained_history_extension::decode_retained_history_extension_payload(bytes) {
        Ok(value) => roundtrip(
            &value,
            |payload| {
                Ok::<_, Infallible>(
                    retained_history_extension::encode_retained_history_extension_payload(*payload),
                )
            },
            retained_history_extension::decode_retained_history_extension_payload,
        ),
        Err(_) => false,
    }
}

pub(crate) fn decode_storage_row(bytes: &[u8]) -> bool {
    match storage_row::decode_storage_row(bytes) {
        Ok(row) => roundtrip(
            &row,
            storage_row::encode_storage_row,
            storage_row::decode_storage_row,
        ),
        Err(_) => false,
    }
}

pub(crate) fn decode_table_artifact(bytes: &[u8]) -> bool {
    table::decode_immutable_table(bytes).is_ok()
}

pub(crate) fn decode_table_block(bytes: &[u8]) -> bool {
    table::decode_table_block_frame(bytes).is_ok_and(|(_frame, consumed)| consumed == bytes.len())
}

/// W2.3 (B3): the indexed point seek must never panic on arbitrary
/// (payload, offsets) inputs — offsets are a cached derived artifact, so the
/// seek's defensive validation is the only thing between garbage and UB. The
/// first two bytes select the payload/offsets split.
pub(crate) fn decode_table_block_indexed_seek(bytes: &[u8]) -> bool {
    if bytes.len() < 3 {
        return false;
    }
    let split = usize::from(u16::from_le_bytes([bytes[0], bytes[1]]));
    let rest = &bytes[2..];
    let split = split % rest.len();
    let (payload, offset_bytes) = rest.split_at(split);
    // Feed the raw fuzz bytes straight into the borrowed view — the probe
    // must reject or succeed without panicking on ANY byte shape.
    let Some(view) = table::EntryOffsetsView::new(&offset_bytes[..offset_bytes.len() / 4 * 4])
    else {
        return false;
    };
    table::seek_table_data_block_point_indexed(
        payload,
        view,
        b"fuzz-seek-key",
        b"fuzz-target",
        None,
        None,
    )
    .is_ok()
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
    match table_manifest::decode_table_manifest(bytes) {
        Ok(value) => roundtrip(
            &value,
            table_manifest::encode_table_manifest,
            table_manifest::decode_table_manifest,
        ),
        Err(_) => false,
    }
}

pub(crate) fn decode_wal_commit_payload(bytes: &[u8]) -> bool {
    match wal::decode_wal_commit_payload(bytes) {
        Ok(value) => roundtrip(
            &value,
            wal::encode_wal_commit_payload,
            wal::decode_wal_commit_payload,
        ),
        Err(_) => false,
    }
}

pub(crate) fn decode_wal_record(bytes: &[u8]) -> bool {
    let record = match wal::decode_wal_record(bytes) {
        Ok((value, _consumed)) => roundtrip(&value, wal::encode_wal_record, |encoded| {
            wal::decode_wal_record(encoded).map(|(record, _consumed)| record)
        }),
        Err(_) => false,
    };
    let envelope = wal::decode_wal_record_envelope(bytes).is_ok();
    record || envelope
}

pub(crate) fn decode_wal_segment_header(bytes: &[u8]) -> bool {
    match wal::decode_wal_segment_header(bytes, None) {
        Ok((value, _consumed)) => roundtrip(
            &value,
            |header| Ok::<_, Infallible>(wal::encode_wal_segment_header(header)),
            |encoded| wal::decode_wal_segment_header(encoded, None).map(|(header, _)| header),
        ),
        Err(_) => false,
    }
}

pub(crate) fn decode_watermark(bytes: &[u8]) -> bool {
    match watermark::decode_snapshot_watermark(bytes) {
        Ok(value) => roundtrip(
            &value,
            |mark| watermark::encode_snapshot_watermark(*mark),
            watermark::decode_snapshot_watermark,
        ),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::roundtrip;

    /// A faithful fake codec: one `u16`, little-endian.
    fn decode_u16(bytes: &[u8]) -> Result<u16, &'static str> {
        let pair: [u8; 2] = bytes.try_into().map_err(|_| "length")?;
        Ok(u16::from_le_bytes(pair))
    }

    #[test]
    fn roundtrip_accepts_a_faithful_codec() {
        assert!(roundtrip(
            &7_u16,
            |value| Ok::<_, &'static str>(value.to_le_bytes().to_vec()),
            decode_u16,
        ));
    }

    #[test]
    fn roundtrip_records_encode_refusal_as_rejection() {
        assert!(!roundtrip(
            &7_u16,
            |_| Err::<Vec<u8>, _>("encoder refuses"),
            decode_u16,
        ));
    }

    #[test]
    #[should_panic(expected = "codec round-trip changed the value")]
    fn roundtrip_panics_when_the_codec_loses_value() {
        // The #2689 shape at the codec layer: encode silently flushes the
        // value to zero. The oracle must PANIC (a fuzz finding), not reject.
        let _ = roundtrip(
            &7_u16,
            |_| Ok::<_, &'static str>(0_u16.to_le_bytes().to_vec()),
            decode_u16,
        );
    }

    #[test]
    #[should_panic(expected = "re-encoded bytes failed to decode")]
    fn roundtrip_panics_when_reencoded_bytes_fail_to_decode() {
        let _ = roundtrip(
            &7_u16,
            |_| Ok::<_, &'static str>(vec![0xff]), // wrong length: decode rejects
            decode_u16,
        );
    }
}
