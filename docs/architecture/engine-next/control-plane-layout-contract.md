# Engine-Next Control-Plane Layout Contract

Status: V1 architecture draft

## Purpose

This document defines the target control-plane layout for engine-next.

The control plane is the engine-owned metadata layer that makes Strata behave
like one coherent product instead of a collection of ad hoc data structures. It
owns database identity, branch catalog facts, capability registry facts,
recipes, projection manifests, watermarks, derived-state health, provenance,
remote refs, and branch-local internal state.

The current code already has two useful mechanisms:

1. A reserved `_system_` branch.
2. A hidden `_system_` space inside every branch.

The target architecture keeps those mechanisms, but does not treat the current
implementation as complete or correct. Current `_system_` usage grew over time:
some rows live on the system branch, some rows live in branch-local system
space, some branch-control rows live in the nil/default namespace, some branch
DAG rows live in a graph-only `_graph_` space, and some upper-layer code writes
system-space keys directly. Engine-next must normalize this.

The target rule is:

```text
global engine metadata       -> _system_ branch / _system_ space
branch-local engine metadata -> user branch   / _system_ space
user data                    -> user branch   / user space
storage mechanics            -> storage-owned rows and manifests, not engine control rows
```

## Related Documents

Read this with:

1. `docs/architecture/engine-next-architecture.md`
2. `docs/architecture/engine-next/README.md`
3. `docs/architecture/engine-next/primitive-implementation-contract.md`
4. `docs/architecture/engine-next/entity-ref-and-relationship-layer-contract.md`
5. `docs/architecture/engine-next/storage-space-id-registry.md`
6. `docs/architecture/engine-next/persistence-adapter-contract.md`
7. `docs/architecture/engine-next/branch-operation-and-capability-adapter-contract.md`
8. `docs/architecture/engine-next/temporal-context-and-timeline-resolver-contract.md`
9. `docs/architecture/storage-next/l9-storage-api-boundary.md`
10. `docs/architecture/stratahub-substrate-architecture.md`
11. `docs/architecture/runtime-resource-profile-architecture.md`
12. `docs/architecture/v1-error-and-diagnostics-contract.md`

Follow-up contracts that depend on this one:

1. Retrieval and derived-state contract.
2. IPC and serializable command-boundary contract.
3. Dataset clone artifact contract.
4. Public API and CLI cleanup checklist.
5. Product-pathway conformance plan.

## Requirement Language

1. Must means the control-plane layout is incomplete without it.
2. Should means expected unless a later architecture decision records a clear
   deferral.
3. May means allowed but not required for V1.

## Current Code Evidence

Current code proves the need for a formal control-plane contract.

Useful current mechanisms:

1. `crates/engine/src/branch_domain/branch_dag.rs` defines `_system_` as the
   reserved system branch name.
2. `crates/engine/src/database/mod.rs` exposes
   `Database::ensure_system_branch_exists()` as a hidden lifecycle helper.
3. `crates/engine/src/system_space.rs` defines `_system_` as a hidden
   branch-local space and provides `system_kv_key()`.
4. `SpaceIndex::exists()` treats `_system_` as implicit, `SpaceIndex::list()`
   hides it, and normal space registration skips it.
5. Storage space-name validation rejects user-created `_system_` spaces.
6. Vector system collections use branch-local `_system_` space for
   `_system_embed_*` shadow collections.
7. Recipe storage already distinguishes built-in recipes on `_system_` branch
   from branch-local recipe overrides in the branch's `_system_` space.
8. Search recovery skips the `_system_` branch so internal control rows are not
   indexed as user content.

Inconsistent current patterns:

1. Branch-control truth lives in the nil/default global namespace rather than
   the system branch.
2. The branch DAG projection lives on `_system_` branch in graph's reserved
   `_graph_` space, even though branch lineage is not graph product data.
