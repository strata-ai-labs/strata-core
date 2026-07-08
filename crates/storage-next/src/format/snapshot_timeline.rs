//! W3.1b: the checkpoint snapshot's retained-timeline section.
//!
//! One section per checkpoint, holding per-branch `(commit_version,
//! commit_timestamp)` groups — the persisted form of the retained-timeline
//! index (`timeline_index.rs`), written only for branches whose index is
//! complete at checkpoint time and restored at recovery so reopen never
//! rescans the timeline space. Layout (all little-endian):
//!
//! ```text
//! group_count: u32
//! repeated group:
//!   branch_id:   16 bytes
//!   entry_count: u32
//!   repeated entry: commit_version: u64, commit_timestamp_micros: u64
//! ```
//!
//! Entries within a group MUST be strictly ascending by commit version —
//! validated at decode, so a corrupt section fails closed rather than seeding
//! a wrong index.

use super::{FormatError, SnapshotSection};
use strata_core_next::{BranchId, CommitVersion, Timestamp};

pub(crate) const SNAPSHOT_TIMELINE_SECTION_KIND: u8 = 2;

const GROUP_HEADER_BYTES: usize = BranchId::BYTE_LEN + 4;
const ENTRY_BYTES: usize = 16;
/// Fail-closed ceiling mirroring the row section's materialization guard: a
/// section that decode would reject is never written.
const MAX_TIMELINE_ENTRIES: usize = 1 << 28;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SnapshotTimelineBranchGroup {
    pub(crate) branch_id: BranchId,
    pub(crate) entries: Vec<(CommitVersion, Timestamp)>,
}

pub(crate) fn encode_snapshot_timeline_section(
    groups: &[SnapshotTimelineBranchGroup],
) -> Result<SnapshotSection, FormatError> {
    let group_count = u32::try_from(groups.len()).map_err(|_| FormatError::InvalidLength {
        field: "timeline_group_count",
    })?;
    let mut payload = Vec::new();
    payload.extend_from_slice(&group_count.to_le_bytes());
    for group in groups {
        validate_group_entries(&group.entries)?;
        let entry_count =
            u32::try_from(group.entries.len()).map_err(|_| FormatError::InvalidLength {
                field: "timeline_entry_count",
            })?;
        payload.extend_from_slice(group.branch_id.as_bytes());
        payload.extend_from_slice(&entry_count.to_le_bytes());
        for (version, timestamp) in &group.entries {
            payload.extend_from_slice(&version.as_u64().to_le_bytes());
            payload.extend_from_slice(&timestamp.as_micros().to_le_bytes());
        }
    }
    SnapshotSection::new(SNAPSHOT_TIMELINE_SECTION_KIND, payload)
}

pub(crate) fn decode_snapshot_timeline_payload(
    payload: &[u8],
) -> Result<Vec<SnapshotTimelineBranchGroup>, FormatError> {
    let mut offset = 0usize;
    let group_count = read_u32(payload, &mut offset, "timeline_group_count")?;
    let mut groups = Vec::new();
    let mut total_entries = 0usize;
    for _ in 0..group_count {
        if payload.len() < offset.saturating_add(GROUP_HEADER_BYTES) {
            return Err(FormatError::InsufficientBytes {
                format: "snapshot_timeline_section",
                needed: offset.saturating_add(GROUP_HEADER_BYTES),
                actual: payload.len(),
            });
        }
        let mut branch_bytes = [0u8; BranchId::BYTE_LEN];
        branch_bytes.copy_from_slice(&payload[offset..offset + BranchId::BYTE_LEN]);
        offset += BranchId::BYTE_LEN;
        let entry_count = read_u32(payload, &mut offset, "timeline_entry_count")? as usize;
        total_entries =
            total_entries
                .checked_add(entry_count)
                .ok_or(FormatError::InvalidLength {
                    field: "timeline_entry_count",
                })?;
        if total_entries > MAX_TIMELINE_ENTRIES {
            return Err(FormatError::InvalidLength {
                field: "timeline_entry_count",
            });
        }
        let entries_end = offset
            .checked_add(entry_count.saturating_mul(ENTRY_BYTES))
            .ok_or(FormatError::InvalidLength {
                field: "timeline_entry_count",
            })?;
        if payload.len() < entries_end {
            return Err(FormatError::InsufficientBytes {
                format: "snapshot_timeline_section",
                needed: entries_end,
                actual: payload.len(),
            });
        }
        let mut entries = Vec::with_capacity(entry_count);
        for _ in 0..entry_count {
            let version = read_u64(payload, &mut offset, "timeline_commit_version")?;
            let timestamp = read_u64(payload, &mut offset, "timeline_commit_timestamp")?;
            entries.push((
                CommitVersion::new(version),
                Timestamp::from_micros(timestamp),
            ));
        }
        validate_group_entries(&entries)?;
        groups.push(SnapshotTimelineBranchGroup {
            branch_id: BranchId::from_bytes(branch_bytes),
            entries,
        });
    }
    if offset != payload.len() {
        return Err(FormatError::InvalidLength {
            field: "timeline_section_trailing_bytes",
        });
    }
    Ok(groups)
}

