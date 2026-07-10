# Engine Core Control Plane Implementation Plan

Status: draft implementation plan

Test plan:
`docs/architecture/implementation-plans/engine-core-control-plane-test-plan.md`

## Problem

The rebuilt engine has enough primitive and executor surface to exercise real
product workflows, but the internal metadata boundary is still too loose. The
old architecture had two useful reserved locations:

1. A global `_system_` branch for engine-owned database metadata.
2. A hidden `_system_` space inside every branch for branch-local internal
   metadata.

That model is worth keeping, but the old implementation accumulated unrelated
state in the same area: recipes, search policy, shadow vectors, graph-shaped
branch projections, query caches, tags, notes, and direct upper-layer writes to
raw system keys. This slice should not recreate that sprawl.

The goal is to implement the narrow core control plane required to open,
validate, branch, route storage rows, and protect reserved namespaces. Search,
query, retrieval, intelligence, recipes, and derived-state systems get their own
passes.

## Related Documents

Architecture references:

1. `docs/architecture/engine/control-plane-layout-contract.md`
2. `docs/architecture/engine/persistence-adapter-contract.md`
3. `docs/architecture/engine/storage-space-id-registry.md`
4. `docs/architecture/engine/branch-operation-and-capability-adapter-contract.md`
5. `docs/architecture/runtime-resource-profile-architecture.md`

Old architecture evidence:

1. `crates/engine/src/system_space.rs`
2. `crates/engine/src/primitives/space.rs`
3. `crates/engine/src/branch_ops/branch_control_store.rs`
4. `crates/engine/src/graph/branch_dag.rs`
5. `crates/engine/src/database/mod.rs`
6. `crates/engine/src/database/branch_service.rs`
7. `crates/storage/src/layout.rs`

Current targets:

1. `crates/engine-next/src/control/`
2. `crates/engine-next/src/branch/`
3. `crates/engine-next/src/persistence/`
4. `crates/engine-next/src/api/`
5. `crates/engine-next/tests/control_plane.rs`
6. `crates/engine-next/tests/branch_semantics.rs`
7. `crates/engine-next/tests/dependency_guards.rs`

## Core Scope

In scope:

1. Database identity and local instance identity facts needed to open the
   database safely.
2. Storage-space registry facts and compiled registry validation.
3. Capability and migration registry facts required to decide whether the engine
   can open the database.
4. Branch catalog rows: name, branch id, storage branch id, generation,
   lifecycle, creation facts, deletion facts, and default branch pointer.
5. Branch lineage rows required by branch semantics: root, fork anchor, and
   branch operation edges that exist in the current product surface.
6. Branch operation pending rows used to fail closed after interrupted creates,
   forks, deletes, or metadata activation.
7. Branch-local space catalog rows for user-visible spaces.
8. Reserved-space facts proving `_system_` exists implicitly and is not
   user-managed.
9. Engine-only control-plane read/write services over the persistence adapter.
10. Visibility guards preventing `_system_` branch, `_system_` space, and
    control storage-space IDs from leaking through user APIs.
11. Minimal typed diagnostics for openability and core metadata health.

Out of scope:

1. Recipes and retrieval policy.
2. Search/query configuration.
3. Shadow vectors and autoembedding state.
4. Vector/search/graph/retrieval derived manifests and watermarks.
5. Query expansion caches, prompt caches, or other discardable caches.
6. Local AI model metadata.
7. Branch DAG graph projection.
8. Tags and notes.
9. Merge, publish, review, cherry-pick, revert, restore, and branch diff.
10. CLI, SDK, MCP, and admin UX.

## Binding Decisions

1. **Core control plane stays small.** A row family belongs in this slice only
   if the database cannot safely open, validate branches, route storage rows, or
   protect reserved namespaces without it.
2. **Reserved axes remain independent.** Global core rows live on `_system_`
   branch in `_system_` space. Branch-local core rows live on the user branch in
   `_system_` space.
3. **User data never shares control locations.** User KV, JSON, event, vector,
   and graph rows live in user-visible spaces. Core control rows do not.
4. **Control writes go through engine services.** Primitive services,
   executor, inference, intelligence, CLI, and future SDK layers must not
   construct raw system-space keys or write core control rows directly.
5. **Persistence is the only storage boundary.** Control services use
   `StoragePersistence` and row-address APIs. They do not call storage-next
   directly.
6. **Branch lineage is not graph product data.** Authoritative lineage is a core
   control row family. A graph-shaped projection may be added later as derived
   state, but not in this slice.
7. **Open-time source rows fail closed.** Missing, corrupt, or incompatible
   identity, registry, capability, migration, branch catalog, or space catalog
   source rows must fail closed with structured errors unless the create path can
   prove it is finishing first-time initialization.