3. Branch tags and notes live on `_system_` branch in `default` space.
4. Built-in recipes live on `_system_` branch in `_system_` space.
5. Query expansion cache lives in branch-local `_system_` space from the
   intelligence crate by calling `system_kv_key()` directly.
6. Autoembedding shadow vectors live in branch-local `_system_` space, but the
   ownership and rebuild metadata are spread across vector, search, and
   intelligence code.
7. The current helper `is_system_branch()` classifies any name starting with
   `_system` as a system branch. Public branch validation rejects all
   underscore-prefixed names, but internal code should still avoid broad
   prefix-based authority when exact identity is intended.

The target contract keeps the useful axes and removes the layout drift.

## Definitions

### Control Plane

The control plane is engine-owned metadata required to open, validate, route,
branch, retrieve, clone, diagnose, or repair a Strata database.

It includes:

1. Engine-visible storage identity references and format facts.
2. Dataset and provenance facts.
3. Branch catalog, branch lifecycle, and branch lineage facts.
4. Product space catalog facts.
5. Capability registry and capability status facts.
6. Storage-space registry facts owned by engine.
7. Recipe and retrieval policy records.
8. Projection manifests, watermarks, and health records.
9. Remote refs and hub-neutral substrate metadata.
10. Background job state that must survive process restart.
11. Derived-state status that affects product behavior.

It does not include:

1. User-authored KV, JSON, event, vector, or graph records.
2. WAL segments, tables, object manifests, or storage checkpoints.
3. Storage commit timeline rows.
4. In-memory caches that are safe to lose.
5. Secrets, auth tokens, provider refresh tokens, or private credentials.
6. Model provider calls or inference execution.

### System Branch

The system branch is the reserved branch named `_system_`.

It is the global engine metadata branch. It must not be listed, selected,
deleted, forked, merged, searched, exported as a user branch, or mutated through
ordinary user data APIs.

The system branch is still backed by normal storage rows. That is deliberate:
global control metadata needs durability, versioning, crash recovery, clone
behavior, and diagnostics. Storage must not know what those rows mean.

The system branch has deterministic identity. Engine-next must be able to
derive it before reading branch catalog rows, because the branch catalog itself
lives on the system branch. This is a bootstrap exception to the normal rule
that branch identity is discovered through the branch catalog.

### System Space

The system space is the reserved space named `_system_`.

Every branch has an implicit hidden system space. It is where engine-next stores
branch-local control rows and branch-local derived-state metadata.

Users must not create, list, select, delete, or write the system space directly.
Engine diagnostics may expose selected system-space facts through explicit
admin surfaces.

### User Space

A user space is a product-visible namespace inside a branch, such as `default`
or `tenant_a`.

User-authored data lives in user spaces. Control-plane rows must not be mixed
into user spaces.

### Row Family

A row family is a small, documented set of control-plane rows that share:

1. Scope.
2. Storage-space ID.
3. Key prefix.
4. Source, derived, or cache classification.
5. Branch copy behavior.
6. Merge behavior.
7. Clone/export behavior.
8. Retention behavior.
9. Temporal behavior.
10. Error classification.

This is a documentation concept. It does not require a Rust type named
`RowFamily`, but every durable control-plane grouping must declare these facts
somewhere before implementation.

### Source, Derived, And Cache Rows

Control-plane rows fall into three durability classes:

1. Source rows are authoritative product facts.
2. Derived rows are rebuildable facts whose absence or staleness affects
   performance or feature availability.
3. Cache rows are discardable facts whose absence must not change correctness.

Derived and cache rows must never become the only copy of user-authored data.

## Binding Decisions

1. **The control plane is first-class engine architecture.**
   It is not a bag of helper keys. Engine-next must have one documented layout
   and one normal write/read path for control-plane rows.

2. **Global control rows live on `_system_` branch in `_system_` space.**
   Engine-next should not use `_system_` branch `default` space for control
   rows. Current tags/notes/audit rows in that location are current
   implementation residue, not target architecture.

