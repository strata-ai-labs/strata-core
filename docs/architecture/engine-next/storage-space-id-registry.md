# Engine-Next Storage-Space ID Registry

Status: V1 architecture draft

## Purpose

This document defines the engine-owned storage-space ID assignments for
engine-next.

Storage-next persists branch-aware MVCC KV rows. The physical row key includes
an opaque `storage_space_id` byte:

```text
branch + space + storage_space_id + key bytes
```

Storage may order, route, compact, and recover by this byte. Storage must not
know what an engine-owned byte means. Engine owns the meaning.

The registry exists to prevent every capability from inventing its own physical
partitioning scheme. It should keep the durable row layout boring, auditable,
and stable.

## Related Documents

Read this with:

1. `docs/architecture/storage-next/storage-space-id-registry.md`
2. `docs/architecture/engine-next-architecture.md`
3. `docs/architecture/engine-next/README.md`
4. `docs/architecture/engine-next/primitive-implementation-contract.md`
5. `docs/architecture/engine-next/entity-ref-and-relationship-layer-contract.md`
6. `docs/architecture/storage-next/l9-storage-api-boundary.md`

Follow-up contracts that depend on this one:

1. Engine persistence adapter contract.
2. Branch operation and capability adapter contract.
3. Control-plane layout contract.
4. Retrieval and derived-state contract.
5. Dataset clone artifact contract.

## Requirement Language

1. Must means the V1 durable row layout is incomplete without it.
2. Should means expected unless a later architecture decision records a clear
   deferral.
3. May means allowed but not required for V1.

## Current Code Evidence

Current storage uses `TypeTag` as a primitive-shaped byte:

| Current tag | Current meaning |
|---|---|
| `0x01` | KV |
| `0x02` | Event |
| `0x03` | Branch |
| `0x04` | Space |
| `0x05` | Vector |
| `0x06` | JSON |
| `0x07` | Graph |

That layout is useful evidence, but it is not the V1 target. In storage-next,
`0x01` is reserved by storage for the commit timeline. Engine-owned row spaces
start at `0x20`.

Because Strata is pre-launch, V1 does not need to preserve the old `TypeTag`
bytes or migrate pre-V1 development databases during normal open. The old tags
should be treated as current implementation vocabulary, not durable V1 product
vocabulary.

## Allocation Boundary

The storage-next registry owns the global byte split:

| Range | Owner | Meaning |
|---|---|---|
| `0x00` | Storage-next | Invalid sentinel. |
| `0x01` | Storage-next | Commit timeline rows. |
| `0x02..=0x1f` | Storage-next | Reserved for storage-internal rows. |
| `0x20..=0xff` | Engine-next | Engine-owned product and derived rows. |

Engine-next must not write storage-owned IDs. Storage-next must reject
engine-supplied commit rows that use storage-owned IDs.

## Design Rules

### Use Few IDs

A storage-space ID is not a class hierarchy. It is a durable partitioning byte.

Engine-next should create a new ID only when rows need a materially different:

1. Source-of-truth status.
2. Rebuild behavior.
3. Retention behavior.
4. Prefix-scan boundary.
5. Branch/copy/merge behavior.
6. Access-control or system-space boundary.
7. Recovery or diagnostic treatment.

If ordinary key prefixes inside an existing ID solve the problem, use key
prefixes. Do not create a new ID just because a new Rust module, DTO, command,
or background job exists.

### Separate Source Rows From Derived Rows

User-authored source rows are the durable truth for product data. Derived rows
are indexes, shadows, projections, reverse maps, or manifests that can be
rebuilt or invalidated from source rows.

Source rows and derived rows should normally live in different storage spaces so
branch operations, clone/export, repair, and diagnostics can treat them
differently.

### Keep Storage Opaque

Storage may expose a byte and may validate byte ownership. Storage must not map
engine-owned bytes back to `kv`, `json`, `event`, `vector`, `graph`, `search`,
or any other product concept.

### Keep Product Space Separate