8. **Registry validation is core.** The compiled registry seed and persisted
   registry must agree on core control IDs before ordinary product services
   start.
9. **Space catalog is branch-local source metadata.** It tracks user-visible
   spaces and reserved-space protection. It does not own primitive-specific
   metadata or search indexes.
10. **Existing broad contracts are narrowed by this plan.** Any older control
    plane text that places recipes, derived manifests, shadow vectors, or query
    caches in the control plane is deferred until a search/query/retrieval pass
    reintroduces those row families deliberately.

## Target Row Families

Core global rows use:

```text
branch = _system_
space  = _system_
```

| Family | Storage-space ID | Class | Owning service |
| --- | ---: | --- | --- |
| Database identity | `0x34` | Source | identity service |
| Local instance identity | `0x34` | Source | identity service |
| Storage-space registry | `0x32` | Source | registry service |
| Capability registry | `0x32` | Source | registry service |
| Migration registry | `0x32` | Source | registry service |
| Branch catalog | `0x30` | Source | branch service |
| Branch generation guards | `0x30` | Source | branch service |
| Branch lineage | `0x30` | Source | branch service |
| Pending branch operations | `0x30` | Source | branch service |
| Default branch fact | `0x30` | Source | branch service |

Core branch-local rows use:

```text
branch = user branch
space  = _system_
```

| Family | Storage-space ID | Class | Owning service |
| --- | ---: | --- | --- |
| Space catalog | `0x31` | Source | space service |
| Reserved-space facts | `0x31` | Source | space service |

Reserved but unused in this slice:

1. `0x33` remains unavailable for core use. Do not implement recipe rows here.
2. `0x40..=0x45` remain derived/cache territory. Do not implement them here.

## Durable Key Families

Use stable byte prefixes under each storage-space ID. Exact encoding should
follow the existing `persistence/key.rs` style and have round-trip tests.

Global identity and registry prefixes:

1. `identity/database`
2. `identity/local-instance`
3. `registry/storage-space`
4. `registry/capability`
5. `registry/migration`

Global branch prefixes:

1. `branch/index`
2. `branch/default`
3. `branch/catalog/<branch-name>`
4. `branch/generation/<branch-id>`
5. `branch/lineage/<branch-id>/<generation>/<sequence>`
6. `branch/pending/index`
7. `branch/pending/<operation-id>`

Branch-local space prefixes:

1. `space/index`
2. `space/catalog/<space-name>`
3. `space/reserved/<space-name>`

Do not add prefixes for recipes, retrieval, search, vectors, graph projections,
or caches in this slice.

## Target Internal API

Add a small control module boundary. Names can follow existing module style, but
the responsibilities should be explicit:

1. `control::identity`
   - creates and loads database identity rows
   - creates and loads local instance identity rows
   - validates identity payload versions
2. `control::registry`
   - seeds compiled registry rows on create
   - validates persisted registry rows on open
   - exposes typed registry diagnostics
3. `control::branch`
   - owns branch catalog, generation guard, lineage, pending operation rows, and
     default branch rows
   - exposes typed methods consumed by the branch service
4. `control::space`
   - owns branch-local space catalog and reserved-space rows
   - exposes typed methods consumed by space-aware primitive services
5. `control::health`
   - assembles minimal diagnostics from the above services
   - does not expose raw keys or storage-space byte IDs to normal user APIs

Do not expose these services through executor command contracts in this slice.
Executor should keep using product branch, space, and primitive commands.

## Implementation Order

1. **Document the narrow core registry.**
   - Add a code-local registry seed for core control IDs: `0x30`, `0x31`,
     `0x32`, and `0x34`.
   - Record that `0x33` and `0x40..=0x45` are not part of this core slice.
   - Add a source comment that any future row-family addition must identify its
     owning service and pass registry tests.

2. **Add `SpaceControl` row class.**
   - Add `RowClass::SpaceControl` mapped to `0x31`.
   - Keep this as branch-local source metadata only.
   - Do not move primitive row classes in the same patch unless a separate
     format-reset decision explicitly requires it.

3. **Harden global control address helpers.**
   - Centralize `_system_` branch addressing for global control rows.
   - Prevent callers from manually passing user branch ids for global row
     families.
   - Keep persistence adapter commits as the only storage write path.

4. **Add branch-local system-space addressing.**
   - Add an engine-only helper that creates branch-local control addresses for
     a validated product branch and a control row class.
   - Use it only from control services, not primitive services directly.

5. **Split bootstrap into owned services.**
   - Move identity, registry, branch catalog, and pending-branch bootstrap work
     behind dedicated control services.
   - Preserve current behavior while removing monolithic bootstrap-only
     knowledge.