3. **Branch-local control rows live on that branch in `_system_` space.**
   Recipe overrides, derived-state manifests, search/vector/graph projection
   status, branch-local capability facts, and branch-local caches must use this
   axis.

4. **The system branch and system space are independent axes.**
   `_system_` branch plus `_system_` space means global control rows.
   User branch plus `_system_` space means branch-local control rows. These
   must not be conflated.

5. **Control rows use engine-owned storage-space IDs.**
   The storage-space ID registry assigns `0x30..=0x34` for control rows and
   `0x40..=0x45` for derived rows. Control-plane layout must use that registry
   instead of old `TypeTag` or primitive-shaped routing bytes.

6. **No direct storage access outside the persistence adapter.**
   Control-plane code uses the engine persistence adapter. Capabilities,
   retrieval, orchestration, intelligence, executor, and CLI must not construct
   raw system-space storage keys.

7. **No upper-layer direct writes to system space.**
   Intelligence and executor may request engine services that update
   branch-local control rows. They must not call a raw `system_kv_key()`-style
   helper in target architecture.

8. **Branch lineage is control-plane state, not graph product data.**
   Branch catalog, branch lifecycle, branch lineage, merge-base facts, and
   operation audit facts belong under control-plane row families. A graph-like
   projection may exist as derived state, but it is not authoritative graph
   capability data and should not live in a user-visible graph layout.

9. **Recipes are control-plane records.**
   Built-in recipes are global control rows. User recipe overrides are
   branch-local control rows. Recipe resolution must be observable and
   deterministic.

10. **Derived state is registered and health-tracked.**
    Search indexes, shadow vectors, vector indexes, graph reverse maps,
    retrieval projections, and any future derived state need manifests,
    watermarks, and health status if stale state can affect behavior.

11. **Caches are explicitly discardable.**
    Query expansion caches, prompt caches, or other performance-only records may
    live in branch-local system space, but they must be labelled discardable,
    omitted from normal clone artifacts, and safe to delete during repair.

12. **System rows are not user-searchable content.**
    Retrieval and indexing must skip the system branch and branch-local system
    spaces unless a diagnostic command explicitly requests control-plane data.

13. **System metadata follows temporal rules.**
    Branch-local source control rows are versioned with the branch. Temporal
    reads and branch-from-history must resolve them at the selected branch
    frontier when they affect product behavior.

14. **Global defaults must be resolved and recorded.**
    If a branch-local command falls back to global control metadata, such as a
    built-in recipe, the resolved global record version or registry version
    must be recorded in command provenance for operations whose output depends
    on that metadata. Retrieval and search recipes always meet this bar.

15. **Secrets do not live in the control plane.**
    Hub credentials, API keys, refresh tokens, private signing keys, and
    provider credentials must live in explicit credential stores or runtime
    configuration, not in `_system_` rows.

16. **Public APIs do not expose raw control-plane paths.**
    Users may inspect health, recipes, capabilities, provenance, and dataset
    metadata through typed product APIs. They should not need to know branch
    `_system_` or space `_system_` exists.

17. **Storage database identity and engine product identity are separate.**
    Storage-next may own the durable database UUID or storage identity required
    for manifests, recovery, and backend validation. Engine-next owns local
    instance identity, dataset identity, provenance, remote refs, and
    hub-compatible product metadata. Engine control rows may reference the
    storage database identity, but they are not its source of truth.

## Target Layout

### Global Scope

Global rows use:

```text
branch = _system_
space  = _system_
```

Target global row families:

| Family | Storage-space ID | Class | Purpose |
|---|---:|---|---|
| Storage identity reference | `0x34` | Source | Engine-visible reference to storage-owned database identity and required format facts. |
| Local instance identity | `0x34` | Source | Local engine/product instance identity minted at create or clone. |
| Dataset provenance | `0x34` | Source | Dataset identity, clone source, local instance identity, bundle provenance. |
| Remote refs | `0x34` | Source | Hub-neutral remote associations and last-known remote facts. |
| Capability registry | `0x32` | Source | Enabled capabilities, capability schema versions, required migrations. |
| Storage-space registry | `0x32` | Source | Engine registry version and durable ID assignment facts. |
| Branch catalog | `0x30` | Source | Branch names, branch refs, lifecycle status, generations, default branch fact. |
| Branch generation guards | `0x30` | Source | Per-branch-name generation counters and guards used to reject stale branch operations. |
| Branch lineage | `0x30` | Source | Fork anchors, merge/revert/cherry-pick/copy/restore edges. |
| Branch DAG projection | `0x45` | Derived | Optional graph-shaped acceleration over authoritative branch lineage. Rebuildable only. |
| Built-in recipes | `0x33` | Source | Built-in retrieval recipes and recipe schema versions. |
| Global capability policies | `0x32` | Source | Capability and registry policies that affect open or command behavior. |
| Global dataset policies | `0x34` | Source | Dataset, clone, remote, and hub-neutral product policies. |
| Global derived-state catalog | `0x45` | Derived | Database-wide derived-state health summaries. |
| Durable background jobs | `0x45` | Derived | Restartable repair/rebuild jobs that must survive process restart. |

Rules:

1. Global rows must not be stored in `_system_` branch `default` space.
2. Global rows must not depend on a user branch being present.
3. Global rows must be safe to scan without scanning user data.
4. Global source rows required for open must fail closed when corrupt.
5. Global derived rows may degrade or trigger repair according to their
   declared row-family policy.

### Branch-Local Scope

Branch-local rows use:

```text
branch = the user branch
space  = _system_
```

Target branch-local row families:

| Family | Storage-space ID | Class | Purpose |
|---|---:|---|---|
| Space catalog | `0x31` | Source | User-visible space names, lifecycle facts, and branch-local space metadata. |
| Reserved-space facts | `0x31` | Source | Hidden/reserved branch-local space declarations such as `_system_`. |
| Recipe overrides | `0x33` | Source | Branch-local named recipes overriding or extending global recipes. |
| Branch capability facts | `0x32` | Source | Branch-local enabled/disabled capability facts or schema status. |
| Projection manifests | `0x45` | Derived | Per-branch search/vector/graph/retrieval projection status. |
| Watermarks | `0x45` | Derived | Per-source coverage for derived state. |
| Derived health | `0x45` | Derived | Fresh/stale/degraded/rebuild-required facts. |
| Shadow vectors | `0x41` | Derived | Autoembedding records and source-link rows. |
| Search rows | `0x40` | Derived | Branch-local BM25/text index rows when persisted as rows. |
| Vector index rows | `0x42` | Derived | Rebuildable ANN index metadata or row-native accelerators. |
| Graph index rows | `0x43` | Derived | Reverse maps and traversal accelerators. |
| Retrieval projections | `0x44` | Derived | Rebuildable retrieval projection caches. |
| Expansion/prompt cache entries | `0x44` | Cache | Discardable query expansion, prompt, and retrieval cache payloads. |
| Expansion/prompt cache manifests | `0x45` | Cache | Discardable cache indexes, watermarks, and eviction metadata. |

Rules:

1. Branch-local source control rows participate in branch versioning.
2. Branch-local derived rows must be rebuildable or validated before use.
3. Branch-local cache rows may be deleted without repair.
4. Branch-local rows are hidden from normal `space list`, user scans, and
   retrieval.
5. Branch-local rows must declare how fork, merge, restore, clone, and delete
   treat them.

### User Scope

User rows use:

```text
branch = user branch
space  = user-visible product space
```

Target user row families are capability source rows:

| Capability | Storage-space ID | Class |
|---|---:|---|
| KV | `0x20` | Source |
| JSON | `0x21` | Source |
| Event | `0x22` | Source |
| Vector | `0x23` | Source |
| Graph | `0x24` | Source |

Rules:

1. User rows must not store global control-plane metadata.
2. Capability-local source metadata may share the capability source ID when it
   commits and branches with the source data.
3. Capability-local indexes that are rebuildable should use derived row IDs,
   not source IDs.
4. User data APIs must not expose control-plane storage-space IDs.

## Row-Family Declaration

Every durable control-plane row family must declare:

| Field | Requirement |
|---|---|
| Scope | `global`, `branch-local`, or explicitly both. |
| Physical location | Branch axis and space axis. |
| Storage-space ID | Engine-owned ID from the registry. |
| Key prefix | Stable prefix within that storage-space ID. |
| Class | Source, derived, or cache. |
| Authority | Which engine service owns writes. |
| Readers | Which engine services may read it. |
| Fork behavior | Copy, recompute, omit, or pin to source. |
| Merge behavior | Merge, source-wins, target-wins, refuse, recompute, or ignore. |
| Restore behavior | Revert with branch, recompute, or leave current. |
| Clone/export behavior | Include, omit, include-with-validation, or provider-specific. |
| Retention behavior | Retain as source, prune as derived, or discard anytime. |
| Temporal behavior | Historical, current-only, or provenance-pinned. |
| Corruption behavior | Fail open, fail closed, degrade, quarantine, or rebuild. |
| Diagnostic code | Stable error/health code family. |

Do not add a new row family for every Rust module. A row family is justified
only when one of the declared behaviors differs materially.

## Key Prefix Discipline

Storage-space IDs are coarse. Key prefixes are fine-grained.

Engine-next should prefer a small number of storage-space IDs and a small,
documented set of prefixes under each ID.

Suggested prefix families:

| Prefix | Scope | Purpose |
|---|---|---|
| `db/` | Global | Storage identity reference and local instance identity. |
| `dataset/` | Global | Dataset identity and provenance. |
| `remote/` | Global | Remote refs. |
| `capability/` | Global or branch-local | Capability registry/status facts. |
| `space-id/` | Global | Engine storage-space registry version/facts. |
| `branch/` | Global | Branch catalog and lifecycle rows. |
| `lineage/` | Global | Fork/merge/revert/cherry-pick/copy/restore edges. |
| `recipe/` | Global or branch-local | Built-in recipes and branch overrides. |
| `projection/` | Branch-local | Projection manifests. |
| `watermark/` | Branch-local | Derived-state coverage. |
| `health/` | Global or branch-local | Derived-state and capability health. |
| `cache/` | Branch-local | Discardable caches. |
| `job/` | Global or branch-local | Restartable repair/rebuild jobs. |

These are target-prefix examples, not final byte encodings. The final durable
encoding belongs in the engine format spec before implementation.

## Branch Behavior

### Create

Creating a user branch must write the global branch catalog row for the new
branch and initialize only the branch-local system rows required by source
semantics. Derived rows should normally start absent or rebuild-required.

Empty branch creation is a V1 workflow. A branch may exist with global catalog
and required branch-local control rows before it has user data.

### Fork

Fork must treat branch-local system rows according to their row-family policy.
Fork must also write global branch catalog and lineage rows that record the
source branch, selected branch frontier, and new branch lifecycle identity.

Default policy:

1. Source control rows copy at the selected fork frontier.
2. Derived rows are omitted or marked rebuild-required.
3. Cache rows are omitted.

Examples:

1. A branch-local recipe override copies to the child branch.
2. A search projection manifest is either omitted or copied only if its
   watermark is valid at the fork frontier.
3. Autoembedding shadow vectors are omitted unless the derived-state contract
   proves they are safe and complete for the fork frontier.

### Branch From Version Or Time

Branch-from-history follows fork semantics at the resolved historical frontier.

Branch-local source control rows must reflect that historical frontier. Derived
rows must not claim freshness beyond the selected frontier.

Global lineage rows must record the resolved source version or timestamp
frontier so later diagnostics can explain the branch point without recomputing
it from row contents.

### Merge Or Promote

