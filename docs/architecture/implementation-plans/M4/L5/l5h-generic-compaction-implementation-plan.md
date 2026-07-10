# L5H Implementation Plan: Generic Table Compaction

Status: draft implementation plan

Parent plans:

1. `docs/architecture/implementation-plans/m4-m4t-implementation-plan.md`
2. `docs/architecture/implementation-plans/m4-l5-table-runtime-implementation-plan.md`
3. `docs/architecture/implementation-plans/M4/l5b-row-key-adapters-implementation-plan.md`
4. `docs/architecture/implementation-plans/M4/l5d-raw-cursors-merge-cursor-implementation-plan.md`
5. `docs/architecture/implementation-plans/M4/l5e-immutable-table-builder-implementation-plan.md`
6. `docs/architecture/implementation-plans/M4/l5f-immutable-table-reader-implementation-plan.md`
7. `docs/architecture/implementation-plans/M4/l5h-generic-compaction-test-plan.md`

## Goal

Port generic table-compaction mechanics into storage-next L5 without porting old
branch, level, retention, or lifecycle policy.

L5H must let higher layers take already-selected L5 table sources and produce
new immutable table artifacts by:

1. consuming raw sorted L5 cursors;
2. merging rows in canonical encoded internal-key order;
3. asking a caller-supplied policy whether each row is kept or dropped;
4. splitting output at table-runtime boundaries supplied in config;
5. building M3G immutable table artifacts through the L5 table builder;
6. reporting exact input, output, drop, split, and byte facts;
7. preserving every row unless the caller policy explicitly drops it;
8. staying independent from branch topology, object names, table manifests,
   WAL durability, lifecycle scheduling, snapshot safety, and product payload
   meaning.

L5H is not the compaction scheduler. It is the deterministic table-local
execution primitive that L6/L8 can call after they have already decided which
tables are safe to compact and which retention decisions are legal.

## Inputs

1. `docs/architecture/storage/l5-table-runtime.md`
2. `docs/architecture/implementation-plans/m4-l5-table-runtime-implementation-plan.md`
3. `docs/architecture/implementation-plans/m4-l5-table-runtime-test-plan.md`
4. `docs/architecture/implementation-plans/M4/m4-l5-porting-log.md`
5. `crates/storage/src/compaction.rs`
6. `crates/storage/src/segmented/compaction.rs`
7. `crates/storage/src/merge_iter.rs`
8. `crates/storage/src/segment_builder.rs`
9. `crates/storage-next/src/table/compaction.rs`
10. `crates/storage-next/src/table/cursor.rs`
11. `crates/storage-next/src/table/builder.rs`
12. `crates/storage-next/src/table/reader.rs`
13. `crates/storage-next/src/table/key.rs`
14. `crates/storage-next/src/table/facts.rs`
15. `crates/storage-next/src/table/config.rs`
16. `crates/storage-next/src/format/table/`

## Existing-Code Source Map

| Current file | Relevant evidence | L5H porting rule |
|---|---|---|
| `crates/storage/src/compaction.rs` | `CompactionIterator` proves the historical row-pruning cases: floor handling, tombstones, max versions, snapshots, TTL, and event exemptions. | Reuse as regression input only. Do not embed these decisions in L5. Convert each pruning case into an explicit policy decision supplied by the caller. |
| `crates/storage/src/segmented/compaction.rs` | Branch compaction selects segments, merges sources, writes output files, publishes manifests, then reclaims old files. It also has level target, overlap, and grandparent split evidence. | Extract only source merge, output splitting, build-result accounting, and failure cleanup semantics. Leave branch selection, manifest install, object publication, old-file retention, and overlap policy to L6/L8. |
| `crates/storage/src/segment_builder.rs` | `SplittingSegmentBuilder` shows output splitting at size and key-boundary predicates. | Port the split mechanics, not filesystem path creation, old segment bytes, rate limiting, or direct writes. L5H builds in-memory M3G artifacts. |
| `crates/storage/src/merge_iter.rs` | Raw k-way merge over sorted sources. | Reuse through L5D `MergeTableCursor`; do not reintroduce MVCC latest selection or source-level priority policy. |
| `crates/storage-next/src/table/cursor.rs` | `TableCursor` and `MergeTableCursor` provide raw sorted cursor traversal. | L5H consumes this cursor surface and adds no upper-layer read semantics. |
| `crates/storage-next/src/table/builder.rs` | `ImmutableTableBuilder` builds validated M3G artifacts from sorted unique L5 rows. | L5H must produce output through this builder so all table bytes remain M3G. |
| `crates/storage-next/src/table/config.rs` | `TableCompactionConfig` already carries target output bytes and max output tables. | Reuse and extend only if L5H needs explicit split predicates or row-count limits. |
| `crates/storage-next/src/table/facts.rs` | `TableRuntimeStats` already has compaction input/output row counters. | Extend facts only with mechanical compaction outcome fields that higher layers need. |

