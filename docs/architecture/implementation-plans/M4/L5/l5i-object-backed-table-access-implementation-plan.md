# L5I Implementation Plan: Object-Backed Table Access

Status: draft implementation plan

Parent plans:

1. `docs/architecture/implementation-plans/m4-m4t-implementation-plan.md`
2. `docs/architecture/implementation-plans/m4-l5-table-runtime-implementation-plan.md`
3. `docs/architecture/implementation-plans/M4/l5e-immutable-table-builder-implementation-plan.md`
4. `docs/architecture/implementation-plans/M4/l5f-immutable-table-reader-implementation-plan.md`
5. `docs/architecture/implementation-plans/M4/l5g-block-cache-accelerators-implementation-plan.md`
6. `docs/architecture/implementation-plans/M4/l5h-generic-compaction-implementation-plan.md`
7. `docs/architecture/implementation-plans/M4/l5i-object-backed-table-access-test-plan.md`

## Goal

Add the L4/L5 handoff that lets storage-next read published immutable table
objects through the L5 table reader without letting L5 own object names,
backend handles, layout strings, filesystem paths, or durable publication.

L5I must connect the pieces already built in M3H1 and L5F:

1. L4 publishes table objects and owns `ObjectName` layout.
2. L4 records table-object facts from decoded M3G bytes.
3. L5F reads M3G bytes through `TableByteSource`.
4. L5I adapts a published table object into an exact range-readable
   `TableByteSource`.
5. `ImmutableTableReader` validates the table through the same L5F decode and
   fact path used for byte-backed readers. The L4/L5 boundary reads through the
   object-backed source before opening the reader so backend error causes stay
   typed as object-read failures instead of collapsing into generic L5
   source-read vocabulary.

L5I is not a branch table manifest, table installer, compaction scheduler, or
object lifecycle layer. It is the narrow object-access primitive L6 needs after
it has decided a table object is reachable and safe to read.

## Inputs

1. `docs/architecture/storage/l5-table-runtime.md`
2. `docs/architecture/storage/l4-log-manifest-snapshot-services.md`
3. `docs/architecture/implementation-plans/m4-l5-table-runtime-implementation-plan.md`
4. `docs/architecture/implementation-plans/m4-l5-table-runtime-test-plan.md`
5. `docs/architecture/implementation-plans/M4/m4-l5-porting-log.md`
6. `docs/spec/strata-storage-format-v1.md`
7. `crates/storage-next/src/backend/mod.rs`
8. `crates/storage-next/src/backend/memory.rs`
9. `crates/storage-next/src/backend/local_fs.rs`
10. `crates/storage-next/src/service/table.rs`
11. `crates/storage-next/src/table/reader.rs`
12. `crates/storage-next/src/table/facts.rs`
13. `crates/storage-next/src/table/cache.rs`
14. `crates/storage/src/segment.rs`
15. `crates/storage/src/block_cache.rs`

## Existing-Code Source Map

| Current file | Relevant evidence | L5I porting rule |
|---|---|---|
| `crates/storage/src/segment.rs` | `KVSegment` opens local segment files and uses positional reads for table access. | Preserve the read-by-range behavior, not paths, file handles, `pread`, path-hash identity, old table bytes, or direct filesystem ownership. |
| `crates/storage/src/block_cache.rs` | Cache keys historically used path/file identity plus block address facts. | Do not reintroduce path-derived identity. L5G cache keys must use caller-supplied table identity and M3G block address facts. |
| `crates/storage-next/src/service/table.rs` | L4 table-object publication already validates M3G bytes, creates object names, publishes durable bytes, and returns table-object facts. | Extend or wrap this L4 service for object-backed reads. Keep object names and backend access here, not in `table/reader.rs`. |
| `crates/storage-next/src/table/reader.rs` | L5F `TableByteSource` and `ImmutableTableReader::open_source` already define the object-neutral reader contract. | Reuse the trait and reader exactly. Add no backend or object imports to production `table/` modules. |
| `crates/storage-next/src/backend/mod.rs` | Backends expose `ReadRange`, `ReadObject`, `ObjectMetadata`, and typed backend errors. | L5I should require the smallest honest read capability set and map backend read failures into table/source errors at the handoff. |

