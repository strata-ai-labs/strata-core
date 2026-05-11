# M0 / M0T Implementation Plan: Architecture Freeze And Tracking

Status: draft implementation plan

## Goal

Make the V1 architecture document set implementation-ready before any new crate
work starts.

## Inputs

1. `docs/architecture/strata-v1-architecture.md`
2. `docs/architecture/strata-v1-implementation-roadmap.md`
3. `docs/architecture/v1-engineering-standards.md`
4. `docs/architecture/v1-existing-test-inventory-and-porting-plan.md`
5. All storage-next, engine-next, inference-next, and intelligence-next
   architecture documents.

All slices must follow `docs/architecture/v1-engineering-standards.md`:
permanent domain names, concept-budget discipline, file/function thresholds,
comment standards, and no roadmap labels in production code vocabulary.

## Implementation Track

| Epic | Title | Scope | Exit gate |
|---|---|---|---|
| `M0A` | Document inventory | Confirm every required architecture and contract document exists. | Missing documents are written or explicitly deferred. |
| `M0B` | Decision closure | Resolve or assign every load-bearing open question. | No crate construction depends on an unowned decision. |
| `M0C` | Standards alignment | Apply the V1 engineering standards to the roadmap and target crate-shape docs. | Planning docs clearly separate roadmap labels from code vocabulary. |
| `M0D` | Tracking setup | Establish milestone issue/PR labels and progress tracking. | Contributors can find current milestone, epic, and test-track status. |

## Test Track

| Test epic | Title | Scope | Exit gate |
|---|---|---|---|
| `M0TA` | Document link checks | Verify links among V1 architecture documents. | No broken required-document links. |
| `M0TB` | Terminology scans | Scan docs for stale cleanup-era language where it would confuse V1 work. | Historical references are marked as historical or moved out of the V1 reading path. |
| `M0TC` | Boundary baseline | Capture current crate graph and known boundary debt before implementation. | Later milestones can compare against a recorded baseline. |
| `M0TD` | Standards baseline | Run the engineering-standards scans against current source and docs. | Existing violations are classified as old-code debt or V1 blockers. |
| `M0TE` | Existing test inventory | Populate `docs/architecture/v1-test-inventory.md` and classify current tests. | Every existing test file has keep/rewrite/archive/delete action and target track where applicable. |

## Convergence Notes

1. `M0TA` and `M0TB` close before `M0B` finalizes decision ownership.
2. `M0TE` starts before substantial V1 code work and feeds every later
   milestone test track.
3. `M0A` explicitly verifies that `docs/architecture/next-charter.md` remains
   historical and is not part of the binding V1 reading path.

## Slice Policy

Slice numbers are assigned only when implementation starts. A slice should touch
one document group or one tracking mechanism. Avoid broad edits that reword
architecture decisions without changing their meaning.

## Non-Goals

1. No crate scaffolding.
2. No production Rust changes except optional guard scripts.
3. No attempt to make the current old architecture match V1 boundaries.

## Milestone Exit Gate

M0 is complete when the architecture set is internally consistent, open
questions are assigned, implementation tracking exists, and the first code
milestone can start without guessing ownership. The roadmap Test Gate Summary
remains the canonical milestone gate; this plan explains how M0 reaches it.
