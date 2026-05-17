//! Raw table cursors.

use super::{
    validate_strictly_sorted_unique_keys, FrozenTable, MutableTable, TableInternalKeyBytes,
    TableKeyBounds, TableRow,
};
use crate::table::TableRuntimeResult;
use std::cmp::Ordering;
use std::collections::BinaryHeap;

pub(crate) const MERGE_HEAP_THRESHOLD: usize = 4;

pub(crate) trait TableCursor {
    fn seek_to_first(&mut self) -> TableRuntimeResult<()>;
    fn seek(&mut self, target: &TableInternalKeyBytes) -> TableRuntimeResult<()>;
    fn advance(&mut self) -> TableRuntimeResult<()>;
    fn current(&self) -> Option<&TableRow>;

    fn current_key(&self) -> Option<&TableInternalKeyBytes> {
        self.current().map(TableRow::key)
    }

    fn is_valid(&self) -> bool {
        self.current().is_some()
    }
}

#[derive(Clone)]
pub(crate) struct MemoryTableCursor<'a> {
    rows: Vec<&'a TableRow>,
    position: Option<usize>,
}

impl<'a> MemoryTableCursor<'a> {
    pub(crate) fn from_mutable(table: &'a MutableTable) -> Self {
        Self::from_trusted_rows(table.iter())
    }

    pub(crate) fn from_frozen(table: &'a FrozenTable) -> Self {
        Self::from_trusted_rows(table.iter())
    }

    pub(crate) fn from_sorted_rows(
        rows: impl IntoIterator<Item = &'a TableRow>,
    ) -> TableRuntimeResult<Self> {
        let rows = rows.into_iter().collect::<Vec<_>>();
        validate_strictly_sorted_unique_keys(rows.iter().map(|row| row.key()))?;
        Ok(Self {
            rows,
            position: None,
        })
    }

    fn from_trusted_rows(rows: impl IntoIterator<Item = &'a TableRow>) -> Self {
        Self {
            rows: rows.into_iter().collect(),
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

impl TableCursor for MemoryTableCursor<'_> {
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
        self.position
            .and_then(|position| self.rows.get(position).copied())
    }
}

pub(crate) struct BoundedTableCursor<'a> {
    inner: Box<dyn TableCursor + 'a>,
    bounds: TableKeyBounds,
    positioned: bool,
}

impl<'a> BoundedTableCursor<'a> {
    pub(crate) fn new(inner: Box<dyn TableCursor + 'a>, bounds: TableKeyBounds) -> Self {
        Self {
            inner,
            bounds,
            positioned: false,
        }
    }

    fn skip_to_next_in_bounds(&mut self) -> TableRuntimeResult<()> {
        while let Some(key) = self.inner.current_key() {
            if self.bounds.contains_key(key) {
                break;
            }
            self.inner.advance()?;
        }
        Ok(())
    }
}

impl TableCursor for BoundedTableCursor<'_> {
    fn seek_to_first(&mut self) -> TableRuntimeResult<()> {
        self.inner.seek_to_first()?;
        self.positioned = true;
        self.skip_to_next_in_bounds()
    }

    fn seek(&mut self, target: &TableInternalKeyBytes) -> TableRuntimeResult<()> {
        self.inner.seek(target)?;
        self.positioned = true;
        self.skip_to_next_in_bounds()
    }

    fn advance(&mut self) -> TableRuntimeResult<()> {
        if !self.positioned {
            return Ok(());
        }
        self.inner.advance()?;
        self.skip_to_next_in_bounds()
    }

    fn current(&self) -> Option<&TableRow> {
        self.positioned
            .then(|| {
                self.inner
                    .current()
                    .filter(|row| self.bounds.contains_key(row.key()))
            })
            .flatten()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CursorMergePath {
    Empty,
    Single,
    Linear,
    Heap,
}

pub(crate) struct MergeTableCursor<'a> {
    state: MergeState<'a>,
}

enum MergeState<'a> {
    Empty,
    Single {
        child: Box<dyn TableCursor + 'a>,
        positioned: bool,
    },
    Linear {
        children: Vec<Box<dyn TableCursor + 'a>>,
        current_source: Option<usize>,
    },
    Heap {
        children: Vec<Box<dyn TableCursor + 'a>>,
        heap: BinaryHeap<HeapItem>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct HeapItem {
    key: TableInternalKeyBytes,
    source_index: usize,
}

impl Ord for HeapItem {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .key
            .cmp(&self.key)
            .then_with(|| other.source_index.cmp(&self.source_index))
    }
}

