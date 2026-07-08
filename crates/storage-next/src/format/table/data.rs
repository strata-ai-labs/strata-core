use super::super::{
    key::{decode_internal_key, encode_internal_key},
    storage_row::{
        decode_storage_row, encode_storage_row, encode_storage_row_with_physical_key_bytes_into,
    },
    ByteReader, FormatError,
};
use super::index::TableIndexEntry;
use super::{MAX_TABLE_BLOCK_ENTRIES, MAX_TABLE_KEY_BYTES, MAX_TABLE_ROW_BYTES};
use crate::row::{InternalKey, StorageRow};
use strata_core_next::{CommitVersion, Timestamp};

const TABLE_DATA_BLOCK_FORMAT: &str = "table_data_block";
const INTERNAL_KEY_SUFFIX_LEN: usize = 8;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TableDataEntry {
    internal_key: InternalKey,
    internal_key_bytes: Vec<u8>,
    row: StorageRow,
    row_bytes: Vec<u8>,
}

impl TableDataEntry {
    pub(crate) fn new(row: &StorageRow) -> Result<Self, FormatError> {
        let internal_key = InternalKey::new(row.physical_key().clone(), row.commit_version());
        let internal_key_bytes = encode_internal_key(&internal_key);
        let row_bytes = encode_storage_row(row)?;
        Self::from_parts(internal_key_bytes, row_bytes)
    }

    fn from_parts(internal_key_bytes: Vec<u8>, row_bytes: Vec<u8>) -> Result<Self, FormatError> {
        validate_len(
            internal_key_bytes.len(),
            MAX_TABLE_KEY_BYTES,
            "internal_key_len",
        )?;
        validate_len(row_bytes.len(), MAX_TABLE_ROW_BYTES, "row_len")?;

        let internal_key = decode_internal_key(&internal_key_bytes)?;
        let row = decode_storage_row(&row_bytes)?;
        // The key is stored redundantly so table scans can use ordering bytes
        // without decoding every row, but the row remains the authoritative
        // storage fact. Decode rejects drift between the two copies.
        if internal_key.physical_key() != row.physical_key() {
            return Err(FormatError::InvalidValue {
                field: "physical_key",
            });
        }
        if internal_key.commit_version() != row.commit_version() {
            return Err(FormatError::InvalidValue {
                field: "commit_version",
            });
        }

        Ok(Self {
            internal_key,
            internal_key_bytes,
            row,
            row_bytes,
        })
    }

    pub(crate) const fn internal_key(&self) -> &InternalKey {
        &self.internal_key
    }

    pub(crate) fn internal_key_bytes(&self) -> &[u8] {
        &self.internal_key_bytes
    }

    pub(crate) const fn row(&self) -> &StorageRow {
        &self.row
    }