## Scope

L5H implements:

1. `table/compaction.rs` as a real production module;
2. a compaction request type over sorted L5 cursor sources;
3. a caller-supplied row policy interface;
4. a policy decision enum that makes keep/drop explicit;
5. a deterministic row merge pipeline using L5D cursor mechanics;
6. validation that compaction inputs are sorted and duplicate-free within each
   source;
7. a documented duplicate handling rule across sources;
8. output buffering into one or more sorted row batches;
9. output splitting by approximate bytes and optional caller split boundaries;
10. M3G table artifact creation through `ImmutableTableBuilder`;
11. compaction result facts, including input rows, kept rows, dropped rows,
    output table count, output bytes, and split reasons;
12. typed errors for invalid inputs, policy failures, build failures, and output
    table limit violations;
13. direct unit and generated property tests;
14. source guards proving L5H does not import upper storage layers;
15. a porting-log entry for old compaction mechanics.

L5H does not implement:

1. compaction scheduling;
2. branch-local level selection;
3. inherited table or copy-on-write semantics;
4. fork gates or branch-id rewriting;
5. commit version allocation;
6. snapshot-safe floors;
7. max-version retention decisions;
8. tombstone elision decisions;
9. TTL expiry decisions;
10. product table or event-log exemptions;
11. table object naming or durable publication;
12. manifest creation or install;
13. old-table deletion, quarantine, garbage collection, or reachability proofs;
14. WAL truncation or checkpoint coordination;
15. direct filesystem paths, backend APIs, object-layout calls, or L4 service
    calls;
16. old `STRAKV` segment byte compatibility.

## Boundary Rule

L5H may execute a drop. It must not decide that the drop is safe.

Every dropped row must be traceable to a policy callback result that names the
row and the reason. Keep-all policy is the default model for correctness tests.
Under keep-all policy, compaction is an identity transform modulo k-way merge
and output splitting.

This means historical behavior such as:

1. keeping one below-floor version;
2. dropping below-floor tombstones only at the bottommost level;
3. preserving above-floor tombstones in non-bottommost compaction;
4. protecting rows above a snapshot floor;
5. dropping expired TTL rows;
6. exempting event rows;

must appear only in tests as caller-supplied policy choices or as deferred L6/L8
rules. None of those rules belong inside L5H.

## Proposed Type Surface

The shipped M4 shape is intentionally smaller than the old segmented
compaction API. Names may change during implementation if the responsibilities
remain intact.

### `TableCompactor` And `TableCompactionConfig`

Primary shipped shape:

```text
TableCompactor::new(
    config: TableCompactionConfig,
    builder_config: TableBuilderConfig,
) -> TableRuntimeResult<Self>

TableCompactor::compact(
    &self,
    output_identity_seed: &TableIdentity,
    sources: &[TableCompactionSource],
    policy: &mut impl TableCompactionPolicy,
) -> TableRuntimeResult<TableCompactionOutput>

TableCompactionConfig {
    target_output_bytes: u64,
    max_output_tables: usize,
}
```