## Boundary Decision

The object-backed source must be owned by the L4/L5 boundary, not by pure L5
table code.

Preferred implementation shape:

```text
crates/storage-next/src/service/table.rs
  TableObjectByteSource<'a>
  TableObjectReaderService<'a> or TableObjectService::open_reader(...)

crates/storage-next/src/table/reader.rs
  existing TableByteSource trait
  existing ImmutableTableReader byte/source-backed decode path
```

Rules:

1. `crates/storage-next/src/table/` must not import `backend`, `layout`,
   `object`, `service`, `std::fs`, `Path`, or path-like APIs.
2. `service/table.rs` may depend on L5 reader types only as a boundary adapter.
   The core table-object publication service still owns object names and
   durable publish validation.
3. L6 will later choose which `TableObjectFacts` are reachable from branch
   state. L5I must not list table prefixes or infer reachability.
4. The adapter may use `ReadRange` directly. It must not use local filesystem
   APIs or assume a local path exists.
5. The adapter must validate byte-count and decoded table facts against the L4
   facts it was given.

If a code-review pass decides that `service/table.rs` must not import L5 table
types, create a small crate-private boundary module under `src/service/` with
an explicit name such as `table_access.rs`. Do not move the adapter into
`src/table/`.

## Scope

L5I implements:

1. an object-backed `TableByteSource` implementation over a backend reference,
   object name, and stable expected byte count;
2. capability preflight for object-backed table reads;
3. exact range-read validation before opening a reader;
4. optional object metadata validation when `ObjectMetadata` is available or
   explicitly required by the read path;
5. a helper that opens `ImmutableTableReader` from `TableObjectFacts` plus a
   caller-supplied `TableIdentity`;
6. post-open fact validation against L4 `TableObjectFacts`;
7. typed error mapping for unsupported capabilities, missing objects, short or
   long reads, range overflows, stale byte-count facts, corrupt table bytes,
   and fact mismatches;
8. memory-backend and localfs-backend integration tests that publish through
   L4 then read through L5;
9. source guards proving production L5 table code stayed object/backend-free;
10. a porting-log entry describing the old file-backed segment behavior that
    this object-backed handoff replaces.

L5I does not implement:

1. branch table manifests;
2. table install or replacement;
3. table-id allocation;
4. level ownership;
5. inherited table lookup;
6. latest-visible/MVCC read policy;
7. cache population policy beyond ordinary reader construction;
8. lazy block decoding;
9. object lifecycle, deletion, garbage collection, or quarantine;
10. WAL/checkpoint coordination;
11. table object listing;
12. durable publication beyond the existing L4 table-object service;
13. object-store conditional fences;
14. any public API.

## Proposed Type Surface

Names may change if responsibilities remain intact.

### `TableObjectByteSource`

An exact range-readable source over one L4 table object.

Suggested shape:

```text
TableObjectByteSource<'a> {
    backend: &'a dyn Backend,
    object: ObjectName,
    byte_count: u64,
}
```

Construction rules:

1. require `ReadRange`;
2. reject zero byte count;
3. store an owned `ObjectName`;
4. store the expected byte count from L4 table-object facts;
5. do not parse the object name for table identity;
6. do not construct object names.

`TableByteSource` behavior:

1. `byte_count()` returns the expected fact supplied at construction.
2. `read_at(offset, len)` rejects `offset + len` overflow before backend calls.
3. `read_at` rejects reads past `byte_count`.
4. zero-length reads return `Vec::new()` without backend access.
5. nonzero reads call `backend.read_range(object, BackendRange { offset, len })`
   or the local equivalent.
6. returned byte length must equal `len`; short reads become a typed source
   error.
7. backend `NotFound`, `Interrupted`, unsupported capability, and other errors
   must surface as typed object-backed read errors with the object retained for
   diagnostics.

### `TableObjectReadError`