    pub(crate) fn row_bytes(&self) -> &[u8] {
        &self.row_bytes
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TableDataBlock {
    entries: Vec<TableDataEntry>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct EncodedTableDataBlock {
    payload: Vec<u8>,
    row_count: usize,
    first_key_bytes: Vec<u8>,
    last_key_bytes: Vec<u8>,
}

impl EncodedTableDataBlock {
    pub(crate) fn into_payload(self) -> Vec<u8> {
        self.payload
    }

    pub(crate) const fn row_count(&self) -> usize {
        self.row_count
    }

    pub(crate) fn first_key_bytes(&self) -> &[u8] {
        &self.first_key_bytes
    }

    pub(crate) fn last_key_bytes(&self) -> &[u8] {
        &self.last_key_bytes
    }
}

impl TableDataBlock {
    pub(crate) fn from_rows(rows: &[StorageRow]) -> Result<Self, FormatError> {
        let entries = rows
            .iter()
            .map(TableDataEntry::new)
            .collect::<Result<Vec<_>, _>>()?;
        Self::from_entries(entries)
    }

    fn from_entries(entries: Vec<TableDataEntry>) -> Result<Self, FormatError> {
        validate_count(entries.len())?;
        validate_entry_order(&entries)?;
        Ok(Self { entries })
    }

    pub(crate) fn entries(&self) -> &[TableDataEntry] {
        &self.entries
    }

    pub(crate) fn rows(&self) -> impl Iterator<Item = &StorageRow> {
        self.entries.iter().map(TableDataEntry::row)
    }

    pub(crate) const fn row_count(&self) -> usize {
        self.entries.len()
    }

    pub(crate) fn first_key_bytes(&self) -> &[u8] {
        self.entries[0].internal_key_bytes()
    }

    pub(crate) fn last_key_bytes(&self) -> &[u8] {
        self.entries[self.entries.len() - 1].internal_key_bytes()
    }
}

pub(crate) fn encode_table_data_block(block: &TableDataBlock) -> Result<Vec<u8>, FormatError> {
    validate_count(block.entries().len())?;
    let entry_count =
        u32::try_from(block.entries().len()).map_err(|_| FormatError::InvalidLength {
            field: "entry_count",
        })?;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&entry_count.to_le_bytes());
    for entry in block.entries() {
        append_len_prefixed(&mut bytes, entry.internal_key_bytes(), "internal_key_len")?;
        append_len_prefixed(&mut bytes, entry.row_bytes(), "row_len")?;
    }
    Ok(bytes)
}

pub(crate) fn encode_table_data_block_from_encoded_rows<'a>(
    rows: impl IntoIterator<Item = (&'a [u8], &'a StorageRow)>,
) -> Result<EncodedTableDataBlock, FormatError> {
    let mut payload = Vec::new();
    payload.extend_from_slice(&0u32.to_le_bytes());

    let mut row_bytes = Vec::new();
    let mut row_count = 0usize;
    let mut first_key = None;
    let mut last_key = None;
    let mut previous_key = None;
    for (internal_key_bytes, row) in rows {
        validate_len(
            internal_key_bytes.len(),
            MAX_TABLE_KEY_BYTES,
            "internal_key_len",
        )?;
        let physical_key_bytes = internal_key_physical_key_bytes_checked(internal_key_bytes)?;
        validate_internal_key_commit_suffix(internal_key_bytes, row.commit_version())?;
        validate_next_key_order(previous_key, internal_key_bytes)?;
        if first_key.is_none() {
            first_key = Some(internal_key_bytes);
        }
        previous_key = Some(internal_key_bytes);
        last_key = Some(internal_key_bytes);

        encode_storage_row_with_physical_key_bytes_into(row, physical_key_bytes, &mut row_bytes)?;
        validate_len(row_bytes.len(), MAX_TABLE_ROW_BYTES, "row_len")?;
        append_len_prefixed(&mut payload, internal_key_bytes, "internal_key_len")?;
        append_len_prefixed(&mut payload, &row_bytes, "row_len")?;
        row_count = row_count.checked_add(1).ok_or(FormatError::InvalidLength {
            field: "entry_count",
        })?;
    }

    validate_count(row_count)?;
    let entry_count = u32::try_from(row_count).map_err(|_| FormatError::InvalidLength {
        field: "entry_count",
    })?;
    payload[..4].copy_from_slice(&entry_count.to_le_bytes());
    let first_key_bytes = first_key
        .ok_or(FormatError::InvalidLength {
            field: "entry_count",
        })?
        .to_vec();
    let last_key_bytes = last_key
        .ok_or(FormatError::InvalidLength {
            field: "entry_count",
        })?
        .to_vec();
    Ok(EncodedTableDataBlock {
        payload,
        row_count,
        first_key_bytes,
        last_key_bytes,
    })
}

pub(crate) fn decode_table_data_block(bytes: &[u8]) -> Result<TableDataBlock, FormatError> {
    let mut reader = ByteReader::new(TABLE_DATA_BLOCK_FORMAT, bytes);
    let entry_count =
        usize::try_from(reader.read_u32_le()?).map_err(|_| FormatError::InvalidLength {
            field: "entry_count",
        })?;
    validate_count(entry_count)?;

    let mut entries = Vec::new();
    for _ in 0..entry_count {
        let internal_key_len = read_len(&mut reader, MAX_TABLE_KEY_BYTES, "internal_key_len")?;
        let internal_key_bytes = reader.read_exact(internal_key_len)?.to_vec();
        let row_len = read_len(&mut reader, MAX_TABLE_ROW_BYTES, "row_len")?;
        let row_bytes = reader.read_exact(row_len)?.to_vec();
        entries.push(TableDataEntry::from_parts(internal_key_bytes, row_bytes)?);
    }
    reader.finish()?;

    TableDataBlock::from_entries(entries)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TableDataBlockPointSeek {
    row: Option<StorageRow>,
    rows_visited: usize,
    entries_scanned: usize,
    candidate_rows_decoded: usize,
    continue_to_next_block: bool,
}

impl TableDataBlockPointSeek {
    pub(crate) const fn new(
        row: Option<StorageRow>,
        rows_visited: usize,
        entries_scanned: usize,
        candidate_rows_decoded: usize,
        continue_to_next_block: bool,
    ) -> Self {
        Self {
            row,
            rows_visited,
            entries_scanned,
            candidate_rows_decoded,
            continue_to_next_block,
        }
    }

    pub(crate) fn row(self) -> Option<StorageRow> {
        self.row
    }

    pub(crate) const fn rows_visited(&self) -> usize {
        self.rows_visited
    }

    pub(crate) const fn entries_scanned(&self) -> usize {
        self.entries_scanned
    }

    pub(crate) const fn candidate_rows_decoded(&self) -> usize {
        self.candidate_rows_decoded
    }

    pub(crate) const fn continue_to_next_block(&self) -> bool {
        self.continue_to_next_block
    }
}

pub(crate) fn seek_table_data_block_point(
    index_entry: &TableIndexEntry,
    bytes: &[u8],
    seek_key: &[u8],
    target_physical_key: &[u8],
    max_commit_version: Option<CommitVersion>,
    max_commit_timestamp: Option<Timestamp>,
) -> Result<TableDataBlockPointSeek, FormatError> {
    let mut reader = ByteReader::new(TABLE_DATA_BLOCK_FORMAT, bytes);
    let entry_count =
        usize::try_from(reader.read_u32_le()?).map_err(|_| FormatError::InvalidLength {
            field: "entry_count",
        })?;
    validate_count(entry_count)?;

    let mut first_key = None;
    let mut last_key = None;
    let mut previous_key = None;
    let mut scan_started = false;
    let mut saw_target_at_block_tail = false;
    let mut terminal_reached = false;
    let mut row = None;
    let mut rows_visited = 0usize;
    let mut candidate_rows_decoded = 0usize;

    for _ in 0..entry_count {
        let internal_key_len = read_len(&mut reader, MAX_TABLE_KEY_BYTES, "internal_key_len")?;
        let internal_key_bytes = reader.read_exact(internal_key_len)?;
        let internal_key = decode_internal_key(internal_key_bytes)?;
        validate_next_key_order(previous_key, internal_key_bytes)?;
        if first_key.is_none() {
            first_key = Some(internal_key_bytes);
        }
        previous_key = Some(internal_key_bytes);
        last_key = Some(internal_key_bytes);

        let row_len = read_len(&mut reader, MAX_TABLE_ROW_BYTES, "row_len")?;
        let row_bytes = reader.read_exact(row_len)?;

        if terminal_reached || row.is_some() {
            continue;
        }
        if !scan_started {
            if internal_key_bytes < seek_key {
                continue;
            }
            scan_started = true;
        }

        rows_visited = rows_visited.saturating_add(1);
        match internal_key_physical_key_bytes(internal_key_bytes).cmp(target_physical_key) {
            std::cmp::Ordering::Less => {
                saw_target_at_block_tail = false;
            }
            std::cmp::Ordering::Greater => {
                saw_target_at_block_tail = false;
                terminal_reached = true;
            }
            std::cmp::Ordering::Equal => {
                saw_target_at_block_tail = true;
                if max_commit_version
                    .is_some_and(|max_version| internal_key.commit_version() > max_version)
                {
                    continue;
                }
                let candidate = decode_storage_row_for_internal_key(&internal_key, row_bytes)?;
                candidate_rows_decoded = candidate_rows_decoded.saturating_add(1);
                if max_commit_timestamp
                    .is_none_or(|max_timestamp| candidate.commit_timestamp() <= max_timestamp)
                {
                    row = Some(candidate);
                }
            }
        }
    }
    reader.finish()?;
    validate_scanned_block_against_index(index_entry, entry_count, first_key, last_key)?;

    Ok(TableDataBlockPointSeek::new(
        row,
        rows_visited,
        entry_count,
        candidate_rows_decoded,
        saw_target_at_block_tail && !terminal_reached,
    ))
}

/// W2.3 (B3): byte offsets (payload-relative) of every entry's start — the
/// derived index that lets a trusted seek bisect instead of walking
/// the block linearly. One pass over the length prefixes; no key compares,
/// no row decodes. The result is cached under the block-cache Accelerator
/// kind; it is NOT part of the durable format.
pub(crate) fn build_data_block_entry_offsets(bytes: &[u8]) -> Result<Vec<u32>, FormatError> {
    let mut reader = ByteReader::new(TABLE_DATA_BLOCK_FORMAT, bytes);
    let entry_count =
        usize::try_from(reader.read_u32_le()?).map_err(|_| FormatError::InvalidLength {
            field: "entry_count",
        })?;
    validate_count(entry_count)?;

    let mut offsets = Vec::with_capacity(entry_count);
    let mut position = 4usize;
    for _ in 0..entry_count {
        let offset = u32::try_from(position).map_err(|_| FormatError::InvalidLength {
            field: "entry_offset",
        })?;
        let internal_key_len = read_len(&mut reader, MAX_TABLE_KEY_BYTES, "internal_key_len")?;
        reader.read_exact(internal_key_len)?;
        let row_len = read_len(&mut reader, MAX_TABLE_ROW_BYTES, "row_len")?;
        reader.read_exact(row_len)?;
        offsets.push(offset);
        position = position
            .checked_add(8)
            .and_then(|p| p.checked_add(internal_key_len))
            .and_then(|p| p.checked_add(row_len))
            .ok_or(FormatError::InvalidLength {
                field: "entry_offset",
            })?;
    }
    reader.finish()?;
    Ok(offsets)
}

/// W2.3 (B3): the indexed point seek for integrity-proven payloads (block
/// cache hits): bisect the derived offsets for the first entry whose
/// FULL internal key is >= `seek_key`, then run the exact forward-selection
/// walk of [`seek_table_data_block_point`] from that entry. Result semantics
/// (`row`, `rows_visited`, `continue_to_next_block`) are pinned identical by
/// the equivalence property tests. Offsets are validated defensively — a
/// malformed accelerator yields a structural error (the caller rebuilds from
/// the payload), never a panic and never a wrong answer.
pub(crate) fn seek_table_data_block_point_indexed(
    bytes: &[u8],
    offsets: &[u32],
    seek_key: &[u8],
    target_physical_key: &[u8],
    max_commit_version: Option<CommitVersion>,
    max_commit_timestamp: Option<Timestamp>,
) -> Result<TableDataBlockPointSeek, FormatError> {
    let mut reader = ByteReader::new(TABLE_DATA_BLOCK_FORMAT, bytes);
    let entry_count =
        usize::try_from(reader.read_u32_le()?).map_err(|_| FormatError::InvalidLength {
            field: "entry_count",
        })?;
    validate_count(entry_count)?;
    if offsets.len() != entry_count || offsets.first().is_some_and(|first| *first != 4) {
        return Err(FormatError::InvalidLength {
            field: "entry_offsets",
        });
    }
    // Strictly increasing and payload-bounded — one cheap pass that rejects
    // gross accelerator corruption. (A bit-flip that PRESERVES these
    // invariants can still land a probe mid-entry; that residual class is
    // in-memory corruption of a derived artifact — the same accepted trust
    // class as the cached frame bytes themselves, see W2.6.)
    let payload_len = u32::try_from(bytes.len()).map_err(|_| FormatError::InvalidLength {
        field: "entry_offsets",
    })?;
    if offsets
        .windows(2)
        .any(|pair| pair[0] >= pair[1] || pair[1] >= payload_len)
    {
        return Err(FormatError::InvalidLength {
            field: "entry_offsets",
        });
    }

    // Lower bound over full internal keys (inverted-version suffix included) —
    // exactly the linear scan's `internal_key_bytes < seek_key` ordering.
    let mut low = 0usize;
    let mut high = entry_count;
    while low < high {
        let mid = low + (high - low) / 2;
        let key = entry_key_at_offset(bytes, offsets, mid)?;
        if key < seek_key {
            low = mid + 1;
        } else {
            high = mid;
        }
    }

    let mut saw_target_at_block_tail = false;
    let mut terminal_reached = false;
    let mut row = None;
    let mut rows_visited = 0usize;
    let mut candidate_rows_decoded = 0usize;

    if low < entry_count {
        let start = usize::try_from(offsets[low]).map_err(|_| FormatError::InvalidLength {
            field: "entry_offsets",
        })?;
        let window = bytes.get(start..).ok_or(FormatError::InvalidLength {
            field: "entry_offsets",
        })?;
        let mut reader = ByteReader::new(TABLE_DATA_BLOCK_FORMAT, window);
        for _ in low..entry_count {
            let internal_key_len = read_len(&mut reader, MAX_TABLE_KEY_BYTES, "internal_key_len")?;
            let internal_key_bytes = reader.read_exact(internal_key_len)?;
            let internal_key = decode_internal_key(internal_key_bytes)?;
            let row_len = read_len(&mut reader, MAX_TABLE_ROW_BYTES, "row_len")?;
            let row_bytes = reader.read_exact(row_len)?;

            if terminal_reached || row.is_some() {
                break;
            }

            rows_visited = rows_visited.saturating_add(1);
            match internal_key_physical_key_bytes(internal_key_bytes).cmp(target_physical_key) {
                std::cmp::Ordering::Less => {
                    saw_target_at_block_tail = false;
                }
                std::cmp::Ordering::Greater => {
                    saw_target_at_block_tail = false;
                    terminal_reached = true;
                }
                std::cmp::Ordering::Equal => {
                    saw_target_at_block_tail = true;
                    if max_commit_version
                        .is_some_and(|max_version| internal_key.commit_version() > max_version)
                    {
                        continue;
                    }
                    let candidate = decode_storage_row_for_internal_key(&internal_key, row_bytes)?;
                    candidate_rows_decoded = candidate_rows_decoded.saturating_add(1);
                    if max_commit_timestamp
                        .is_none_or(|max_timestamp| candidate.commit_timestamp() <= max_timestamp)
                    {
                        row = Some(candidate);
                    }
                }
            }
        }
    }

    Ok(TableDataBlockPointSeek::new(
        row,
        rows_visited,
        entry_count,
        candidate_rows_decoded,
        saw_target_at_block_tail && !terminal_reached,
    ))
}

/// Parse the full internal key of entry `index` via its derived offset, with
/// every access bounds-checked (offsets come from a cached artifact).
fn entry_key_at_offset<'a>(
    bytes: &'a [u8],
    offsets: &[u32],
    index: usize,
) -> Result<&'a [u8], FormatError> {
    let start = usize::try_from(offsets[index]).map_err(|_| FormatError::InvalidLength {
        field: "entry_offsets",
    })?;
    let window = bytes.get(start..).ok_or(FormatError::InvalidLength {
        field: "entry_offsets",
    })?;
    let mut reader = ByteReader::new(TABLE_DATA_BLOCK_FORMAT, window);
    let internal_key_len = read_len(&mut reader, MAX_TABLE_KEY_BYTES, "internal_key_len")?;
    reader.read_exact(internal_key_len)
}