The product `space` in the storage key is not a storage-space ID.

Examples:

1. User space `default` plus storage-space ID `0x20` means KV rows in user
   space `default`.
2. User space `_system_` plus storage-space ID `0x33` means recipe/control
   rows in the branch-local system space.
3. The same storage-space ID can appear in many product spaces. The product
   space still participates in isolation and access rules.

### Freeze Means Freeze

Before V1 format freeze, assignments may change if this document changes and
pre-V1 databases are rejected. After V1 format freeze, an assigned byte must
never be reused for a different meaning.

Renaming the label in code or docs is allowed only if the durable meaning stays
the same.

## V1 Engine Assignments

### Source Rows

These rows are product source of truth unless the capability contract says a
specific row prefix is metadata.

| ID | Label | Rows |
|---|---|---|
| `0x20` | KV | KV user records. |
| `0x21` | JSON | JSON documents and document-local metadata. |
| `0x22` | Event | Event records and event-log metadata. |
| `0x23` | Vector | User-authored vector collections, configs, and records. |
| `0x24` | Graph | Graph metadata, nodes, edges, ontology, and relationship bindings. |

Rules:

1. These IDs are for authored or user-visible data capability state.
2. Capability-local metadata may share the same ID when it commits and branches
   with the source data.
3. Capability-local secondary rows may share the same ID only when they are
   updated transactionally with the source rows and are not managed by a
   separate rebuild lifecycle.

### Control Rows

These rows define engine behavior and database/branch metadata.

| ID | Label | Rows |
|---|---|---|
| `0x30` | Branch | Branch catalog, branch generation guards, authoritative branch lineage, branch workflow metadata. |
| `0x31` | Space | Space catalog and reserved-space records. |
| `0x32` | Registry | Capability registry, storage-space registry, format/cutover facts. |
| `0x33` | Recipe | Built-in recipes, user recipes, branch recipe overrides. |
| `0x34` | Dataset | Database identity, dataset identity, provenance, StrataHub substrate metadata. |

Rules:

1. Global control rows normally live in the `_system_` branch.
2. Branch-local control rows normally live in the branch's `_system_` space.
3. Control rows are engine-owned product rows. Storage does not interpret them.
4. Branch and space rows do not make branch or space into storage primitives.
5. Authoritative branch lineage rows live under `0x30` and are source control
   state. If a later implementation keeps a graph-shaped branch DAG projection,
   that projection is rebuildable derived state under `0x45`.

### Derived Rows

These rows are rebuildable, invalidatable, or health-tracked derived state.

| ID | Label | Rows |
|---|---|---|
| `0x40` | Search | BM25/text index rows and search lookup tables. |
| `0x41` | Shadow vector | Autoembedding vectors and source-link rows. |
| `0x42` | Vector index | ANN index metadata and rebuildable vector acceleration rows. |
| `0x43` | Graph index | Graph reverse maps, relationship lookup rows, traversal accelerators. |
| `0x44` | Projection | Retrieval projections and source projection caches. |
| `0x45` | Derived state | Watermarks, rebuild state, health records, projection manifests, optional branch-DAG projections. |

Rules:

1. Derived rows must declare whether they are safe to omit from clone/export.
2. Derived rows must have health or rebuild state when stale results could
   affect product behavior.
3. Derived rows must not become the only copy of user-authored data.
4. Derived rows may be dropped and rebuilt when the owning contract says doing
   so is safe.
5. `0x45` rows are subordinate derived-state records. They must identify the
   derived subsystem or row family they describe through key prefix or row
   payload, and they must not become standalone authoritative product state.
6. If clone/export includes a derived row family, it must either include the
   corresponding `0x45` health/watermark rows or mark that derived family for
   validation on import.
7. If clone/export omits a derived row family, import must omit, drop, or
   reinitialize the matching `0x45` rows. It must not preserve stale derived
   health or watermark records as if the omitted rows still exist.
8. `0x40` is for text-search substrate rows such as BM25 postings, term
   dictionaries, and search lookup tables.
