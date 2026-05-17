//! Immutable table reader.

use super::{
    BoundedTableCursor, TableIdentity, TableInternalKeyBytes, TableKeyBounds, TableReaderConfig,
    TableRow, TableRuntimeError, TableRuntimeFacts, TableRuntimeResult,
};
use crate::format::decode_immutable_table;

use super::facts::table_facts_from_decoded;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BytesTableSource {
    bytes: Vec<u8>,
}

impl BytesTableSource {
    pub(crate) fn new(bytes: impl Into<Vec<u8>>) -> Self {
        Self {
            bytes: bytes.into(),
        }
    }

    pub(crate) fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

impl TableByteSource for BytesTableSource {
    fn byte_count(&self) -> u64 {
        u64::try_from(self.bytes.len()).expect("usize fits in u64")
    }

    fn read_at(&self, offset: u64, len: usize) -> TableRuntimeResult<Vec<u8>> {
        let start = usize::try_from(offset).map_err(|_| TableRuntimeError::InvalidRange {
            field: "byte_offset",
        })?;
        let end = start
            .checked_add(len)
            .ok_or(TableRuntimeError::InvalidRange {
                field: "byte_range",
            })?;
        if end > self.bytes.len() {
            return Err(TableRuntimeError::SourceRead {
                reason: "byte range exceeds source length",
            });
        }
        Ok(self.bytes[start..end].to_vec())
    }
}

pub(crate) trait TableByteSource {
    fn byte_count(&self) -> u64;
    fn read_at(&self, offset: u64, len: usize) -> TableRuntimeResult<Vec<u8>>;
}

impl<T: TableByteSource + ?Sized> TableByteSource for &T {
    fn byte_count(&self) -> u64 {
        (**self).byte_count()
    }

    fn read_at(&self, offset: u64, len: usize) -> TableRuntimeResult<Vec<u8>> {
        (**self).read_at(offset, len)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ImmutableTableReader {
    config: TableReaderConfig,
    facts: TableRuntimeFacts,
    rows: Vec<TableRow>,
}

impl ImmutableTableReader {
    pub(crate) fn open_bytes(
        identity: TableIdentity,
        bytes: Vec<u8>,
        config: TableReaderConfig,
    ) -> TableRuntimeResult<Self> {
        let source = BytesTableSource::new(bytes);
        let (facts, rows) = decode_reader_rows(identity, source.bytes())?;
        Ok(Self {
            config,
            facts,
            rows,
        })
    }

    pub(crate) fn open_source<S: TableByteSource>(
        identity: TableIdentity,
        source: S,
        config: TableReaderConfig,
    ) -> TableRuntimeResult<Self> {
        let bytes = read_full_source(&source)?;
        let (facts, rows) = decode_reader_rows(identity, &bytes)?;
        Ok(Self {
            config,
            facts,
            rows,
        })
    }

    pub(crate) const fn config(&self) -> TableReaderConfig {
        self.config
    }

    pub(crate) const fn facts(&self) -> &TableRuntimeFacts {
        &self.facts
    }

    pub(crate) const fn byte_count(&self) -> u64 {
        self.facts.byte_count()
    }

    pub(crate) fn rows(&self) -> &[TableRow] {
        &self.rows
    }

    pub(crate) fn get_exact(&self, key: &TableInternalKeyBytes) -> Option<TableRow> {
        self.rows
            .binary_search_by(|row| row.key().cmp(key))
            .ok()
            .map(|index| self.rows[index].clone())
    }

    pub(crate) fn cursor(&self) -> ImmutableTableCursor<'_> {
        ImmutableTableCursor::new(&self.rows)
    }

    pub(crate) fn bounded_cursor(&self, bounds: TableKeyBounds) -> BoundedTableCursor<'_> {
        BoundedTableCursor::new(Box::new(self.cursor()), bounds)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ImmutableTableCursor<'a> {
    rows: &'a [TableRow],
    position: Option<usize>,
}

impl<'a> ImmutableTableCursor<'a> {
    fn new(rows: &'a [TableRow]) -> Self {
        Self {
            rows,
            position: None,
        }
    }

    fn seek_index(&self, target: &TableInternalKeyBytes) -> Option<usize> {
        match self.rows.binary_search_by(|row| row.key().cmp(target)) {
            Ok(index) | Err(index) if index < self.rows.len() => Some(index),
            Ok(_) | Err(_) => None,
        }
    }
}

impl super::TableCursor for ImmutableTableCursor<'_> {
    fn seek_to_first(&mut self) -> TableRuntimeResult<()> {
        self.position = if self.rows.is_empty() { None } else { Some(0) };
        Ok(())
    }

    fn seek(&mut self, target: &TableInternalKeyBytes) -> TableRuntimeResult<()> {
        self.position = self.seek_index(target);
        Ok(())
    }

    fn advance(&mut self) -> TableRuntimeResult<()> {
        self.position = self.position.and_then(|position| {
            let next = position.saturating_add(1);
            (next < self.rows.len()).then_some(next)
        });
        Ok(())
    }

    fn current(&self) -> Option<&TableRow> {
        self.position.and_then(|position| self.rows.get(position))
    }
}

fn read_full_source(source: &impl TableByteSource) -> TableRuntimeResult<Vec<u8>> {
    let len =
        usize::try_from(source.byte_count()).map_err(|_| TableRuntimeError::InvalidRange {
            field: "byte_count",
        })?;
    let bytes = source.read_at(0, len)?;
    if bytes.len() != len {
        return Err(TableRuntimeError::SourceRead {
            reason: "short table source read",
        });
    }
    Ok(bytes)
}

fn decode_reader_rows(
    identity: TableIdentity,
    bytes: &[u8],
) -> TableRuntimeResult<(TableRuntimeFacts, Vec<TableRow>)> {
    let decoded = decode_immutable_table(bytes)
        .map_err(|source| TableRuntimeError::DecodeFormat { source })?;
    let facts = table_facts_from_decoded(identity, bytes, &decoded)?;
    let rows = decoded
        .rows()
        .iter()
        .cloned()
        .map(TableRow::new)
        .collect::<Vec<_>>();
    super::validate_strictly_sorted_unique_rows(&rows)?;
    Ok((facts, rows))
}