fn append_len_prefixed(
    target: &mut Vec<u8>,
    payload: &[u8],
    field: &'static str,
) -> Result<(), FormatError> {
    let max = match field {
        "internal_key_len" => MAX_TABLE_KEY_BYTES,
        _ => MAX_TABLE_ROW_BYTES,
    };
    validate_len(payload.len(), max, field)?;
    let len = u32::try_from(payload.len()).map_err(|_| FormatError::InvalidLength { field })?;
    target.extend_from_slice(&len.to_le_bytes());
    target.extend_from_slice(payload);
    Ok(())
}

fn read_len(
    reader: &mut ByteReader<'_>,
    max: usize,
    field: &'static str,
) -> Result<usize, FormatError> {
    let len =
        usize::try_from(reader.read_u32_le()?).map_err(|_| FormatError::InvalidLength { field })?;
    validate_len(len, max, field)?;
    Ok(len)
}

fn validate_count(count: usize) -> Result<(), FormatError> {
    if count == 0 || count > MAX_TABLE_BLOCK_ENTRIES as usize {
        return Err(FormatError::InvalidLength {
            field: "entry_count",
        });
    }
    Ok(())
}

fn validate_len(len: usize, max: usize, field: &'static str) -> Result<(), FormatError> {
    if len == 0 || len > max {
        return Err(FormatError::InvalidLength { field });
    }
    Ok(())
}

