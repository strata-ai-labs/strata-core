//! Table row and key adapters.

use super::{TableRuntimeError, TableRuntimeResult};
use crate::format::{decode_internal_key, encode_internal_key, encode_physical_key, FormatError};
use crate::observability::perf_trace;
use crate::row::{InternalKey, PhysicalKey, StorageRow};
use std::cell::Cell;
use std::{cmp::Ordering, fmt};
use strata_core_next::{CommitVersion, Timestamp};

const TABLE_INTERNAL_KEY_COMMIT_SUFFIX_BYTES: usize = 8;
const TABLE_ROW_METADATA_BYTES: usize = 8 + 8 + 8 + 1;
const TABLE_ROW_ADAPTER_OVERHEAD_BYTES: usize =
    std::mem::size_of::<TableInternalKeyBytes>() + std::mem::size_of::<StorageRow>();

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct TableInternalKeyBytes {
    bytes: Vec<u8>,
}

impl TableInternalKeyBytes {
    pub(crate) fn from_row(row: &StorageRow) -> Self {
        Self::from_internal_key(&InternalKey::new(
            row.physical_key().clone(),
            row.commit_version(),
        ))
    }

    pub(crate) fn from_internal_key(key: &InternalKey) -> Self {
        Self {
            bytes: encode_internal_key(key),
        }
    }

    pub(crate) fn from_canonical_bytes(bytes: impl Into<Vec<u8>>) -> TableRuntimeResult<Self> {
        let bytes = bytes.into();
        let key = decode_internal_key(&bytes)
            .map_err(|source| TableRuntimeError::DecodeFormat { source })?;
        let canonical = encode_internal_key(&key);
        if canonical != bytes {
            return Err(TableRuntimeError::DecodeFormat {
                source: FormatError::InvalidValue {
                    field: "internal_key",
                },
            });
        }
        Ok(Self { bytes })
    }

    pub(crate) fn decode(&self) -> TableRuntimeResult<InternalKey> {
        decode_internal_key(&self.bytes)
            .map_err(|source| TableRuntimeError::DecodeFormat { source })
    }

    pub(crate) fn physical_key(&self) -> TableRuntimeResult<PhysicalKey> {
        self.decode().map(|key| key.physical_key().clone())
    }

    pub(crate) fn commit_version(&self) -> TableRuntimeResult<CommitVersion> {
        self.decode().map(|key| key.commit_version())
    }

    pub(crate) fn as_slice(&self) -> &[u8] {
        &self.bytes
    }

    pub(crate) fn copy_from(&mut self, other: &Self) {
        self.bytes.clear();
        self.bytes.extend_from_slice(other.as_slice());
    }

    pub(crate) fn physical_key_bytes(&self) -> &[u8] {
        table_internal_physical_key_bytes(&self.bytes)
    }

    pub(crate) fn into_vec(self) -> Vec<u8> {
        self.bytes
    }

    pub(crate) fn len(&self) -> usize {
        self.bytes.len()
    }
}

