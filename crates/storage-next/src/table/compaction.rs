//! Generic table compaction.

use super::{
    validate_strictly_sorted_unique_rows, BuiltTableArtifact, ImmutableTableBuilder,
    TableBuilderConfig, TableCompactionConfig, TableCursor, TableIdentity, TableInternalKeyBytes,
    TablePhysicalKeyBytes, TableRow, TableRuntimeError, TableRuntimeResult,
};

const MAX_SOURCE_ID_BYTES: usize = 128;
const OUTPUT_IDENTITY_HASH_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const OUTPUT_IDENTITY_HASH_PRIME: u64 = 0x0000_0100_0000_01b3;

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
        compact_tables(self, output_identity_seed, sources, policy)
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

    pub(crate) fn rows(&self) -> &[TableRow] {
        &self.rows
    }

    pub(crate) fn len(&self) -> usize {
        self.rows.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.rows.is_empty()
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

    pub(crate) fn drop_summaries(&self) -> &[TableCompactionDropSummary] {
        &self.drop_summaries
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

fn compact_tables(
    compactor: &TableCompactor,
    output_identity_seed: &TableIdentity,
    sources: &[TableCompactionSource],
    policy: &mut (impl TableCompactionPolicy + ?Sized),
) -> TableRuntimeResult<TableCompactionOutput> {
    let builder = ImmutableTableBuilder::new(compactor.builder_config)?;
    let mut report = TableCompactionReport::new(sources.len());
    let mut merged = merged_rows(sources);
    merged.sort_by(
        |(left_source_index, left_row_index, left_row),
         (right_source_index, right_row_index, right_row)| {
            left_row
                .key()
                .cmp(right_row.key())
                .then_with(|| left_source_index.cmp(right_source_index))
                .then_with(|| left_row_index.cmp(right_row_index))
        },
    );
    validate_no_global_duplicate_internal_keys(&merged)?;

    let target_output_bytes = compactor.config.target_output_bytes();
    require_nonzero_target_output_bytes(target_output_bytes)?;
    let mut output_rows = Vec::new();
    let mut output_approximate_bytes = 0;
    let mut output_last_physical_key = None;
    let mut artifacts = Vec::new();
    let mut previous_kept_key: Option<TableInternalKeyBytes> = None;

    for (source_index, source_row_index, row) in merged {
        report.input_rows = report.input_rows.saturating_add(1);

        let context = TableCompactionRowContext {
            source_id: sources[source_index].id(),
            source_index,
            source_row_index,
            merged_row_index: report.input_rows.saturating_sub(1),
            previous_kept_key: previous_kept_key.as_ref(),
        };
        match policy.decide(&context, row)? {
            TableCompactionDecision::Keep => {
                if should_split_before(
                    &output_rows,
                    output_approximate_bytes,
                    output_last_physical_key.as_deref(),
                    target_output_bytes,
                    row,
                )? {
                    build_pending_output(
                        &builder,
                        output_identity_seed,
                        &mut output_rows,
                        &mut output_approximate_bytes,
                        &mut output_last_physical_key,
                        &mut artifacts,
                        &mut report,
                        compactor.config.max_output_tables(),
                    )?;
                }
                push_pending_row(
                    &mut output_rows,
                    &mut output_approximate_bytes,
                    &mut output_last_physical_key,
                    row.clone(),
                )?;
                previous_kept_key = Some(row.key().clone());
                report.record_keep();
            }
            TableCompactionDecision::Drop { reason } => {
                report.record_drop(reason);
            }
        }
    }

    build_pending_output(
        &builder,
        output_identity_seed,
        &mut output_rows,
        &mut output_approximate_bytes,
        &mut output_last_physical_key,
        &mut artifacts,
        &mut report,
        compactor.config.max_output_tables(),
    )?;

    Ok(TableCompactionOutput::new(artifacts, report))
}

fn validate_no_global_duplicate_internal_keys(
    merged: &[(usize, usize, &TableRow)],
) -> TableRuntimeResult<()> {
    let mut previous_key: Option<&TableInternalKeyBytes> = None;
    for (_, _, row) in merged {
        if let Some(previous) = previous_key {
            if previous == row.key() {
                return Err(TableRuntimeError::DuplicateInternalKey {
                    key: row.encoded_key().to_vec(),
                });
            }
        }
        previous_key = Some(row.key());
    }
    Ok(())
}

fn merged_rows(sources: &[TableCompactionSource]) -> Vec<(usize, usize, &TableRow)> {
    let capacity = sources.iter().map(TableCompactionSource::len).sum();
    let mut merged = Vec::with_capacity(capacity);
    for (source_index, source) in sources.iter().enumerate() {
        for (source_row_index, row) in source.rows().iter().enumerate() {
            merged.push((source_index, source_row_index, row));
        }
    }
    merged
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
    let identity = output_identity(output_identity_seed, output_index, &rows)?;
    let artifact = builder.build_from_rows(identity, &rows)?;
    report.output_bytes = report.output_bytes.saturating_add(artifact.byte_count());
    artifacts.push(artifact);
    report.output_tables = artifacts.len();
    report.split_count = report.output_tables.saturating_sub(1) as u64;
    Ok(())
}

fn output_identity(
    seed: &TableIdentity,
    output_index: usize,
    rows: &[TableRow],
) -> TableRuntimeResult<TableIdentity> {
    let fingerprint = output_rows_fingerprint(rows);
    TableIdentity::new(format!(
        "{}-{fingerprint:016x}-{output_index:08x}",
        seed.as_str()
    ))
}

fn output_rows_fingerprint(rows: &[TableRow]) -> u64 {
    let mut hash = OUTPUT_IDENTITY_HASH_OFFSET;
    hash_u64(&mut hash, rows.len() as u64);
    for row in rows {
        hash_bytes(&mut hash, row.encoded_key());
        hash_u64(&mut hash, row.commit_timestamp().as_micros());
        hash_u64(&mut hash, row.expires_at().as_micros());
        hash_bytes(&mut hash, &[u8::from(row.is_tombstone())]);
        hash_bytes(&mut hash, row.value());
    }
    hash
}

fn hash_u64(hash: &mut u64, value: u64) {
    hash_bytes(hash, &value.to_le_bytes());
}

fn hash_bytes(hash: &mut u64, bytes: &[u8]) {
    hash_u64_raw(hash, bytes.len() as u64);
    for byte in bytes {
        *hash ^= u64::from(*byte);
        *hash = hash.wrapping_mul(OUTPUT_IDENTITY_HASH_PRIME);
    }
}

fn hash_u64_raw(hash: &mut u64, value: u64) {
    for byte in value.to_le_bytes() {
        *hash ^= u64::from(byte);
        *hash = hash.wrapping_mul(OUTPUT_IDENTITY_HASH_PRIME);
    }
}

fn physical_key_bytes(row: &TableRow) -> Vec<u8> {
    TablePhysicalKeyBytes::from_row(row.row())
        .as_slice()
        .to_vec()
}