Rules:

1. empty source sets are rejected or return an explicit no-output result, but
   the contract must choose one behavior and test it;
2. each source must expose sorted, duplicate-free encoded internal keys;
3. source identity is diagnostic only and must not encode branch policy;
4. output identities must be caller-supplied or mechanically derived from the
   request seed plus output ordinal;
5. request validation must happen before any output table is built;
6. split decisions use the configured approximate row-size target and maximum
   output table count.

### `TableCompactionSource`

Suggested shape:

```text
TableCompactionSource<'a> {
    source_id: TableCompactionSourceId,
    cursor: Box<dyn TableCursor + 'a>,
    estimated_rows: Option<u64>,
    estimated_bytes: Option<u64>,
}
```

Rules:

1. source ids are opaque debug labels, not object names;
2. cursors are consumed from first row to end;
3. a source that yields out-of-order rows returns `InvalidRowOrder`;
4. a source that yields a duplicate key within itself returns
   `DuplicateInternalKey`;
5. exact duplicate internal keys across sources follow the documented
   cross-source rule below.

### Cross-Source Duplicate Rule

The safest M4 rule is to reject exact duplicate internal keys across all input
sources before building output.

Rationale:

1. M3G table artifacts require sorted unique internal keys;
2. duplicate exact internal keys are not an L5 retention question because they
   cannot both be represented in one output table;
3. resolving duplicates by source priority would import branch/level/newness
   policy into L5;
4. L6 can pre-resolve duplicates before calling L5H if it has a valid priority
   fact.

If implementation chooses a different rule, it must still be explicit,
deterministic, and policy-provided. Silent source-index priority is not allowed.

### `TableCompactionPolicy`

Suggested shape:

```text
trait TableCompactionPolicy {
    fn decide(
        &mut self,
        context: &TableCompactionRowContext<'_>,
        row: &TableRow,
    ) -> TableRuntimeResult<TableCompactionDecision>;
}
```

The row context may include only mechanical information:

1. source id;
2. source ordinal;
3. merged row ordinal;
4. previous kept key;
5. physical-key group ordinal if already computed mechanically;
6. input row facts that are already part of `TableRow`.

The row context must not include:

1. branch topology;
2. current snapshot set;
3. branch level;
4. object names;
5. manifest facts;
6. product capability state.

Suggested decisions:

```text
TableCompactionDecision::Keep
TableCompactionDecision::Drop { reason: TableCompactionDropReason }
```

Drop reasons should be a closed L5 vocabulary for tests and observability:

1. `CallerSelected`;
2. `OlderVersion`;
3. `TombstoneElided`;
4. `Expired`;
5. `Custom(&'static str)` only if bounded and not product specific.

The reason records why the caller asked L5H to drop a row. It is not proof that
the drop was safe.

### Future Split Request Surface

Caller-provided split predicates and overlap boundaries are not part of the M4
L5H API. If L6 later needs pure split facts from overlap analysis, it can grow
a request wrapper around the shipped compactor shape. One possible future shape
is:

```text
TableCompactionRequest<'a> {
    config: TableCompactionConfig,
    builder_config: TableBuilderConfig,
    output_identity_seed: TableIdentity,
    sources: Vec<TableCompactionSource>,
    policy: &'a mut dyn TableCompactionPolicy,
    split_policy: TableCompactionSplitPolicy,
}

TableCompactionSplitPolicy {
    target_output_bytes: u64,
    max_output_tables: usize,
    split_at_physical_key_boundary: bool,
    force_split: Option<fn(&TableCompactionSplitContext<'_>) -> bool>,
}
```

The M4 implementation uses `TableCompactionConfig` only:

1. target output byte estimate;
2. max output table count;
3. never split before the first row;
4. split only between rows;
5. prefer splitting at physical-key boundaries so one physical key's versions
   are not split across tables when possible.

