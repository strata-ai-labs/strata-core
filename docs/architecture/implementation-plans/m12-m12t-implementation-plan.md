# M12 / M12T Implementation Plan: Advanced Branch Operations

Status: draft implementation plan

## Goal

Restore the advanced branch operations — compare (`diff`), promote (`merge`),
copy-selected (`cherry-pick`), and undo-range (`revert`) — that v0.6 shipped and
the V1 cutover deferred, built natively on the V1 persistence substrate and
governed by the engine branch-operation contract.

## Inputs

1. `docs/architecture/engine/branch-operation-and-capability-adapter-contract.md`
   (the authoritative engine contract: adapter trait, conflict model, and the 26
   conformance tests).
2. `docs/product/strata-v1-branching-direction.md` (product direction, scope, and
   conflict-strategy semantics).
3. `docs/architecture/engine/temporal-context-and-timeline-resolver-contract.md`
   (branch-point and retained-frontier resolution).
4. `docs/architecture/v1-engineering-standards.md`.
5. `docs/architecture/v1-error-and-diagnostics-contract.md`.
6. `crates/executor/idl/v1/README.md` (the IDL runbook for command wiring).
7. `crates/engine/tests/branch_merge_absence.rs` (the absence guard and its
   documented retirement procedure).
8. `crates/engine/src/branch/service.rs` (the as-shipped V1 branch surface and
   recorded branch-point lineage).
9. Historical reference only: `crates/engine/src/branch_ops.rs` at tag `v0.6.0`
   (fork/diff/merge; no cherry-pick/revert/merge-base; two-way merge; debug-string
   diff — a shape reference for compare, not a design for promotion).

All slices must follow the V1 engineering standards: permanent domain names,
concept-budget discipline, file/function thresholds, comment standards, and no
roadmap labels in production code vocabulary. Engine owns branch semantics;
branch operations use persistence, never direct storage imports.

## Scope Decisions

Locked for this milestone:

1. **Sequencing is compare-first.** Compare is read-only and unblocks the
   conflict logic every mutating operation depends on.
2. **The public surface keeps Git names** — `diff`, `merge`, `cherry-pick`,
   `revert` — in the IDL, CLI, and SDK. The product-direction document's
   activity vocabulary (compare / promote / copy-selected / undo-range) is used
   in prose and documentation, not as command names.
3. **KV and JSON are the core capabilities** for every operation first; event,
   vector, and graph coverage lands in a dedicated breadth epic (`M12G`).
4. Conflict strategies are `Strict` (default) and source-wins, per the contract.
   No additional strategies in M12.

## Implementation Track

| Epic | Title | Scope | Exit gate |
|---|---|---|---|
| `M12A` | Capability branch adapter substrate | Define the engine capability-branch-adapter trait from the contract; implement it for KV and JSON; add version-bounded per-branch/space row enumeration over the persistence read path (real values, not display strings). | KV and JSON expose adapter enumeration at a version bound; no operation logic yet. |
| `M12B` | Compare | Engine `compare` producing a `BranchComparison` grouped by capability and space (added/removed/modified), derived rows omitted by default, `as_of` supported; IDL `branch.diff`, executor, CLI `branch diff`, SDK. | Compare returns correct grouped results for KV+JSON across branches and at a retained `as_of`; ships without touching the absence guard. |
| `M12C` | Preview promotion | Branch-point resolution from recorded lineage; three-way comparison; conflict reporting without mutation; IDL `branch.merge_base` / `branch.diff_three_way`. Executes the absence-guard retirement procedure (see Landing Procedure). | Preview reports conflicts against the derived branch point without mutating source or target; the guard is retired with its replacement tests in place. |
| `M12D` | Promote (merge) | `Strict` and source-wins strategies; promotion applied as a commit on the target with recorded lineage; source branch unchanged; all-or-nothing; IDL `branch.merge`, CLI, SDK. | Strict refuses with zero target mutation on conflict; source-wins reports overwritten/deleted entries; lineage is authoritative or recoverable across a crash window. |
| `M12E` | Copy selected (cherry-pick) | Distinguish current-record copy from selected-change apply; explicit `(space, key)` selection for KV+JSON; IDL `branch.cherry_pick`, CLI, SDK. | Both copy modes are distinguishable and mutate only the target; missing records and tombstones behave per contract. |
| `M12F` | Undo range (revert) | Compensating-commit undo over an inclusive version range; preserve later work by default; IDL `branch.revert`, CLI, SDK. | Undo writes compensating changes without rewriting history and reports skipped records that changed after the range. |
| `M12G` | Capability breadth | Extend every operation to event, vector, and graph with their capability-specific conflict rules and derived-state disposition. | Event/vector/graph pass the shared branch-adapter conformance for all four operations. |