fn validate_entry_order(entries: &[TableDataEntry]) -> Result<(), FormatError> {
    for pair in entries.windows(2) {
        match pair[0]
            .internal_key_bytes()
            .cmp(pair[1].internal_key_bytes())
        {
            std::cmp::Ordering::Less => {}
            std::cmp::Ordering::Equal => {
                return Err(FormatError::InvalidValue {
                    field: "internal_key",
                });
            }
            std::cmp::Ordering::Greater => {
                return Err(FormatError::InvalidValue {
                    field: "internal_key_order",
                });
            }
        }
    }
    Ok(())
}

fn validate_next_key_order(
    previous_key: Option<&[u8]>,
    current_key: &[u8],
) -> Result<(), FormatError> {
    let Some(previous_key) = previous_key else {
        return Ok(());
    };
    match previous_key.cmp(current_key) {
        std::cmp::Ordering::Less => Ok(()),
        std::cmp::Ordering::Equal => Err(FormatError::InvalidValue {
            field: "internal_key",
        }),
        std::cmp::Ordering::Greater => Err(FormatError::InvalidValue {
            field: "internal_key_order",
        }),
    }
}

fn validate_scanned_block_against_index(
    index_entry: &TableIndexEntry,
    row_count: usize,
    first_key: Option<&[u8]>,
    last_key: Option<&[u8]>,
) -> Result<(), FormatError> {
    if index_entry.row_count() as usize != row_count {
        return Err(FormatError::InvalidValue {
            field: "index_row_count",
        });
    }
    if Some(index_entry.first_key_bytes()) != first_key {
        return Err(FormatError::InvalidValue {
            field: "index_first_key",
        });
    }
    if Some(index_entry.last_key_bytes()) != last_key {
        return Err(FormatError::InvalidValue {
            field: "index_last_key",
        });
    }
    Ok(())
}