Grandparent-overlap splitting and caller-provided split boundaries from old
segmented compaction are explicitly deferred. The caller may later supply a
pure split predicate when L6 has the necessary overlap facts.

### `TableCompactionOutput`

Suggested shape:

```text
TableCompactionOutput {
    artifacts: Vec<BuiltTableArtifact>,
    report: TableCompactionReport,
}
```

Suggested report fields:

```text
TableCompactionReport {
    input_sources: usize,
    input_rows: u64,
    kept_rows: u64,
    dropped_rows: u64,
    output_tables: usize,
    output_bytes: u64,
    split_count: u64,
    drop_reasons: Vec<TableCompactionDropSummary>,
}
```

Rules:

1. report counters are mechanical and deterministic;
2. output bytes are the sum of M3G artifact byte lengths;
3. output table facts come from `BuiltTableArtifact`;
4. no old input objects are reported as deleted or reclaimable by L5H;
5. output artifacts are returned to the caller for L4/L6 publication.

## Execution Algorithm

Use this baseline algorithm unless implementation discovers a simpler local
fit:

1. Validate request config and source count.
2. Seek every source cursor to first.
3. Wrap sources in `MergeTableCursor` or an equivalent L5D cursor merge.
4. Iterate merged rows in encoded internal-key order.
5. Validate strict global key progression.
6. For each row, increment input count and call policy.
7. Drop only when policy returns `Drop`.
8. Add kept rows to the current output buffer.
9. When the buffer is nonempty and the next row would cross a split boundary,
   build the current buffer into a M3G artifact and start a new buffer.
10. Refuse to exceed `max_output_tables`.
11. After iteration, build the final nonempty buffer.
12. Decode validation is already performed by `ImmutableTableBuilder`, but L5H
    should still assert every produced artifact has facts matching the report.
13. Return artifacts and report.

The first implementation may buffer output rows before building each table.
Streaming table build is a later optimization because the current L5 builder
accepts sorted slices and returns in-memory bytes. The buffering contract must
be documented so L8 can reason about large compactions.

## Output Splitting

Use approximate row sizes for pre-build split decisions and actual artifact
bytes for the final report.

Rules:

1. target bytes must be nonzero;
2. one row larger than target is allowed as a single-row output table;
3. split decisions occur only between rows;
4. output rows remain sorted globally across all artifacts;
5. no output artifact is empty;
6. output table count must not exceed `max_output_tables`;
7. split by approximate bytes may produce actual artifact bytes slightly above
   target; this is acceptable and must be documented;
8. tests should cover both approximate split behavior and actual M3G decoding.

Physical-key boundary preference:

1. If the next row has the same physical key as the current output table's last
   row, prefer not to split even if the approximate target is crossed.
2. If a single physical-key group exceeds the target, allow the oversized table
   rather than splitting inside the group.
3. This is a mechanical grouping rule, not MVCC latest selection.

If physical-key grouping proves too much for the first implementation, the
plan may choose pure row-boundary splitting. That choice must be called out in
the test plan and the L6 overlap plan.

## Error Handling

Use existing `TableRuntimeError` variants where they fit:

1. `InvalidConfig` for bad config;
2. `InvalidRowOrder` for unsorted source or global output order;
3. `DuplicateInternalKey` for duplicate exact internal keys;
4. `InvalidRange` for row count, byte count, source count, or output count
   violations;
5. `BuildFormat` and `DecodeFormat` through the builder path;
6. `CompactionPolicy` for policy callback errors.

Add new error variants only if the existing vocabulary cannot express the
failure without hiding meaning.

Policy errors abort the compaction before any result is returned. Because L5H
does not publish objects, aborting leaves no durable partial state.

## Implementation Steps

### L5H-A: Source Audit And Porting Boundary

1. Read old `CompactionIterator` tests and classify each behavior as
   L5-mechanical, caller-policy, L6 branch policy, L8 lifecycle policy, or
   retired.
2. Read `SplittingSegmentBuilder` tests and extract split mechanics that still
   apply to in-memory M3G artifacts.