9. `0x44` is for retrieval projection and cache rows such as snippets,
   projected source payloads, expansion cache entries, prompt/context cache
   entries, and other discardable retrieval intermediates. Prefix discipline
   inside `0x44` separates projection rows from cache rows.

### Reserved Engine Range

All other engine-owned IDs in `0x20..=0xff` are unassigned until this registry
assigns them.

Unassigned IDs must not appear in stable V1 databases. Tests and fault harnesses
should avoid durable use of unassigned IDs unless they are explicitly testing
invalid ID handling.

## How To Choose An ID

Use this decision order:

1. If the row is storage-internal, it does not belong in this registry.
2. If the row is user-authored KV data, use `0x20`.
3. If the row is user-authored JSON, event, vector, or graph data, use that
   capability's source ID.
4. If the row describes branches, spaces, registries, recipes, dataset identity,
   or provenance, use the matching control ID.
5. If the row can be rebuilt from source rows, use the derived ID that matches
   the subsystem that owns the rebuild.
6. If none of these fit, update this document before writing code.

Key prefixes, not new storage-space IDs, should distinguish small internal
families under the same source/control/derived category.

Examples:

1. A JSON document row uses `0x21`.
2. JSON document-local metadata that commits with the document may use `0x21`
   with a reserved key prefix.
3. A text projection for that JSON document uses `0x40` or `0x44`, depending on
   whether it is part of the text index or a retrieval projection cache.
4. A graph node bound to a JSON document uses `0x24`; the bound `EntityRef`
   lives in the graph row value.
5. A reverse lookup from that JSON document to bound graph nodes uses `0x43`.
6. The reverse-map rebuild watermark uses `0x45`.

## Registry Persistence

Engine-next should persist the active registry in engine control rows so a
database can validate that the compiled engine agrees with the durable layout.

`0x32` is the bootstrap control ID for the registry itself. Engine-next must know
this ID from the compiled V1 format seed before it can read the persisted
registry. The persisted registry validates the rest of the active assignment
table and confirms that `0x32` still means registry/control layout. It does not
discover the registry's own location from scratch.

Because `0x32` is a bootstrap ID, it cannot be reassigned, deprecated into a
different meaning, or hidden behind a later registry lookup after V1 format
freeze. A database whose persisted control rows conflict with the compiled
bootstrap meaning of `0x32` must fail closed with a structured format/layout
error.

Bootstrap fault hierarchy:

1. Missing registry rows on a new-database create path are initialized from the
   compiled V1 registry seed.
2. Missing registry rows on an existing database open fail closed unless the
   open path can prove it is completing first-create initialization.
3. Matching persisted registry rows continue open.
4. Corrupt registry rows fail closed with `corruption.registry`.
5. Persisted rows that decode but disagree with the compiled `0x32` bootstrap
   meaning fail closed with `unsupported.format_version`.

Minimum persisted facts:

1. Registry version.
2. Assigned byte.
3. Stable label.
4. Source/control/derived classification.
5. Owning engine contract or subsystem.
6. Format-freeze status.

Open behavior:

1. If a database has no registry because it is a new V1 database, engine creates
   one during database creation.
2. If the durable registry exists and matches the compiled registry, open
   continues.
3. If the durable registry conflicts with the compiled registry after V1 format
   freeze, open fails with a structured format/layout error.
4. If the durable registry conflicts before V1 format freeze, developer tooling
   may offer conversion or deletion. Normal product open should not silently
   reinterpret bytes.

The exact row keys for persisted registry records belong in the control-plane
layout contract.

## Snapshot, Clone, And Recovery Rules

1. Row-native snapshots preserve storage-space IDs exactly.
2. Clone/export must include source rows unless the user explicitly filters
   data.
3. Clone/export may omit derived rows only if the artifact records that they
   must be rebuilt.
4. Engine open and recovery must reject unknown engine-owned storage-space IDs
   unless the format/cutover contract says the database is pre-freeze developer
   data.