## Test Track

| Test epic | Title | Scope | Exit gate |
|---|---|---|---|
| `M12TA` | Adapter substrate conformance | Adapter enumeration correctness, version-bound selection, malformed-byte rejection, read-only rejection of mutating workflows. | Conformance #18, #24, #25 pass for KV+JSON. |
| `M12TB` | Compare conformance | Grouping by capability/space, derived-row omission, `as_of` frontier resolution, binary key display/programmatic forms, diagnostics. | Conformance #4 passes; compare is deterministic. |
| `M12TC` | Preview + strict-refusal | Branch-point from lineage, three-way conflict detection without mutation, and the strict-refusal tests that replace the absence guard. | Conformance #5–6 pass; rule 20's promised refusal surface is tested. |
| `M12TD` | Promotion conformance | Strict vs source-wins outcomes, lineage authority, and crash/reopen windows (fork-before-publish, commit-before-projection). | Conformance #7–11 pass. |
| `M12TE` | Copy-selected + undo conformance | Current-record vs selected-change modes; compensating-undo range validation and later-work preservation. | Conformance #12–13 pass. |
| `M12TF` | Capability breadth conformance | Shared adapter tests across KV/JSON/event/vector/graph; event-divergence refusal; graph relationship bindings stay branch/space-relative; derived-row disposition. | Conformance #19–23 pass. |

## Compare Slice Breakdown

`M12A` and `M12B` are the first execution targets and split into slices:

1. `M12A1`: capability-branch-adapter trait definition (engine, no capability impls).
2. `M12A2`: KV adapter — version-bounded enumeration with real values.
3. `M12A3`: JSON adapter — version-bounded enumeration, document and path shapes.
4. `M12B1`: engine `compare` over the adapters — `BranchComparison` type and grouping.
5. `M12B2`: `as_of` frontier resolution for compare via the timeline resolver.
6. `M12B3`: IDL `branch.diff` + executor + CLI `branch diff` + SDK (IDL runbook).

## Landing Procedure

The absence guard `crates/engine/tests/branch_merge_absence.rs` forbids the
vocabulary `cherry_pick`, `branch_merge`, `merge_branch`, `merge_base`,
`branch_revert`, `three_way` (and hyphen variants). It does **not** forbid
`diff`. Therefore:

1. `M12A` and `M12B` (adapter substrate and compare) land with no guard change
   and no amendment to CLAUDE.md rule 20.
2. The first slice to introduce forbidden vocabulary is `M12C` (preview
   promotion: `merge_base`, `three_way`). That slice must, in one change:
   delete the absence guard, amend CLAUDE.md rule 20, and add the strict-refusal
   tests the rule promises (owned by `M12TC`). The typed-refusal surface cannot
   ship untested.

## Convergence Notes

1. `M12TA`/`M12TB` land with `M12A`/`M12B`; compare is provable before any
   mutating operation begins.
2. `M12TC` carries the guard-replacement tests and must be green in the same
   slice that retires the guard.
3. `M12G`/`M12TF` follow the four operations; an operation is not "done" for a
   capability until its shared adapter conformance passes.

## Slice Policy

Each slice starts from a specific contract requirement or conformance test and
its matching failing test. Slices stay within the standard net-LOC budget; the
adapter trait and each capability adapter are separate slices. Public API and
CLI wording changes ride the IDL runbook, not ad hoc handler edits.

## Non-Goals

1. No CRDT merge contract, HLC-stamped rows, replica IDs, or sync tombstone GC.
2. No new conflict strategies beyond Strict and source-wins.
3. No tags, notes, or legacy branch-bundle workflows.
4. No StrataHub-facing publish/fork flow; local branch lineage only.
5. No event/vector/graph coverage before KV+JSON is proven per operation.

## Milestone Exit Gate

M12 is complete when a user can compare two branches, preview and promote a
branch with an explicit conflict strategy, copy selected records or changes, and
undo a version range — across KV, JSON, event, vector, and graph — with
engine-owned semantics over retained history, authoritative lineage that
survives the required crash windows, and the branch-operation contract's
conformance tests passing. The absence guard is retired and CLAUDE.md rule 20 is
amended to describe the shipped strict-refusal surface.