Merging user data must not blindly merge all branch-local system rows.
Promote must write global branch lineage rows describing the source, target,
selected strategy, selected branch point, and resulting target commit.

Default policy:

1. Source control rows require explicit merge semantics.
2. Derived rows are recomputed or invalidated.
3. Cache rows are ignored.

Recipe overrides are a source control row and need a declared merge policy.
Search/vector/graph projection rows are derived and should normally be
invalidated or recomputed after merge.

### Copy

Copying records between branches must not copy branch-local system rows unless
the command explicitly targets a control-plane feature. Copying a KV key should
not copy search rows, shadow vector rows, watermarks, recipes, or relationship
indexes.

If selected-copy is treated as a branch workflow rather than a plain data write,
global lineage rows should record enough provenance to explain source branch,
target branch, selected frontier, and copied entity set summary.

### Restore And Revert

Restore/revert of user data must apply row-family policy to related
branch-local system rows.
Restore/revert must write global branch lineage rows or audit facts sufficient
to explain the selected version range, source frontier, and resulting commit.

Default policy:

1. Source control rows revert only when the user requested that control-plane
   feature or the product operation requires it.
2. Derived rows become stale or rebuild-required.
3. Cache rows may be dropped.

### Delete

Deleting a branch must delete branch-local system rows for that branch and run
declared cleanup for derived state. Global branch catalog and lineage records
must preserve enough tombstone/history facts to prevent same-name lifecycle
confusion.

## Recipe Resolution

Recipe lookup must be deterministic and explainable.

Target lookup order:

1. Branch-local recipe override in the requested branch's `_system_` space.
2. Global built-in or shared recipe in `_system_` branch `_system_` space.
3. In-memory emergency default only when the global recipe registry is missing
   in a way classified as recoverable.

Rules:

1. Built-in recipes are global source control rows.
2. Branch-local overrides are branch-local source control rows.
3. A command using a named recipe should record the resolved recipe identity,
   recipe source, registry version, and recipe content hash in provenance.
4. Built-in seeding should be transactional by registry version. A partial set
   of built-ins should be treated as corrupt, incomplete, or repair-required;
   it should not silently appear as a valid registry.
5. Upper layers should not write recipe rows directly.
6. Built-in recipe registry versions should be immutable once published inside a
   V1 database. The registry version is monotonic. Each recipe row carries a
   content hash. Changing a built-in recipe creates a new registry version and a
   new recipe hash; it must not silently change historical provenance.

## Bootstrap Rules

Engine-next must be able to open enough of the control plane to decide whether
the database is valid.

Bootstrap order:

1. Open storage and validate storage-owned format, manifest, and database
   identity facts.
2. Derive the deterministic `_system_` branch identity without consulting the
   branch catalog.
3. Read or create the `_system_` branch `_system_` space according to open mode.
4. Load the minimal global control rows required for open: engine format facts,
   storage identity reference, storage-space registry version, capability
   registry version, branch catalog root, and built-in recipe registry version.
5. Validate the global control rows before exposing a database handle.
6. Initialize missing rows only on create-new or explicit repair paths. Normal
   open of an existing durable database must not silently invent required global
   source rows after corruption or partial initialization.

System-branch catalog rule:

1. The `_system_` branch may have a branch catalog row for diagnostics and
   consistency.
2. That row is not required to derive the system branch identity.
3. If present, it must match the deterministic system branch identity.
4. If absent on a newly created database, create it during initial bootstrap.
5. If absent on an existing durable database after initialization completed,
   report a typed corruption or repair-required diagnostic.

Cache mode may initialize the same logical rows in memory. It should still
follow the same validation order so cache behavior matches durable behavior
except for durability guarantees.

## Derived-State Layout

Derived state must be visible to diagnostics before it is used for answers.

Every derived subsystem should expose:

1. Source row family covered.
2. Branch and product space coverage.
3. Commit frontier or timeline watermark.
4. Schema/version of derived rows.
5. Fresh, stale, missing, rebuilding, degraded, or corrupt status.
6. Rebuild eligibility.
7. Whether reads may use stale state.
8. Error codes for unavailable or degraded results.

Derived-state manifests live in branch-local `_system_` space, normally under
`0x45`.

Derived rows themselves use the subsystem ID:

1. `0x40` for search rows.
2. `0x41` for shadow vector rows.
3. `0x42` for vector index rows.
4. `0x43` for graph index rows.
5. `0x44` for retrieval projection rows.
6. `0x45` for derived-state manifests, watermarks, health, rebuild state, and
   optional branch-DAG projections.

If a derived subsystem stores data outside row-native storage, such as a local
filesystem sidecar for an index accelerator, the branch-local system-space
manifest is still the durable engine fact that says whether that sidecar is
present, fresh, stale, or disposable.

## StrataHub And Clone Behavior

Control-plane layout must preserve future hub workflows without making storage
hub-aware.

Global source rows that clone/export may include:

1. Dataset identity.
2. Source bundle identity.
3. Provenance.
4. License/trust metadata.
5. Capability registry version.
6. Engine storage-space registry version.
7. Remote refs only when explicitly requested.

Global rows that clone/export should not include:

1. Local machine identity by default.
2. User account identity by default.
3. Credentials or tokens.
4. Private fleet registration facts by default.

Branch-local rows that clone/export may include:

1. Recipe overrides.
2. Branch-local capability facts.
3. Provenance needed to explain branch contents.

Branch-local rows that clone/export should normally omit or invalidate:

1. Search indexes.
2. Shadow vectors unless included with validated coverage.
3. Vector index accelerators.
4. Graph reverse maps.
5. Query expansion caches.
6. Prompt/result caches.

Import must not trust derived-state health rows unless the corresponding
derived rows are present and validated. If derived rows are omitted, matching
health, watermark, and manifest rows must be omitted, reset, or marked
rebuild-required.

## Runtime Resource Profiles

Runtime resource profile facts are mostly runtime diagnostics, not durable user
configuration.

Control-plane rows may store:

1. Explicit user policy choices that should survive reopen.
2. Database-level limits intentionally configured by an owner.
3. Last-open diagnostic summaries, if classified as discardable diagnostic
   rows.

Control-plane rows must not store:

1. Auto-detected RAM or CPU facts as if they were user configuration.
2. Host-specific profile choices inside clone artifacts by default.
3. Machine-local resource facts that make a cloned database behave incorrectly
   on a different device.

The selected runtime profile should be observable through diagnostics. It does
not need to be a source control row unless the user explicitly configured it.

## Access And Visibility

V1 visibility rules:

1. Normal data APIs never expose `_system_` branch.
2. Normal data APIs never expose `_system_` space.
3. `space list` excludes `_system_`.
4. `branch list` excludes `_system_`.
5. Search and retrieval exclude system branch and system space by default.
6. Clone/export excludes local-only control rows by default.
7. Diagnostics may expose selected control-plane facts through typed output.
8. Debug/fault/test harnesses may inspect raw rows only through explicit
   testkit or diagnostic surfaces.

## Error Handling

Control-plane failures should map through the V1 error and diagnostics
contract.

Default classifications:

| Failure | Class |
|---|---|
| Missing required global source row at open | `corruption` or `failed_precondition` depending on cause. |
| Corrupt branch catalog row | `corruption.branch_catalog` or equivalent registry code. |
| Corrupt recipe row used by command | `corruption.control_plane` or typed recipe error. |
| Missing derived manifest | `unavailable.derived_state` or rebuild-required health, not corruption. |
| Stale derived watermark | `failed_precondition.stale_index` or feature-specific degraded result. |
| Discarded cache row | No user error. |
| User attempts to name system branch/space through normal product APIs | `invalid_argument.reserved_name` or closest V1 reserved-name code. |
| Unauthorized diagnostic access to control-plane facts | `permission_denied`. |
| Upper layer attempts unsupported control write | `failed_precondition` or internal contract violation. |