6. **Implement space catalog source rows.**
   - Seed branch-local `space/index` and reserved `_system_` facts for new
     branches.
   - Register user spaces through the space control service.
   - Keep `_system_` implicit and hidden from product space lists.

7. **Add core control diagnostics.**
   - Report database identity status.
   - Report registry status.
   - Report branch catalog status and default branch status.
   - Report space catalog status for a requested branch.
   - Do not expose recipes, search, query, retrieval, shadow vector, or derived
     index status.

8. **Add source and dependency guards.**
   - Reject imports of raw control-key helpers outside `control` and
     `persistence`.
   - Reject direct storage-next dependencies from ordinary engine modules above
     persistence.
   - Reject public APIs that expose `_system_` branch or `_system_` space as
     user-manageable resources.

9. **Update architecture docs.**
   - Add a short note to the broad control-plane contract saying this
     implementation slice is intentionally narrower than the full future
     control-plane surface.
   - Link this plan from the implementation plan index if such an index exists.

## Open And Create Behavior

Create path:

1. Create deterministic system branch.
2. Create or validate storage branch for the selected default product branch.
3. Write identity rows.
4. Write registry rows.
5. Write branch catalog/default/pending rows.
6. Write branch-local space catalog and reserved `_system_` facts for the
   default branch.
7. Return database only after all source control rows commit.

Open path:

1. Load and validate identity rows.
2. Load and validate registry rows.
3. Load and validate capability and migration registry rows.
4. Load and validate branch catalog/default/pending rows.
5. Fail closed on unresolved pending operations unless the operation service can
   prove a deterministic recovery action.
6. Lazily load branch-local space catalog rows when the branch is selected or
   when diagnostics request them.
7. Start primitive services only after core control validation passes.

## Visibility Rules

1. `list_branches` never returns `_system_`.
2. `get_branch("_system_")` through product APIs returns reserved-name or not
   found, according to existing error conventions.
3. `create_branch("_system_")` fails as reserved.
4. `delete_branch("_system_")` fails as reserved.
5. `list_spaces` never returns `_system_`.
6. `create_space("_system_")` fails as reserved.
7. `delete_space("_system_")` fails as reserved.
8. User scans never decode control rows as primitive rows.
9. Diagnostics may report typed health facts but not raw control paths by
   default.

## Error And Recovery Policy

Required structured failures:

1. Missing database identity on existing open.
2. Corrupt identity payload.
3. Registry missing on existing open.
4. Registry ID conflict.
5. Unsupported core format version.
6. Missing default branch row.
7. Default branch points to a missing catalog row.
8. Branch catalog has duplicate live names.
9. Branch generation guard is lower than a catalog generation.
10. Pending branch operation unresolved after restart.
11. Branch-local space catalog corrupt.
12. Reserved `_system_` space missing or marked user-managed.

Recovery should be conservative:

1. Core source row corruption fails closed.
2. Missing branch-local space catalog for a branch created by an old developer
   build may be repaired only if the engine can prove user data is intact and
   no reserved-space conflict exists.
3. Derived/cache repair policies are not part of this slice.

## Acceptance Criteria

1. Core control row families are documented, encoded, and tested.
2. Global control rows are written only on `_system_` branch.
3. Branch-local space rows are written only in branch-local `_system_` scope.
4. `_system_` branch and `_system_` space remain invisible and protected through
   product APIs.
5. Database create and durable reopen validate identity, registry, branch, and
   space catalog source rows.
6. Branch create/fork/delete initializes or validates required branch-local
   space rows.
7. Primitive KV, JSON, event, vector, and graph behavior remains unchanged
   except for using the new space catalog service where needed.
8. No recipe, search, retrieval, intelligence, shadow vector, derived manifest,
   or cache row family is added.
9. Source/dependency guards prevent upper layers from writing raw control rows.
10. Existing engine and executor tests pass.

## Stop Conditions

Stop and write a follow-up plan instead of widening this slice if any of these
conditions appear:

1. A change requires recipe or retrieval policy rows.
2. A change requires shadow vector manifests or search index health.
3. A change requires CLI/admin UX design.
4. A change requires remapping all primitive storage-space IDs.
5. A change requires merge/revert/restore semantics.
6. A control service wants to expose raw system keys publicly.

## Follow-Up Slices

Expected later plans:

1. Search/query control and retrieval policy layout.
2. Derived-state manifests, watermarks, and health.
3. Shadow vector ownership and rebuild tracking.
4. CLI/admin diagnostics over typed control health.
5. Dataset clone/export treatment of core and derived control rows.