Do not overload `TableObjectServiceError` if it makes read failures ambiguous.

Suggested variants:

```text
TableObjectReadError::UnsupportedCapability {
    object: ObjectName,
    capability: BackendCapability,
}
TableObjectReadError::Backend {
    object: ObjectName,
    source: BackendError,
}
TableObjectReadError::Source {
    object: ObjectName,
    reason: &'static str,
}
TableObjectReadError::Decode {
    object: ObjectName,
    source: TableRuntimeError,
}
TableObjectReadError::FactMismatch {
    object: ObjectName,
    field: &'static str,
}
```

Rules:

1. preserve backend errors as sources when possible;
2. preserve L5 decode errors as sources when opening the reader fails;
3. keep display bounded and do not include table payload bytes;
4. distinguish unsupported capabilities from ordinary backend failure;
5. distinguish source short-read/range errors from table-format corruption.

### `TableObjectReaderService`

Optional helper around backend + `ImmutableTableReader`.

Suggested shape:

```text
TableObjectReaderService<'a> {
    backend: &'a dyn Backend,
}

impl TableObjectReaderService<'_> {
    fn open_reader(
        &self,
        identity: TableIdentity,
        object_facts: &TableObjectFacts,
        config: TableReaderConfig,
    ) -> Result<ImmutableTableReader, TableObjectReadError>
}
```

Rules:

1. caller supplies `TableIdentity`; L5I does not derive it from object path
   components;
2. the source byte count comes from `object_facts.byte_count()`;
3. after `ImmutableTableReader` opens the decoded table, compare reader facts to
   `TableObjectFacts`:
   - byte count;
   - row count;
   - data block count;
   - commit min;
   - commit max;
4. key range comparison is optional only because current `TableObjectFacts`
   does not expose key range; if L4 facts gain key range, validate it too;
5. object metadata check may be added before opening, but it must not replace
   full M3G decode validation.
6. preserve backend read errors as `TableObjectReadError::Backend`; do not
   force them through object-neutral `TableRuntimeError::SourceRead` when the
   boundary can retain the original cause.

### `TableObjectFacts` Follow-Up

Current facts include object, byte count, row count, data block count, and
commit range. L5I should decide whether to extend them with:

1. `table_identity` if L4 should record the identity used by L5 readers;
2. key range if L4 should let L5I validate full reader facts without decoding
   the same table a second time;
3. backend fence when conditional object-store reads land.

Do not add these fields unless tests use them immediately. Passing
`TableIdentity` explicitly is acceptable for M4.

## Implementation Steps

### L5I-A: Boundary Audit

1. Read `table/reader.rs`, `service/table.rs`, backend read APIs, and L5 source
   guards.
2. Confirm the adapter target location.
3. Record the decision in the porting log.
4. Add placeholders in the L5 implementation plan and test plan if the target
   location changes during implementation.

Exit gate:

- The implementation target is explicit.
- No production `table/` module needs backend/object imports.

### L5I-B: Object-Backed Byte Source

1. Add `TableObjectByteSource`.
2. Require `ReadRange` on construction.
3. Store object and expected byte count.
4. Implement exact `TableByteSource::byte_count`.
5. Implement exact `read_at` with:
   - zero-length fast path;
   - overflow rejection;
   - past-end rejection;
   - backend range read;
   - exact-length check;
   - typed error mapping.

Exit gate:

- The source can be used by `ImmutableTableReader::open_source`.
- It cannot read outside the L4-provided byte count.

### L5I-C: Reader Helper And Fact Validation

1. Add `TableObjectReaderService` or an equivalent helper.
2. Open an `ImmutableTableReader` from `TableObjectByteSource`.
3. Validate reader facts against `TableObjectFacts`.
4. Surface mismatches as typed read errors.
5. Keep `TableIdentity` caller-supplied and single-component.

Exit gate:

- Published table object bytes can be read into an L5 reader.
- Stale or inconsistent L4 facts do not silently succeed.

### L5I-D: Capability And Metadata Policy