impl PartialOrd for HeapItem {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<'a> MergeTableCursor<'a> {
    pub(crate) fn new(children: Vec<Box<dyn TableCursor + 'a>>) -> Self {
        let state = match children.len() {
            0 => MergeState::Empty,
            1 => MergeState::Single {
                child: children.into_iter().next().expect("one child"),
                positioned: false,
            },
            2..=MERGE_HEAP_THRESHOLD => MergeState::Linear {
                children,
                current_source: None,
            },
            _ => MergeState::Heap {
                children,
                heap: BinaryHeap::new(),
            },
        };
        Self { state }
    }

    pub(crate) const fn path(&self) -> CursorMergePath {
        match &self.state {
            MergeState::Empty => CursorMergePath::Empty,
            MergeState::Single { .. } => CursorMergePath::Single,
            MergeState::Linear { .. } => CursorMergePath::Linear,
            MergeState::Heap { .. } => CursorMergePath::Heap,
        }
    }
}

impl TableCursor for MergeTableCursor<'_> {
    fn seek_to_first(&mut self) -> TableRuntimeResult<()> {
        match &mut self.state {
            MergeState::Empty => Ok(()),
            MergeState::Single { child, positioned } => {
                child.seek_to_first()?;
                *positioned = true;
                Ok(())
            }
            MergeState::Linear {
                children,
                current_source,
            } => {
                for child in children.iter_mut() {
                    child.seek_to_first()?;
                }
                *current_source = select_linear_child(children);
                Ok(())
            }
            MergeState::Heap { children, heap } => {
                for child in children.iter_mut() {
                    child.seek_to_first()?;
                }
                rebuild_heap(children, heap);
                Ok(())
            }
        }
    }

    fn seek(&mut self, target: &TableInternalKeyBytes) -> TableRuntimeResult<()> {
        match &mut self.state {
            MergeState::Empty => Ok(()),
            MergeState::Single { child, positioned } => {
                child.seek(target)?;
                *positioned = true;
                Ok(())
            }
            MergeState::Linear {
                children,
                current_source,
            } => {
                for child in children.iter_mut() {
                    child.seek(target)?;
                }
                *current_source = select_linear_child(children);
                Ok(())
            }
            MergeState::Heap { children, heap } => {
                for child in children.iter_mut() {
                    child.seek(target)?;
                }
                rebuild_heap(children, heap);
                Ok(())
            }
        }
    }

    fn advance(&mut self) -> TableRuntimeResult<()> {
        match &mut self.state {
            MergeState::Empty => Ok(()),
            MergeState::Single { child, positioned } => {
                if *positioned {
                    child.advance()?;
                }
                Ok(())
            }
            MergeState::Linear {
                children,
                current_source,
            } => {
                let Some(source_index) = *current_source else {
                    return Ok(());
                };
                children[source_index].advance()?;
                *current_source = select_linear_child(children);
                Ok(())
            }
            MergeState::Heap { children, heap } => {
                if let Some(selected) = heap.pop() {
                    children[selected.source_index].advance()?;
                    if let Some(key) = children[selected.source_index].current_key() {
                        heap.push(HeapItem {
                            key: key.clone(),
                            source_index: selected.source_index,
                        });
                    }
                }
                Ok(())
            }
        }
    }

    fn current(&self) -> Option<&TableRow> {
        match &self.state {
            MergeState::Empty => None,
            MergeState::Single { child, positioned } => {
                positioned.then(|| child.current()).flatten()
            }
            MergeState::Linear {
                children,
                current_source,
            } => current_source.and_then(|source_index| children[source_index].current()),
            MergeState::Heap { children, heap } => heap
                .peek()
                .and_then(|selected| children[selected.source_index].current()),
        }
    }
}

fn select_linear_child(children: &[Box<dyn TableCursor + '_>]) -> Option<usize> {
    let mut selected: Option<(usize, &TableInternalKeyBytes)> = None;
    for (source_index, child) in children.iter().enumerate() {
        let Some(key) = child.current_key() else {
            continue;
        };
        let replace = match selected {
            None => true,
            Some((selected_source, selected_key)) => {
                key < selected_key || (key == selected_key && source_index < selected_source)
            }
        };
        if replace {
            selected = Some((source_index, key));
        }
    }
    selected.map(|(source_index, _)| source_index)
}

fn rebuild_heap(children: &[Box<dyn TableCursor + '_>], heap: &mut BinaryHeap<HeapItem>) {
    heap.clear();
    for (source_index, child) in children.iter().enumerate() {
        if let Some(key) = child.current_key() {
            heap.push(HeapItem {
                key: key.clone(),
                source_index,
            });
        }
    }
}