Write paths must not collapse control-plane errors into generic storage errors.
The persistence adapter should preserve:

1. Scope.
2. Row family.
3. Branch.
4. Space.
5. Storage-space ID label.
6. Whether the row is source, derived, or cache.
7. Suggested repair or rebuild action when available.

System access diagnostics:

1. Normal product APIs that accept user branch or space names should reject
   `_system_` as `invalid_argument.reserved_name` or the closest V1 registry
   code.
2. Product list/read surfaces should omit `_system_` rather than returning
   authorization errors.
3. Admin/diagnostic APIs that expose selected control-plane facts may require
   explicit diagnostic capability. Missing authorization there should use
   `permission_denied`.
4. Direct control-plane mutation attempts outside the owning engine service are
   internal contract violations, not user permission failures.

## Migration From Current Implementation

Engine-next should not port current `_system_` usage mechanically.

Target changes:

1. Move authoritative branch-control rows from nil/default global namespace to
   global control rows under `_system_` branch `_system_` space.
2. Retire `_system_` branch `default` space as a target control-plane location.
3. Treat branch tags and notes as non-core V1 residue unless the public API
   cleanup checklist explicitly keeps or redesigns them.
4. Replace branch DAG graph authority/projection with branch-control row
   families and, if needed, a derived branch-lineage projection under control
   rows.
5. Avoid using graph's `_graph_` space for branch workflow metadata.
6. Replace raw `system_kv_key()` usage outside engine internals with typed
   engine control-plane or derived-state services.
7. Make built-in recipe seeding registry-versioned and atomic.
8. Make derived-state manifests the durable source of truth for whether search,
   shadow vectors, graph indexes, and retrieval projections are usable.
9. Classify query expansion cache and similar rows as discardable cache rows.
10. Replace broad internal system-branch authority checks with exact system
    branch identity where exact identity is intended. Public validation may
    continue rejecting all underscore-prefixed branch names.

Because Strata is pre-V1, engine-next does not need to preserve pre-V1
development database layouts during normal open. A one-shot developer migration
tool may be useful, but the V1 runtime should prefer a clean layout over
compatibility with accidental internal formats.

## Conformance Requirements

Implementation must include tests that prove:

1. User APIs cannot create, select, list, delete, or write `_system_` branch.
2. User APIs cannot create, select, list, delete, or write `_system_` space.
3. Global control rows are absent from user branch scans and search indexes.
4. Branch-local system rows are absent from user space scans and search indexes.
5. Built-in recipe resolution is deterministic and reports its source.
6. Branch-local recipe overrides fork according to source-row policy.
7. Derived-state rows are invalidated or marked rebuild-required after merge
   and restore when required.
8. Cache rows can be deleted without changing query correctness.
9. Clone/export omits local-only rows and handles derived-state rows according
   to declared policy.
10. Corrupt required global source rows fail closed with typed diagnostics.
11. Corrupt derived rows degrade or rebuild according to declared policy.
12. Upper crates cannot import raw control-plane key helpers in production code.
13. The persistence adapter is the only normal production path to
    storage-next for control-plane rows.

## Open Questions And Closed V1 Baselines Before Implementation

1. What exact byte encoding will engine-next use for control-plane row keys?
2. Which branch-lineage projection, if any, is worth keeping after branch
   control rows become authoritative?
   The storage-space decision is closed: authoritative lineage is `0x30`;
   any graph-shaped projection is rebuildable derived state under `0x45`.
3. Should branch-local recipe overrides merge by strict conflict refusal or
   target-wins by default?
4. Which derived row families, if any, should clone/export include in V1?
5. What typed diagnostic surface should expose raw control-plane health without
   making `_system_` a user-facing storage concept?
6. Which current tags/notes APIs are deleted before engine-next and which are
   left as compatibility residue until the public API cleanup checklist?