impl AsRef<[u8]> for TableInternalKeyBytes {
    fn as_ref(&self) -> &[u8] {
        self.as_slice()
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct TablePhysicalKeyBytes {
    bytes: Vec<u8>,
}

impl TablePhysicalKeyBytes {
    pub(crate) fn from_physical_key(key: &PhysicalKey) -> Self {
        Self {
            bytes: encode_physical_key(key),
        }
    }

    pub(crate) fn from_physical_key_prefix(key: &PhysicalKey) -> Self {
        let mut bytes = encode_physical_key(key);
        bytes.truncate(bytes.len().saturating_sub(2));
        Self { bytes }
    }

    pub(crate) fn from_row(row: &StorageRow) -> Self {
        Self::from_physical_key(row.physical_key())
    }

    pub(crate) fn from_encoded_internal_key(encoded_key: &[u8]) -> Self {
        Self {
            bytes: table_internal_physical_key_bytes(encoded_key).to_vec(),
        }
    }

    /// The empty physical key, used as a match-everything prefix for a
    /// `TableKeyBounds::physical_range` whose selectivity comes entirely from its lower/upper
    /// physical-key bounds (subcompaction key-range splitting).
    pub(crate) fn empty() -> Self {
        Self { bytes: Vec::new() }
    }

    pub(crate) fn as_slice(&self) -> &[u8] {
        &self.bytes
    }

    pub(crate) fn len(&self) -> usize {
        self.bytes.len()
    }

    pub(crate) fn is_prefix_of(&self, key: &TableInternalKeyBytes) -> bool {
        key.as_slice().starts_with(&self.bytes)
    }
}

impl AsRef<[u8]> for TablePhysicalKeyBytes {
    fn as_ref(&self) -> &[u8] {
        self.as_slice()
    }
}

#[derive(Debug)]
pub(crate) struct TablePreparedPointLookup {
    physical_key: TablePhysicalKeyBytes,
    seek_key: TableInternalKeyBytes,
    max_commit_version: Option<CommitVersion>,
    max_commit_timestamp: Option<Timestamp>,
    table_uses: Cell<u64>,
}

impl TablePreparedPointLookup {
    pub(crate) fn new(
        key: &PhysicalKey,
        max_commit_version: Option<CommitVersion>,
        max_commit_timestamp: Option<Timestamp>,
    ) -> Self {
        perf_trace::record_table_point_lookup_key_build();
        let physical_key = TablePhysicalKeyBytes::from_physical_key(key);
        let seek_version = max_commit_version.unwrap_or(CommitVersion::MAX);
        let seek_key =
            TableInternalKeyBytes::from_internal_key(&InternalKey::new(key.clone(), seek_version));
        Self {
            physical_key,
            seek_key,
            max_commit_version,
            max_commit_timestamp,
            table_uses: Cell::new(0),
        }
    }

    pub(crate) fn record_table_seek_use(&self) {
        let previous_uses = self.table_uses.get();
        if previous_uses > 0 {
            perf_trace::record_table_point_lookup_key_reuse();
        }
        self.table_uses.set(previous_uses.saturating_add(1));
    }

    pub(crate) const fn physical_key(&self) -> &TablePhysicalKeyBytes {
        &self.physical_key
    }

    pub(crate) const fn seek_key(&self) -> &TableInternalKeyBytes {
        &self.seek_key
    }

    pub(crate) const fn max_commit_version(&self) -> Option<CommitVersion> {
        self.max_commit_version
    }

    pub(crate) const fn max_commit_timestamp(&self) -> Option<Timestamp> {
        self.max_commit_timestamp
    }
}

#[derive(Clone)]
pub(crate) struct TableRow {
    row: StorageRow,
    key: TableInternalKeyBytes,
    approximate_size_bytes: usize,
}

impl TableRow {
    pub(crate) fn new(row: StorageRow) -> Self {
        let key = TableInternalKeyBytes::from_row(&row);
        let approximate_size_bytes = approximate_row_size(&row, &key);
        Self {
            row,
            key,
            approximate_size_bytes,
        }
    }

    pub(crate) const fn row(&self) -> &StorageRow {
        &self.row
    }

    pub(crate) const fn key(&self) -> &TableInternalKeyBytes {
        &self.key
    }

    pub(crate) fn encoded_key(&self) -> &[u8] {
        self.key.as_slice()
    }

    pub(crate) const fn physical_key(&self) -> &PhysicalKey {
        self.row.physical_key()
    }

    pub(crate) const fn commit_version(&self) -> CommitVersion {
        self.row.commit_version()
    }

    pub(crate) const fn commit_timestamp(&self) -> Timestamp {
        self.row.commit_timestamp()
    }

    pub(crate) const fn expires_at(&self) -> Timestamp {
        self.row.expires_at()
    }

    pub(crate) fn value(&self) -> &[u8] {
        self.row.value()
    }

    pub(crate) const fn is_tombstone(&self) -> bool {
        self.row.is_tombstone()
    }

    pub(crate) const fn approximate_size_bytes(&self) -> usize {
        self.approximate_size_bytes
    }

    pub(crate) fn into_row(self) -> StorageRow {
        self.row
    }
}

impl PartialEq for TableRow {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}

impl Eq for TableRow {}

impl fmt::Debug for TableRow {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TableRow")
            .field(
                "physical_key_len",
                &self
                    .key
                    .len()
                    .saturating_sub(TABLE_INTERNAL_KEY_COMMIT_SUFFIX_BYTES),
            )
            .field("encoded_key_len", &self.key.len())
            .field("commit_version", &self.commit_version())
            .field(
                "commit_timestamp_micros",
                &self.commit_timestamp().as_micros(),
            )
            .field("expires_at_micros", &self.expires_at().as_micros())
            .field("is_tombstone", &self.is_tombstone())
            .field("value_len", &self.value().len())
            .field("approximate_size_bytes", &self.approximate_size_bytes)
            .finish_non_exhaustive()
    }
}

impl Ord for TableRow {
    fn cmp(&self, other: &Self) -> Ordering {
        self.key.cmp(&other.key)
    }
}

impl PartialOrd for TableRow {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum TableKeyBound {
    Unbounded,
    Included(TableInternalKeyBytes),
    Excluded(TableInternalKeyBytes),
}

impl TableKeyBound {
    pub(crate) fn included(key: TableInternalKeyBytes) -> Self {
        Self::Included(key)
    }