5. Recovery may quarantine or drop corrupt known derived rows only when the owning
   derived-state contract permits it.
6. Dataset artifacts must preserve the registry or include an equivalent
   manifest that lets import validate byte assignments before writing rows.

Storage-level recovery rejects invalid encodings and misuse of storage-owned IDs
in engine-supplied rows. It must not map unknown engine-owned bytes to product
names or decide whether a future engine-owned byte is semantically valid.

## Extension Rules

Adding an engine-owned storage-space ID requires:

1. Updating this document.
2. Updating the control-plane persisted registry seed.
3. Updating storage/engine conformance tests.
4. Updating clone/export validation if the row can appear in artifacts.
5. Updating recovery behavior if the row can appear in snapshots or WAL replay.
6. Explaining why an existing ID plus key prefix is not sufficient.

Removing an assigned ID after V1 freeze is not allowed. Deprecation means the
engine stops writing new rows under that ID while continuing to understand old
rows.

## Conformance Tests

Registry tests:

1. No duplicate assigned IDs.
2. No engine assignment below `0x20`.
3. No stable V1 database contains unassigned engine IDs.
4. Compiled registry matches the persisted registry on open.
5. Registry rows round-trip through clone/export/import.
6. Registry bootstrap can locate and validate persisted registry rows using the
   compiled `0x32` bootstrap ID, and a conflicting persisted `0x32` meaning fails
   open.

Boundary tests:

1. Storage rejects engine commit rows with storage-owned IDs.
2. Storage does not map engine-owned IDs to product names.
3. Engine persistence adapter is the only normal production path that constructs
   physical row keys with storage-space IDs.
4. Data capability code does not contain raw numeric storage-space IDs outside
   the central registry.

Lifecycle tests:

1. Branch copy/fork/merge handles source IDs and derived IDs differently.
2. Derived rows can be omitted and rebuilt when the owning contract allows it.
3. Unknown engine-owned IDs fail engine open/recovery with structured
   diagnostics.
4. Format-freeze conflicts fail rather than reinterpret existing bytes.
5. Clone/import does not preserve `0x45` health or watermark rows for derived
   row families that were omitted from the artifact.

## Deferred Questions And Closed V1 Baselines

1. Exact Rust names.
   The implementation should keep this registry central and boring. It does not
   need one Rust type per row label.

2. Exact persisted row keys.
   The control-plane layout contract should define where the registry rows live.

3. JSON and event secondary rows.
   Capability implementation may keep transactional secondary rows under the
   source ID, or move rebuildable indexes to derived IDs. The deciding factor is
   lifecycle, not feature name.

4. Search versus projection split.
   Closed for V1: `0x40` is for BM25/text index substrate rows. `0x44` is for
   snippets, retrieval projections, expansion entries, prompt/context caches,
   and other discardable retrieval caches.

5. Derived-state manifests.
   `0x45` is intentionally shared across derived subsystems to avoid one
   manifest ID per subsystem. The retrieval/derived-state contract can define
   key prefixes and health records.

## V1 Minimum

For V1, the minimum acceptable implementation is:

1. Storage owns `0x00..=0x1f`; engine owns `0x20..=0xff`.
2. Engine uses the V1 assignment table above for all stable source, control, and
   derived rows.
3. Storage treats engine-owned IDs as opaque bytes.
4. Engine persists and validates the active registry.
5. New row families justify a new ID instead of adding one by habit.
6. Raw numeric IDs are centralized behind the registry.
7. Unknown or conflicting IDs fail with structured diagnostics.

## Next Step

The engine persistence adapter contract is defined in
`docs/architecture/engine-next/persistence-adapter-contract.md`.

The branch operation and capability adapter contract is defined in
`docs/architecture/engine-next/branch-operation-and-capability-adapter-contract.md`.

The temporal context and timeline resolver contract is defined in
`docs/architecture/engine-next/temporal-context-and-timeline-resolver-contract.md`.

The next contract should be the control-plane layout contract.
