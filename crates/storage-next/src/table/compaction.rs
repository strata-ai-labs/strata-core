//! Generic table compaction.

use super::{
    validate_strictly_sorted_unique_rows, BuiltTableArtifact, ImmutableTableBuilder,
    TableBuilderConfig, TableCompactionConfig, TableCursor, TableIdentity, TableInternalKeyBytes,
    TablePhysicalKeyBytes, TableRow, TableRuntimeError, TableRuntimeResult,
};
use crate::observability::perf_trace;
use std::cmp::Ordering;
use std::collections::BinaryHeap;

const MAX_SOURCE_ID_BYTES: usize = 128;
const OUTPUT_IDENTITY_METADATA_HASH_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const OUTPUT_IDENTITY_METADATA_HASH_PRIME: u64 = 0x0000_0100_0000_01b3;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TableCompactor {
    config: TableCompactionConfig,
    builder_config: TableBuilderConfig,
}

impl TableCompactor {
    pub(crate) fn new(
        config: TableCompactionConfig,
        builder_config: TableBuilderConfig,
    ) -> TableRuntimeResult<Self> {
        config.validate()?;
        builder_config.validate()?;
        Ok(Self {
            config,
            builder_config,
        })
    }

    pub(crate) fn compact<P: TableCompactionPolicy + ?Sized>(
        &self,
        output_identity_seed: &TableIdentity,
        sources: &[TableCompactionSource],
        policy: &mut P,
    ) -> TableRuntimeResult<TableCompactionOutput> {
        let sources = sources
            .iter()
            .map(|source| source as &dyn TableCompactionInput)
            .collect::<Vec<_>>();
        self.compact_inputs(output_identity_seed, &sources, policy)
    }

    pub(crate) fn compact_inputs<P: TableCompactionPolicy + ?Sized>(
        &self,
        output_identity_seed: &TableIdentity,
        sources: &[&dyn TableCompactionInput],
        policy: &mut P,
    ) -> TableRuntimeResult<TableCompactionOutput> {
        compact_table_inputs(self, output_identity_seed, sources, policy)
    }

    pub(crate) const fn config(&self) -> TableCompactionConfig {
        self.config
    }

    pub(crate) const fn builder_config(&self) -> TableBuilderConfig {
        self.builder_config
    }
}