    pub(crate) fn excluded(key: TableInternalKeyBytes) -> Self {
        Self::Excluded(key)
    }

    fn finite_key(&self) -> Option<&TableInternalKeyBytes> {
        match self {
            Self::Unbounded => None,
            Self::Included(key) | Self::Excluded(key) => Some(key),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum TablePhysicalKeyBound {
    Unbounded,
    Included(TablePhysicalKeyBytes),
    Excluded(TablePhysicalKeyBytes),
}

impl TablePhysicalKeyBound {
    pub(crate) fn included(key: TablePhysicalKeyBytes) -> Self {
        Self::Included(key)
    }

    pub(crate) fn excluded(key: TablePhysicalKeyBytes) -> Self {
        Self::Excluded(key)
    }

    fn finite_key(&self) -> Option<&TablePhysicalKeyBytes> {
        match self {
            Self::Unbounded => None,
            Self::Included(key) | Self::Excluded(key) => Some(key),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum TableKeyBounds {
    Range {
        lower: TableKeyBound,
        upper: TableKeyBound,
    },
    Prefix(Vec<u8>),
    PhysicalRange {
        physical_prefix: Vec<u8>,
        lower: TablePhysicalKeyBound,
        upper: TablePhysicalKeyBound,
    },
}

impl TableKeyBounds {
    pub(crate) fn unbounded() -> Self {
        Self::Range {
            lower: TableKeyBound::Unbounded,
            upper: TableKeyBound::Unbounded,
        }
    }

    pub(crate) fn exact(key: TableInternalKeyBytes) -> Self {
        Self::Range {
            lower: TableKeyBound::Included(key.clone()),
            upper: TableKeyBound::Included(key),
        }
    }

    pub(crate) fn closed(
        lower: TableInternalKeyBytes,
        upper: TableInternalKeyBytes,
    ) -> TableRuntimeResult<Self> {
        Self::range(
            TableKeyBound::Included(lower),
            TableKeyBound::Included(upper),
        )
    }

    pub(crate) fn open(
        lower: TableInternalKeyBytes,
        upper: TableInternalKeyBytes,
    ) -> TableRuntimeResult<Self> {
        Self::range(
            TableKeyBound::Excluded(lower),
            TableKeyBound::Excluded(upper),
        )
    }

    pub(crate) fn range(lower: TableKeyBound, upper: TableKeyBound) -> TableRuntimeResult<Self> {
        validate_bound_order(&lower, &upper)?;
        Ok(Self::Range { lower, upper })
    }

    pub(crate) fn prefix(prefix: impl Into<Vec<u8>>) -> Self {
        Self::Prefix(prefix.into())
    }

    pub(crate) fn physical_range(
        physical_prefix: &TablePhysicalKeyBytes,
        lower: TablePhysicalKeyBound,
        upper: TablePhysicalKeyBound,
    ) -> TableRuntimeResult<Self> {
        validate_physical_bound_order(&lower, &upper)?;
        Ok(Self::PhysicalRange {
            physical_prefix: physical_prefix.as_slice().to_vec(),
            lower,
            upper,
        })
    }

    pub(crate) fn contains_key(&self, key: &TableInternalKeyBytes) -> bool {
        match self {
            Self::Range { lower, upper } => {
                lower_contains(lower, key) && upper_contains(upper, key)
            }
            Self::Prefix(prefix) => key.as_slice().starts_with(prefix),
            Self::PhysicalRange {
                physical_prefix,
                lower,
                upper,
            } => {
                let physical_key = key.physical_key_bytes();
                physical_key.starts_with(physical_prefix)
                    && physical_lower_contains(lower, physical_key)
                    && physical_upper_contains(upper, physical_key)
            }
        }
    }

    pub(crate) fn lower_seek_key(&self) -> Option<TableInternalKeyBytes> {
        match self {
            Self::Range { lower, .. } => lower.finite_key().cloned(),
            Self::Prefix(prefix) if prefix.is_empty() => None,
            Self::Prefix(prefix) => Some(TableInternalKeyBytes {
                bytes: prefix.clone(),
            }),
            Self::PhysicalRange {
                physical_prefix,
                lower,
                ..
            } => lower
                .finite_key()
                .map(|key| TableInternalKeyBytes {
                    bytes: key.as_slice().to_vec(),
                })
                .or_else(|| {
                    (!physical_prefix.is_empty()).then(|| TableInternalKeyBytes {
                        bytes: physical_prefix.clone(),
                    })
                }),
        }
    }

    pub(crate) fn is_past_upper_bound(&self, key: &TableInternalKeyBytes) -> bool {
        match self {
            Self::Range { upper, .. } => upper_excludes_current_and_following(upper, key),
            Self::Prefix(prefix) => {
                !key.as_slice().starts_with(prefix) && key.as_slice() > prefix.as_slice()
            }
            Self::PhysicalRange {
                physical_prefix,
                upper,
                ..
            } => {
                let physical_key = key.physical_key_bytes();
                (!physical_key.starts_with(physical_prefix)
                    && physical_key > physical_prefix.as_slice())
                    || physical_upper_excludes_current_and_following(upper, physical_key)
            }
        }
    }

    pub(crate) fn is_past_upper_bound_bytes(&self, key: &[u8]) -> bool {
        match self {
            Self::Range { upper, .. } => upper_excludes_current_and_following_bytes(upper, key),
            Self::Prefix(prefix) => !key.starts_with(prefix) && key > prefix.as_slice(),
            Self::PhysicalRange {
                physical_prefix,
                upper,
                ..
            } => {
                let physical_key = table_internal_physical_key_bytes(key);
                (!physical_key.starts_with(physical_prefix)
                    && physical_key > physical_prefix.as_slice())
                    || physical_upper_excludes_current_and_following(upper, physical_key)
            }
        }
    }
}

pub(crate) fn validate_strictly_sorted_unique_rows(rows: &[TableRow]) -> TableRuntimeResult<()> {
    validate_strictly_sorted_unique_keys(rows.iter().map(TableRow::key))
}

pub(crate) fn validate_strictly_sorted_unique_keys<'a>(
    keys: impl IntoIterator<Item = &'a TableInternalKeyBytes>,
) -> TableRuntimeResult<()> {
    let mut previous: Option<&TableInternalKeyBytes> = None;
    for current in keys {
        if let Some(previous) = previous {
            match previous.cmp(current) {
                Ordering::Less => {}
                Ordering::Equal => {
                    return Err(TableRuntimeError::DuplicateInternalKey {
                        key: current.as_slice().to_vec(),
                    });
                }
                Ordering::Greater => {
                    return Err(TableRuntimeError::InvalidRowOrder {
                        previous: previous.as_slice().to_vec(),
                        current: current.as_slice().to_vec(),
                    });
                }
            }
        }
        previous = Some(current);
    }
    Ok(())
}

pub(crate) fn sort_table_rows_by_key(rows: &mut [TableRow]) {
    rows.sort();
}

pub(crate) fn first_table_key(rows: &[TableRow]) -> Option<&TableInternalKeyBytes> {
    rows.first().map(TableRow::key)
}

pub(crate) fn last_table_key(rows: &[TableRow]) -> Option<&TableInternalKeyBytes> {
    rows.last().map(TableRow::key)
}

fn approximate_row_size(row: &StorageRow, key: &TableInternalKeyBytes) -> usize {
    key.len()
        .saturating_add(row.value().len())
        .saturating_add(TABLE_ROW_METADATA_BYTES)
        .saturating_add(TABLE_ROW_ADAPTER_OVERHEAD_BYTES)
}

fn validate_bound_order(lower: &TableKeyBound, upper: &TableKeyBound) -> TableRuntimeResult<()> {
    if let (Some(lower), Some(upper)) = (lower.finite_key(), upper.finite_key()) {
        if lower > upper {
            return Err(TableRuntimeError::InvalidRange {
                field: "key_bounds",
            });
        }
    }
    Ok(())
}

fn validate_physical_bound_order(
    lower: &TablePhysicalKeyBound,
    upper: &TablePhysicalKeyBound,
) -> TableRuntimeResult<()> {
    if let (Some(lower), Some(upper)) = (lower.finite_key(), upper.finite_key()) {
        if lower > upper {
            return Err(TableRuntimeError::InvalidRange {
                field: "physical_key_bounds",
            });
        }
    }
    Ok(())
}

fn lower_contains(bound: &TableKeyBound, key: &TableInternalKeyBytes) -> bool {
    match bound {
        TableKeyBound::Unbounded => true,
        TableKeyBound::Included(lower) => key >= lower,
        TableKeyBound::Excluded(lower) => key > lower,
    }
}

fn upper_contains(bound: &TableKeyBound, key: &TableInternalKeyBytes) -> bool {
    match bound {
        TableKeyBound::Unbounded => true,
        TableKeyBound::Included(upper) => key <= upper,
        TableKeyBound::Excluded(upper) => key < upper,
    }
}

fn upper_excludes_current_and_following(
    bound: &TableKeyBound,
    key: &TableInternalKeyBytes,
) -> bool {
    match bound {
        TableKeyBound::Unbounded => false,
        TableKeyBound::Included(upper) => key > upper,
        TableKeyBound::Excluded(upper) => key >= upper,
    }
}

fn upper_excludes_current_and_following_bytes(bound: &TableKeyBound, key: &[u8]) -> bool {
    match bound {
        TableKeyBound::Unbounded => false,
        TableKeyBound::Included(upper) => key > upper.as_slice(),
        TableKeyBound::Excluded(upper) => key >= upper.as_slice(),
    }
}

fn physical_lower_contains(bound: &TablePhysicalKeyBound, key: &[u8]) -> bool {
    match bound {
        TablePhysicalKeyBound::Unbounded => true,
        TablePhysicalKeyBound::Included(lower) => key >= lower.as_slice(),
        TablePhysicalKeyBound::Excluded(lower) => key > lower.as_slice(),
    }
}

fn physical_upper_contains(bound: &TablePhysicalKeyBound, key: &[u8]) -> bool {
    match bound {
        TablePhysicalKeyBound::Unbounded => true,
        TablePhysicalKeyBound::Included(upper) => key <= upper.as_slice(),
        TablePhysicalKeyBound::Excluded(upper) => key < upper.as_slice(),
    }
}

fn physical_upper_excludes_current_and_following(
    bound: &TablePhysicalKeyBound,
    key: &[u8],
) -> bool {
    match bound {
        TablePhysicalKeyBound::Unbounded => false,
        TablePhysicalKeyBound::Included(upper) => key > upper.as_slice(),
        TablePhysicalKeyBound::Excluded(upper) => key >= upper.as_slice(),
    }
}

pub(crate) fn table_internal_physical_key_bytes(encoded_key: &[u8]) -> &[u8] {
    encoded_key
        .split_at(
            encoded_key
                .len()
                .saturating_sub(TABLE_INTERNAL_KEY_COMMIT_SUFFIX_BYTES),
        )
        .0
}