fn internal_key_physical_key_bytes(bytes: &[u8]) -> &[u8] {
    &bytes[..bytes.len().saturating_sub(INTERNAL_KEY_SUFFIX_LEN)]
}

fn internal_key_physical_key_bytes_checked(bytes: &[u8]) -> Result<&[u8], FormatError> {
    if bytes.len() < INTERNAL_KEY_SUFFIX_LEN {
        return Err(FormatError::InsufficientBytes {
            format: "internal_key",
            needed: INTERNAL_KEY_SUFFIX_LEN,
            actual: bytes.len(),
        });
    }
    Ok(&bytes[..bytes.len() - INTERNAL_KEY_SUFFIX_LEN])
}

fn validate_internal_key_commit_suffix(
    bytes: &[u8],
    commit_version: CommitVersion,
) -> Result<(), FormatError> {
    if bytes.len() < INTERNAL_KEY_SUFFIX_LEN {
        return Err(FormatError::InsufficientBytes {
            format: "internal_key",
            needed: INTERNAL_KEY_SUFFIX_LEN,
            actual: bytes.len(),
        });
    }
    let suffix =
        <[u8; INTERNAL_KEY_SUFFIX_LEN]>::try_from(&bytes[bytes.len() - INTERNAL_KEY_SUFFIX_LEN..])
            .map_err(|_| FormatError::InvalidLength {
                field: "internal_key",
            })?;
    if CommitVersion::new(!u64::from_be_bytes(suffix)) != commit_version {
        return Err(FormatError::InvalidValue {
            field: "commit_version",
        });
    }
    Ok(())
}

fn decode_storage_row_for_internal_key(
    internal_key: &InternalKey,
    row_bytes: &[u8],
) -> Result<StorageRow, FormatError> {
    let row = decode_storage_row(row_bytes)?;
    if internal_key.physical_key() != row.physical_key() {
        return Err(FormatError::InvalidValue {
            field: "physical_key",
        });
    }
    if internal_key.commit_version() != row.commit_version() {
        return Err(FormatError::InvalidValue {
            field: "commit_version",
        });
    }
    Ok(row)
}
