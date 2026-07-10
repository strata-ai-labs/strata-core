# M4 / M4T Implementation Plan: Storage-Next Runtime Program

Status: draft program plan

## Goal

Finish the storage substrate that engine-next consumes through the L9 boundary.

M4 is a program milestone, not a single implementation slice. The detailed
layer plans live in separate documents so this file remains the program index.

## Inputs

1. `docs/architecture/storage/l5-table-runtime.md`
2. `docs/architecture/storage/l6-branch-isolated-lsm-runtime.md`
3. `docs/architecture/storage/l7-commit-runtime.md`
4. `docs/architecture/storage/l8-lifecycle-recovery-maintenance.md`
5. `docs/architecture/storage/l9-storage-api-boundary.md`
6. `docs/architecture/storage/commit-timeline-substrate.md`
7. `docs/architecture/storage-architecture.md`

All slices must follow the V1 engineering standards: permanent domain names,
concept-budget discipline, file/function thresholds, comment standards, and no
roadmap labels in production code vocabulary.

## Program Decomposition

| Sub-milestone | Layer | Detailed plan | Exit gate |
|---|---|---|---|
| `M4-L5` | Table runtime | `docs/architecture/implementation-plans/m4-l5-table-runtime-implementation-plan.md` | Table mechanics pass direct model/property/conformance tests without branch, commit, recovery, or engine concepts. |
| `M4-L6` | Branch LSM runtime | `docs/architecture/implementation-plans/m4-l6-branch-lsm-runtime-implementation-plan.md` | Branch-aware reads match model tests across inheritance, tombstones, TTL, and retention boundaries. |
| `M4-L7` | Commit runtime | `docs/architecture/implementation-plans/m4-l7-commit-runtime-implementation-plan.md` | Cache, standard, and always commit paths classify ambiguous outcomes and preserve ordering. |
| `M4-L8` | Lifecycle, recovery, maintenance | `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-implementation-plan.md` | Recovery converges to committed visible state or a structured failure without inventing or losing durable data. |
| `M4-L9` | Storage API boundary | TBD | Engine-next can use storage through L9 without reaching lower modules. |

Durability policy testing is not a separate peer layer. Cache, standard, and
always behavior must be proven where it matters:

1. L7 proves commit ordering and acknowledgement behavior.
2. L8 proves recovery and maintenance convergence.
3. L9 proves the public storage boundary presents the mode guarantees correctly.

## Slice Policy

Slices may be vertical only when storage semantics require table, branch, and
commit cooperation. Otherwise keep slices aligned to one domain module and one
test harness.

Each layer-specific implementation plan should include:

1. objective;
2. existing-code source map;
3. implementation slices;
4. test plan;
5. explicit layer boundaries;
6. exit gate.

## Non-Goals

1. No product capability semantics.
2. No `EntityRef`.
3. No graph, vector, JSON, event, or search behavior.
4. No engine error mapping except storage-owned diagnostics.
5. No public transaction-session resurrection.
6. No direct use of old table bytes as valid storage-next V1 artifacts.

## Program Exit Gate

The full M4 program is complete when storage-next opens, commits, recovers,
maintains, and serves branch-aware row reads exclusively through L9. The roadmap
Test Gate Summary remains the canonical program gate; this plan explains how
M4 is decomposed without turning the program into one oversized slice.