impl Default for TableCompactor {
    fn default() -> Self {
        Self::new(
            TableCompactionConfig::default(),
            TableBuilderConfig::default(),
        )
        .expect("default table compactor configuration is valid")
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TableCompactionSourceId {
    text: String,
}

impl TableCompactionSourceId {
    pub(crate) fn new(text: impl Into<String>) -> TableRuntimeResult<Self> {
        let text = text.into();
        if text.is_empty() {
            return Err(TableRuntimeError::InvalidConfig {
                field: "compaction_source_id",
                reason: "must not be empty",
            });
        }
        if text.len() > MAX_SOURCE_ID_BYTES {
            return Err(TableRuntimeError::InvalidConfig {
                field: "compaction_source_id",
                reason: "is too large",
            });
        }
        if text.bytes().any(|byte| byte == 0) {
            return Err(TableRuntimeError::InvalidConfig {
                field: "compaction_source_id",
                reason: "must not contain nul bytes",
            });
        }
        Ok(Self { text })
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.text
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TableCompactionSource {
    id: TableCompactionSourceId,
    rows: Vec<TableRow>,
}

impl TableCompactionSource {
    pub(crate) fn from_rows(
        id: TableCompactionSourceId,
        rows: Vec<TableRow>,
    ) -> TableRuntimeResult<Self> {
        validate_strictly_sorted_unique_rows(&rows)?;
        Ok(Self { id, rows })
    }

    pub(crate) fn from_cursor(
        id: TableCompactionSourceId,
        cursor: &mut impl TableCursor,
    ) -> TableRuntimeResult<Self> {
        let mut rows = Vec::new();
        cursor.seek_to_first()?;
        while let Some(row) = cursor.current() {
            rows.push(row.clone());
            cursor.advance()?;
        }
        Self::from_rows(id, rows)
    }

    pub(crate) const fn id(&self) -> &TableCompactionSourceId {
        &self.id
    }

    pub(crate) fn len(&self) -> usize {
        self.rows.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    fn cursor(&self) -> Box<dyn TableCursor + '_> {
        Box::new(TableCompactionRowsCursor::new(&self.rows))
    }
}

pub(crate) trait TableCompactionInput {
    fn id(&self) -> &TableCompactionSourceId;
    fn open_cursor(&self) -> TableRuntimeResult<Box<dyn TableCursor + '_>>;
}

impl TableCompactionInput for TableCompactionSource {
    fn id(&self) -> &TableCompactionSourceId {
        self.id()
    }

    fn open_cursor(&self) -> TableRuntimeResult<Box<dyn TableCursor + '_>> {
        Ok(self.cursor())
    }
}

struct TableCompactionRowsCursor<'a> {
    rows: &'a [TableRow],
    position: Option<usize>,
}

impl<'a> TableCompactionRowsCursor<'a> {
    const fn new(rows: &'a [TableRow]) -> Self {
        Self {
            rows,
            position: None,
        }
    }

    fn seek_position(&self, target: &TableInternalKeyBytes) -> Option<usize> {
        let index = match self.rows.binary_search_by(|row| row.key().cmp(target)) {
            Ok(index) | Err(index) => index,
        };
        (index < self.rows.len()).then_some(index)
    }
}

impl TableCursor for TableCompactionRowsCursor<'_> {
    fn seek_to_first(&mut self) -> TableRuntimeResult<()> {
        self.position = (!self.rows.is_empty()).then_some(0);
        Ok(())
    }

    fn seek(&mut self, target: &TableInternalKeyBytes) -> TableRuntimeResult<()> {
        self.position = self.seek_position(target);
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TableCompactionDecision {
    Keep,
    Drop { reason: TableCompactionDropReason },
}

impl TableCompactionDecision {
    pub(crate) const fn drop(reason: TableCompactionDropReason) -> Self {
        Self::Drop { reason }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TableCompactionDropReason {
    CallerSelected,
    OlderVersion,
    TombstoneElided,
    Expired,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TableCompactionDropSummary {
    reason: TableCompactionDropReason,
    rows: u64,
}

impl TableCompactionDropSummary {
    const fn new(reason: TableCompactionDropReason) -> Self {
        Self { reason, rows: 0 }
    }

    pub(crate) const fn reason(self) -> TableCompactionDropReason {
        self.reason
    }

    pub(crate) const fn rows(self) -> u64 {
        self.rows
    }

    fn increment(&mut self) {
        self.rows = self.rows.saturating_add(1);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TableCompactionRowContext<'a> {
    source_id: &'a TableCompactionSourceId,
    source_index: usize,
    source_row_index: usize,
    merged_row_index: u64,
    previous_kept_key: Option<&'a TableInternalKeyBytes>,
}

impl<'a> TableCompactionRowContext<'a> {
    pub(crate) const fn source_id(self) -> &'a TableCompactionSourceId {
        self.source_id
    }

    pub(crate) const fn source_index(self) -> usize {
        self.source_index
    }

    pub(crate) const fn source_row_index(self) -> usize {
        self.source_row_index
    }

    pub(crate) const fn merged_row_index(self) -> u64 {
        self.merged_row_index
    }

    pub(crate) const fn previous_kept_key(self) -> Option<&'a TableInternalKeyBytes> {
        self.previous_kept_key
    }
}

pub(crate) trait TableCompactionPolicy {
    fn decide(
        &mut self,
        context: &TableCompactionRowContext<'_>,
        row: &TableRow,
    ) -> TableRuntimeResult<TableCompactionDecision>;
}

impl<F> TableCompactionPolicy for F
where
    F: for<'a> FnMut(
        &TableCompactionRowContext<'a>,
        &TableRow,
    ) -> TableRuntimeResult<TableCompactionDecision>,
{
    fn decide(
        &mut self,
        context: &TableCompactionRowContext<'_>,
        row: &TableRow,
    ) -> TableRuntimeResult<TableCompactionDecision> {
        self(context, row)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TableCompactionReport {
    input_sources: usize,
    input_rows: u64,
    kept_rows: u64,
    dropped_rows: u64,
    output_tables: usize,
    output_bytes: u64,
    split_count: u64,
    peak_buffered_rows: usize,
    drop_summaries: Vec<TableCompactionDropSummary>,
}

impl TableCompactionReport {
    fn new(input_sources: usize) -> Self {
        Self {
            input_sources,
            input_rows: 0,
            kept_rows: 0,
            dropped_rows: 0,
            output_tables: 0,
            output_bytes: 0,
            split_count: 0,
            peak_buffered_rows: 0,
            drop_summaries: Vec::new(),
        }
    }

    pub(crate) const fn input_sources(&self) -> usize {
        self.input_sources
    }

    pub(crate) const fn input_rows(&self) -> u64 {
        self.input_rows
    }

    pub(crate) const fn kept_rows(&self) -> u64 {
        self.kept_rows
    }

    pub(crate) const fn dropped_rows(&self) -> u64 {
        self.dropped_rows
    }

    pub(crate) const fn output_tables(&self) -> usize {
        self.output_tables
    }

    pub(crate) const fn output_bytes(&self) -> u64 {
        self.output_bytes
    }

    pub(crate) const fn split_count(&self) -> u64 {
        self.split_count
    }

    pub(crate) const fn peak_buffered_rows(&self) -> usize {
        self.peak_buffered_rows
    }

    pub(crate) fn drop_summaries(&self) -> &[TableCompactionDropSummary] {
        &self.drop_summaries
    }

    fn record_peak_buffered_rows(&mut self, rows: usize) {
        self.peak_buffered_rows = self.peak_buffered_rows.max(rows);
    }

    fn record_keep(&mut self) {
        self.kept_rows = self.kept_rows.saturating_add(1);
    }

    fn record_drop(&mut self, reason: TableCompactionDropReason) {
        self.dropped_rows = self.dropped_rows.saturating_add(1);
        if let Some(summary) = self
            .drop_summaries
            .iter_mut()
            .find(|summary| summary.reason == reason)
        {
            summary.increment();
            return;
        }
        let mut summary = TableCompactionDropSummary::new(reason);
        summary.increment();
        self.drop_summaries.push(summary);
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TableCompactionOutput {
    artifacts: Vec<BuiltTableArtifact>,
    report: TableCompactionReport,
}

impl TableCompactionOutput {
    fn new(artifacts: Vec<BuiltTableArtifact>, report: TableCompactionReport) -> Self {
        Self { artifacts, report }
    }

    pub(crate) fn artifacts(&self) -> &[BuiltTableArtifact] {
        &self.artifacts
    }

    pub(crate) const fn report(&self) -> &TableCompactionReport {
        &self.report
    }

    pub(crate) fn into_parts(self) -> (Vec<BuiltTableArtifact>, TableCompactionReport) {
        (self.artifacts, self.report)
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct TableStreamingArtifactReport {
    output_tables: usize,
    output_bytes: u64,
    peak_buffered_rows: usize,
}

impl TableStreamingArtifactReport {
    pub(crate) const fn output_tables(&self) -> usize {
        self.output_tables
    }

    pub(crate) const fn peak_buffered_rows(&self) -> usize {
        self.peak_buffered_rows
    }

    fn record_peak_buffered_rows(&mut self, rows: usize) {
        self.peak_buffered_rows = self.peak_buffered_rows.max(rows);
    }
}

pub(crate) struct TableStreamingArtifactBuilder<F> {
    builder: ImmutableTableBuilder,
    max_rows_per_output: usize,
    pending_rows: Vec<TableRow>,
    artifacts: Vec<BuiltTableArtifact>,
    report: TableStreamingArtifactReport,
    identity_for_output: F,
}

impl<F> TableStreamingArtifactBuilder<F>
where
    F: FnMut(usize, &[TableRow]) -> TableRuntimeResult<TableIdentity>,
{
    pub(crate) fn new(
        builder_config: TableBuilderConfig,
        max_rows_per_output: usize,
        identity_for_output: F,
    ) -> TableRuntimeResult<Self> {
        if max_rows_per_output == 0 {
            return Err(TableRuntimeError::InvalidConfig {
                field: "max_rows_per_output",
                reason: "must be nonzero",
            });
        }
        Ok(Self {
            builder: ImmutableTableBuilder::new(builder_config)?,
            max_rows_per_output,
            pending_rows: Vec::new(),
            artifacts: Vec::new(),
            report: TableStreamingArtifactReport::default(),
            identity_for_output,
        })
    }

    pub(crate) fn push(&mut self, row: TableRow) -> TableRuntimeResult<()> {
        if self.pending_rows.len() >= self.max_rows_per_output {
            self.flush()?;
        }
        self.pending_rows.push(row);
        self.report
            .record_peak_buffered_rows(self.pending_rows.len());
        Ok(())
    }

    pub(crate) fn finish(
        mut self,
    ) -> TableRuntimeResult<(Vec<BuiltTableArtifact>, TableStreamingArtifactReport)> {
        self.flush()?;
        Ok((self.artifacts, self.report))
    }

    fn flush(&mut self) -> TableRuntimeResult<()> {
        if self.pending_rows.is_empty() {
            return Ok(());
        }
        let output_index = self.artifacts.len();
        let rows = std::mem::take(&mut self.pending_rows);
        let identity = (self.identity_for_output)(output_index, &rows)?;
        let artifact = build_table_artifact_from_rows(&self.builder, identity, &rows)?;
        self.report.output_bytes = self
            .report
            .output_bytes
            .saturating_add(artifact.byte_count());
        self.artifacts.push(artifact);
        self.report.output_tables = self.artifacts.len();
        Ok(())
    }
}

fn compact_table_inputs(
    compactor: &TableCompactor,
    output_identity_seed: &TableIdentity,
    sources: &[&dyn TableCompactionInput],
    policy: &mut (impl TableCompactionPolicy + ?Sized),
) -> TableRuntimeResult<TableCompactionOutput> {
    let builder = ImmutableTableBuilder::new(compactor.builder_config)?;
    let mut report = TableCompactionReport::new(sources.len());
    let source_identity = output_source_identity(sources);
    let mut merged = TableCompactionMergeCursor::new(sources)?;
    validate_no_global_duplicate_internal_keys(&mut merged)?;
    merged.seek_to_first()?;

    let target_output_bytes = compactor.config.target_output_bytes();
    require_nonzero_target_output_bytes(target_output_bytes)?;
    let mut output_rows = Vec::new();
    let mut output_approximate_bytes = 0;
    let mut output_last_physical_key = None;
    let mut artifacts = Vec::new();
    let mut previous_kept_key: Option<TableInternalKeyBytes> = None;

    while let Some(current) = merged.current().map(|current| {
        (
            current.source_id,
            current.source_index,
            current.source_row_index,
            current.row.clone(),
        )
    }) {
        let (source_id, source_index, source_row_index, row) = current;
        report.input_rows = report.input_rows.saturating_add(1);

        let context = TableCompactionRowContext {
            source_id,
            source_index,
            source_row_index,
            merged_row_index: report.input_rows.saturating_sub(1),
            previous_kept_key: previous_kept_key.as_ref(),
        };
        match policy.decide(&context, &row)? {
            TableCompactionDecision::Keep => {
                if should_split_before(
                    &output_rows,
                    output_approximate_bytes,
                    output_last_physical_key.as_deref(),
                    target_output_bytes,
                    &row,
                )? {
                    build_pending_output(
                        &builder,
                        output_identity_seed,
                        source_identity,
                        &mut output_rows,
                        &mut output_approximate_bytes,
                        &mut output_last_physical_key,
                        &mut artifacts,
                        &mut report,
                        compactor.config.max_output_tables(),
                    )?;
                }
                let kept_key = row.key().clone();
                push_pending_row(
                    &mut output_rows,
                    &mut output_approximate_bytes,
                    &mut output_last_physical_key,
                    row,
                )?;
                report.record_peak_buffered_rows(output_rows.len());
                previous_kept_key = Some(kept_key);
                report.record_keep();
            }
            TableCompactionDecision::Drop { reason } => {
                report.record_drop(reason);
            }
        }
        merged.advance()?;
    }

    build_pending_output(
        &builder,
        output_identity_seed,
        source_identity,
        &mut output_rows,
        &mut output_approximate_bytes,
        &mut output_last_physical_key,
        &mut artifacts,
        &mut report,
        compactor.config.max_output_tables(),
    )?;

    perf_trace::record_table_compaction_peak_buffered_rows(report.peak_buffered_rows());

    Ok(TableCompactionOutput::new(artifacts, report))
}

fn validate_no_global_duplicate_internal_keys(
    merged: &mut TableCompactionMergeCursor<'_>,
) -> TableRuntimeResult<()> {
    let mut previous_key: Option<TableInternalKeyBytes> = None;
    while let Some(current) = merged.current() {
        if let Some(previous) = &previous_key {
            if previous == current.row().key() {
                return Err(TableRuntimeError::DuplicateInternalKey {
                    key: current.row().encoded_key().to_vec(),
                });
            }
        }
        previous_key = Some(current.row().key().clone());
        merged.advance()?;
    }
    Ok(())
}

pub(crate) struct TableCompactionMergeCursor<'a> {
    sources: Vec<TableCompactionSourceCursor<'a>>,
    heap: BinaryHeap<TableCompactionHeapItem>,
}

struct TableCompactionSourceCursor<'a> {
    source_id: &'a TableCompactionSourceId,
    source_index: usize,
    source_row_index: usize,
    last_key: Option<TableInternalKeyBytes>,
    cursor: Box<dyn TableCursor + 'a>,
}

pub(crate) struct TableCompactionMergedRow<'a> {
    source_id: &'a TableCompactionSourceId,
    source_index: usize,
    source_row_index: usize,
    row: &'a TableRow,
}

impl<'a> TableCompactionMergedRow<'a> {
    pub(crate) const fn row(&self) -> &'a TableRow {
        self.row
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct TableCompactionHeapItem {
    key: TableInternalKeyBytes,
    source_index: usize,
}

impl Ord for TableCompactionHeapItem {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .key
            .cmp(&self.key)
            .then_with(|| other.source_index.cmp(&self.source_index))
    }
}

impl PartialOrd for TableCompactionHeapItem {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<'a> TableCompactionMergeCursor<'a> {
    pub(crate) fn new(sources: &'a [&'a dyn TableCompactionInput]) -> TableRuntimeResult<Self> {
        let mut cursors = Vec::with_capacity(sources.len());
        let mut heap = BinaryHeap::new();
        for (source_index, source) in sources.iter().enumerate() {
            let mut cursor = source.open_cursor()?;
            cursor.seek_to_first()?;
            if let Some(key) = cursor.current_key() {
                heap.push(TableCompactionHeapItem {
                    key: key.clone(),
                    source_index,
                });
            }
            cursors.push(TableCompactionSourceCursor {
                source_id: source.id(),
                source_index,
                source_row_index: 0,
                last_key: cursor.current_key().cloned(),
                cursor,
            });
        }
        Ok(Self {
            sources: cursors,
            heap,
        })
    }

    pub(crate) fn seek_to_first(&mut self) -> TableRuntimeResult<()> {
        self.heap.clear();
        for (source_index, source) in self.sources.iter_mut().enumerate() {
            source.cursor.seek_to_first()?;
            source.source_row_index = 0;
            source.last_key = source.cursor.current_key().cloned();
            if let Some(key) = &source.last_key {
                self.heap.push(TableCompactionHeapItem {
                    key: key.clone(),
                    source_index,
                });
            }
        }
        Ok(())
    }

    pub(crate) fn current(&self) -> Option<TableCompactionMergedRow<'_>> {
        let selected = self.heap.peek()?;
        let source = self.sources.get(selected.source_index)?;
        let row = source.cursor.current()?;
        Some(TableCompactionMergedRow {
            source_id: source.source_id,
            source_index: source.source_index,
            source_row_index: source.source_row_index,
            row,
        })
    }

    pub(crate) fn advance(&mut self) -> TableRuntimeResult<()> {
        let Some(selected) = self.heap.pop() else {
            return Ok(());
        };
        let source =
            self.sources
                .get_mut(selected.source_index)
                .ok_or(TableRuntimeError::InvalidRange {
                    field: "compaction_source_index",
                })?;
        source.cursor.advance()?;
        source.source_row_index = source.source_row_index.saturating_add(1);
        if let Some(key) = source.cursor.current_key() {
            validate_compaction_source_key_order(source.last_key.as_ref(), key)?;
            source.last_key = Some(key.clone());
            self.heap.push(TableCompactionHeapItem {
                key: key.clone(),
                source_index: selected.source_index,
            });
        }
        Ok(())
    }
}

fn validate_compaction_source_key_order(
    previous: Option<&TableInternalKeyBytes>,
    current: &TableInternalKeyBytes,
) -> TableRuntimeResult<()> {
    let Some(previous) = previous else {
        return Ok(());
    };
    match previous.cmp(current) {
        Ordering::Less => Ok(()),
        Ordering::Equal => Err(TableRuntimeError::DuplicateInternalKey {
            key: current.as_slice().to_vec(),
        }),
        Ordering::Greater => Err(TableRuntimeError::InvalidRowOrder {
            previous: previous.as_slice().to_vec(),
            current: current.as_slice().to_vec(),
        }),
    }
}

fn require_nonzero_target_output_bytes(target_output_bytes: u64) -> TableRuntimeResult<()> {
    if target_output_bytes == 0 {
        return Err(TableRuntimeError::InvalidConfig {
            field: "target_output_bytes",
            reason: "must be nonzero",
        });
    }
    Ok(())
}

fn should_split_before(
    pending_rows: &[TableRow],
    pending_approximate_bytes: u64,
    pending_last_physical_key: Option<&[u8]>,
    target_output_bytes: u64,
    row: &TableRow,
) -> TableRuntimeResult<bool> {
    if pending_rows.is_empty() {
        return Ok(false);
    }
    let next_size = u64::try_from(row.approximate_size_bytes()).map_err(|_| {
        TableRuntimeError::InvalidRange {
            field: "row_approximate_size",
        }
    })?;
    let would_cross_target =
        pending_approximate_bytes.saturating_add(next_size) > target_output_bytes;
    if !would_cross_target {
        return Ok(false);
    }
    Ok(pending_last_physical_key != Some(physical_key_bytes(row).as_slice()))
}

fn push_pending_row(
    pending_rows: &mut Vec<TableRow>,
    pending_approximate_bytes: &mut u64,
    pending_last_physical_key: &mut Option<Vec<u8>>,
    row: TableRow,
) -> TableRuntimeResult<()> {
    let row_size = u64::try_from(row.approximate_size_bytes()).map_err(|_| {
        TableRuntimeError::InvalidRange {
            field: "row_approximate_size",
        }
    })?;
    *pending_approximate_bytes = pending_approximate_bytes.saturating_add(row_size);
    *pending_last_physical_key = Some(physical_key_bytes(&row));
    pending_rows.push(row);
    Ok(())
}

fn build_pending_output(
    builder: &ImmutableTableBuilder,
    output_identity_seed: &TableIdentity,
    source_identity: u64,
    pending_rows: &mut Vec<TableRow>,
    pending_approximate_bytes: &mut u64,
    pending_last_physical_key: &mut Option<Vec<u8>>,
    artifacts: &mut Vec<BuiltTableArtifact>,
    report: &mut TableCompactionReport,
    max_output_tables: usize,
) -> TableRuntimeResult<()> {
    if pending_rows.is_empty() {
        return Ok(());
    }
    if artifacts.len() >= max_output_tables {
        return Err(TableRuntimeError::InvalidRange {
            field: "max_output_tables",
        });
    }
    let output_index = artifacts.len();
    let rows = std::mem::take(pending_rows);
    *pending_approximate_bytes = 0;
    *pending_last_physical_key = None;
    let identity = output_identity(output_identity_seed, source_identity, output_index)?;
    let artifact = build_table_artifact_from_rows(builder, identity, &rows)?;
    report.output_bytes = report.output_bytes.saturating_add(artifact.byte_count());
    artifacts.push(artifact);
    report.output_tables = artifacts.len();
    report.split_count = report.output_tables.saturating_sub(1) as u64;
    Ok(())
}

fn build_table_artifact_from_rows(
    builder: &ImmutableTableBuilder,
    identity: TableIdentity,
    rows: &[TableRow],
) -> TableRuntimeResult<BuiltTableArtifact> {
    builder.build_from_rows(identity, rows)
}

fn output_identity(
    seed: &TableIdentity,
    source_identity: u64,
    output_index: usize,
) -> TableRuntimeResult<TableIdentity> {
    TableIdentity::new(format!(
        "{}-{source_identity:016x}-{output_index:08x}",
        seed.as_str()
    ))
}

fn output_source_identity(sources: &[&dyn TableCompactionInput]) -> u64 {
    let mut hash = OUTPUT_IDENTITY_METADATA_HASH_OFFSET;
    hash_metadata_u64(&mut hash, sources.len() as u64);
    for source in sources {
        hash_metadata_bytes(&mut hash, source.id().as_str().as_bytes());
    }
    hash
}

fn hash_metadata_u64(hash: &mut u64, value: u64) {
    hash_metadata_bytes(hash, &value.to_le_bytes());
}

fn hash_metadata_bytes(hash: &mut u64, bytes: &[u8]) {
    hash_metadata_u64_raw(hash, bytes.len() as u64);
    for byte in bytes {
        *hash ^= u64::from(*byte);
        *hash = hash.wrapping_mul(OUTPUT_IDENTITY_METADATA_HASH_PRIME);
    }
}

fn hash_metadata_u64_raw(hash: &mut u64, value: u64) {
    for byte in value.to_le_bytes() {
        *hash ^= u64::from(byte);
        *hash = hash.wrapping_mul(OUTPUT_IDENTITY_METADATA_HASH_PRIME);
    }
}

fn physical_key_bytes(row: &TableRow) -> Vec<u8> {
    TablePhysicalKeyBytes::from_row(row.row())
        .as_slice()
        .to_vec()
}