3. Add a porting-log entry summarizing what moved to L5H and what remains
   deferred.

Exit: the porting log has no ambiguous "port later" bucket for old compaction
semantics.

### L5H-B: Compaction API Skeleton

1. Replace the placeholder `table/compaction.rs`.
2. Add compactor, config, source, policy, decision, output, and report types.
3. Re-export the L5H surfaces from `table/mod.rs` with `pub(crate)` visibility.
4. Validate config, empty sources, and output table limits.

Exit: the API compiles with no behavior and no imports above L5.

### L5H-C: Keep-All Merge Execution

1. Implement keep-all compaction over one source.
2. Implement keep-all compaction over multiple sources using L5D merge.
3. Validate per-source and global row order.
4. Reject exact duplicate internal keys.
5. Return a report with input and kept counts.

Exit: keep-all compaction is identity modulo merge order and no output build
has been added yet.

### L5H-D: Policy Decisions

1. Call the policy for every merged row.
2. Record drop decisions and reasons.
3. Prove tombstones, expired-looking rows, old versions, and product-shaped
   payload bytes are kept under keep-all policy.
4. Route policy callback errors as `CompactionPolicy`.

Exit: L5H drops exactly rows the policy selected and no others.

### L5H-E: Output Artifact Build

1. Build kept rows with `ImmutableTableBuilder`.
2. Produce one artifact when no split is needed.
3. Verify produced artifacts decode through L3 via builder facts.
4. Return actual byte counts in the report.
5. Reject all-drop results or return zero artifacts according to the chosen
   contract.

Exit: L5H produces valid M3G artifacts from compacted rows.

### L5H-F: Output Splitting

1. Split output by `TableCompactionConfig::target_output_bytes`.
2. Enforce `max_output_tables`.
3. Choose and document physical-key-boundary behavior.
4. Record split count and output-table count.
5. Test oversized single-row output.

Exit: output splitting is deterministic, bounded, and table-builder validated.

### L5H-G: Testkit And Generated Models

1. Extend `crates/storage-next/src/testkit/table_runtime.rs` with generated
   compaction cases.
2. Add property tests under `crates/storage-next/tests/table_runtime_properties.rs`.
3. Extend source guards for compaction vocabulary and imports.
4. Ensure tests run with default features, `--no-default-features --features
   testkit`, and wasm testkit check.

Exit: generated tests compare L5H against an independent sorted-vector model.

### L5H-H: Documentation And Closeout

1. Update `m4-l5-porting-log.md`.
2. Add a short API note documenting buffering and policy boundaries.
3. Run full L5 table tests, property tests, source guard, clippy, fmt, and
   diff checks.

Exit: L5H is ready for L5I object-backed reader handoff and later L6/L8 policy
integration.

## Deferred Work

1. Streaming compaction output build without buffering all rows.
2. Async or backpressure-aware compaction.
3. Rate limiting.
4. Branch level selection.
5. Grandparent overlap splitting.
6. Caller-provided split boundaries and force-split predicates.
7. Manifest install and old-table retirement.
8. Object publication.
9. Snapshot-aware retention safety.
10. Version-retention and TTL policy implementations.
11. Product-family exemptions.

## Exit Gate

L5H is complete when:

1. `table/compaction.rs` exposes a policy-free generic compaction primitive;
2. keep-all compaction preserves every row and returns deterministic M3G
   artifacts;
3. policy-selected drops are the only drops possible;
4. exact duplicate internal keys are rejected or resolved only by explicit
   caller policy;
5. output splitting obeys the documented target and max-output-table rules;
6. every output artifact decodes through the M3G reader path;
7. reports match actual artifacts and row counts;
8. tests cover unit, model, source-guard, wasm, no-default, and clippy gates;
9. no L5H production code imports service, backend, layout, branch, lifecycle,
   commit runtime, or product payload modules;
10. the porting log records old compaction behavior as ported, deferred, or
    retired.