fn validate_group_entries(entries: &[(CommitVersion, Timestamp)]) -> Result<(), FormatError> {
    let ascending = entries
        .windows(2)
        .all(|pair| pair[0].0.as_u64() < pair[1].0.as_u64());
    if !ascending {
        return Err(FormatError::InvalidValue {
            field: "timeline_entry_order",
        });
    }
    if entries
        .iter()
        .any(|(version, _)| *version == CommitVersion::ZERO)
    {
        return Err(FormatError::InvalidValue {
            field: "timeline_commit_version",
        });
    }
    Ok(())
}

fn read_u32(payload: &[u8], offset: &mut usize, field: &'static str) -> Result<u32, FormatError> {
    let end = offset
        .checked_add(4)
        .ok_or(FormatError::InvalidLength { field })?;
    if payload.len() < end {
        return Err(FormatError::InsufficientBytes {
            format: "snapshot_timeline_section",
            needed: end,
            actual: payload.len(),
        });
    }
    let mut bytes = [0u8; 4];
    bytes.copy_from_slice(&payload[*offset..end]);
    *offset = end;
    Ok(u32::from_le_bytes(bytes))
}

fn read_u64(payload: &[u8], offset: &mut usize, field: &'static str) -> Result<u64, FormatError> {
    let end = offset
        .checked_add(8)
        .ok_or(FormatError::InvalidLength { field })?;
    if payload.len() < end {
        return Err(FormatError::InsufficientBytes {
            format: "snapshot_timeline_section",
            needed: end,
            actual: payload.len(),
        });
    }
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&payload[*offset..end]);
    *offset = end;
    Ok(u64::from_le_bytes(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn branch(byte: u8) -> BranchId {
        BranchId::from_bytes([byte; BranchId::BYTE_LEN])
    }

    fn group(byte: u8, entries: &[(u64, u64)]) -> SnapshotTimelineBranchGroup {
        SnapshotTimelineBranchGroup {
            branch_id: branch(byte),
            entries: entries
                .iter()
                .map(|(version, ts)| (CommitVersion::new(*version), Timestamp::from_micros(*ts)))
                .collect(),
        }
    }

    #[test]
    fn timeline_section_round_trips_multi_branch_groups() {
        let groups = vec![
            group(1, &[(1, 10), (2, 20), (5, 21)]),
            group(2, &[]),
            group(3, &[(7, 70)]),
        ];
        let section = encode_snapshot_timeline_section(&groups).expect("encode");
        assert_eq!(section.section_kind(), SNAPSHOT_TIMELINE_SECTION_KIND);
        let decoded = decode_snapshot_timeline_payload(section.payload()).expect("decode");
        assert_eq!(decoded, groups);
    }

    #[test]
    fn timeline_section_rejects_disorder_zero_versions_and_trailing_bytes() {
        assert!(encode_snapshot_timeline_section(&[group(1, &[(2, 20), (1, 10)])]).is_err());
        assert!(encode_snapshot_timeline_section(&[group(1, &[(0, 0)])]).is_err());

        let section = encode_snapshot_timeline_section(&[group(1, &[(1, 10)])]).expect("encode");
        let mut payload = section.payload().to_vec();
        payload.push(0);
        assert!(decode_snapshot_timeline_payload(&payload).is_err());
        assert!(decode_snapshot_timeline_payload(&payload[..payload.len() - 3]).is_err());
    }
}