1. Require `ReadRange`.
2. Decide whether `ObjectMetadata` is required for M4:
   - if required, check metadata size equals `TableObjectFacts.byte_count`;
   - if optional, document that facts are validated by exact range read plus
     M3G decode.
3. Do not require `DurablePublish` for reading an already-published table.
4. Do not require `ListPrefix` for reading one known object.
5. Do not use `ReadObject` unless implementing an explicit fallback path.

Exit gate:

- Cache-mode and localfs capability expectations are clear and tested.

### L5I-E: Integration With L4 Table Publication

1. Build an M3G artifact with L5E.
2. Publish it through `TableObjectService::publish_create`.
3. Open it through the object-backed reader helper.
4. Assert rows and facts match the original artifact.
5. Repeat against memory backend and localfs backend when the `localfs` feature
   is enabled.

Exit gate:

- Publish-then-read works without table code constructing object names.

### L5I-F: Source Guards And Documentation

1. Extend `table_runtime_source_guard` if needed so production `table/` stays
   free of backend/object/service/layout imports.
2. Add a guard or targeted test proving the adapter is not in `table/reader.rs`.
3. Update the M4-L5 porting log.
4. Record any deferred lazy-read/cache behavior.

Exit gate:

- The boundary is executable, documented, and protected.

## Error And Failure Policy

L5I must distinguish these cases:

1. backend lacks `ReadRange`;
2. object is missing;
3. backend returns fewer bytes than requested;
4. backend returns more bytes than requested, if a backend API allows it;
5. requested range overflows `u64`/`usize`;
6. requested range exceeds expected byte count;
7. expected byte count cannot fit in `usize` for V1 full-object validation;
8. bytes decode as invalid M3G table data;
9. decoded reader facts disagree with L4 table-object facts;
10. caller supplies invalid `TableIdentity`;
11. backend read is interrupted or otherwise fails.

Unsupported capability and backend failure are L4/object-access errors. Table
format corruption remains a L5 table-runtime decode error wrapped by the
object-access error.

## Cache And Lazy Read Policy

L5I should not force a lazy-reader rewrite.

The current L5F reader validates the whole V1 table object at open because the
M3G footer checksum covers the full table body. That is correct for M4. L5I can
still be object-backed by reading the full object through exact range reads.

Defer these optimizations:

1. chunked full-object validation to avoid one large allocation;
2. lazy data-block materialization after full-object validation;
3. cache-backed candidate-block reads;
4. bloom-assisted object reads;
5. partial object validation proofs from L4;
6. async object-store reads.

If L5I adds chunking in M4, it must be a pure implementation detail and must
not weaken checksum validation or reader facts.

## Layer Guardrails

The final implementation must preserve these import constraints:

1. `src/table/` may import `format`, `row`, and local `table` modules.
2. `src/table/` may not import `backend`, `layout`, `object`, `service`,
   `branch`, `commit`, `lifecycle`, engine crates, `std::fs`, `Path`, or
   `PathBuf`.
3. `src/service/table.rs` may import backend/object/layout because it already
   owns table-object publication.
4. Any service-to-table import must be limited to the adapter/helper surface:
   `TableByteSource`, `ImmutableTableReader`, `TableIdentity`,
   `TableReaderConfig`, and table-runtime errors/facts.
5. No public API is introduced.

## Exit Gate

L5I is complete when:

1. a published table object can be opened through an object-backed
   `TableByteSource`;
2. memory backend and localfs backend publish-then-read tests pass;
3. missing object, short read, corrupt bytes, stale byte count, fact mismatch,
   unsupported capability, and invalid identity are tested;
4. `ImmutableTableReader` rows and facts match byte-backed reads for the same
   table;
5. L5 production code still has no backend/object/layout/service/path imports;
6. no object names are parsed inside L5;
7. L4 still owns durable publication and object layout;
8. L6 can later pass reachable table-object facts to the reader helper without
   adding new L5 table semantics;
9. generated table-runtime checks still pass under default, no-default +
   testkit, and wasm testkit builds;
10. `cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings`
    and `git diff --check` pass.
