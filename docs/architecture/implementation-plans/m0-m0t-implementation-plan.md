# M0 / M0T Implementation Plan: Architecture Freeze And Tracking

Status: draft implementation plan

## Goal

Make the V1 architecture document set implementation-ready before any new crate
work starts.

## Inputs M0 Reads

1. `docs/architecture/strata-v1-architecture.md`
2. `docs/architecture/strata-v1-implementation-roadmap.md`
3. `docs/architecture/v1-engineering-standards.md`
4. `docs/architecture/v1-existing-test-inventory-and-porting-plan.md`
5. All storage-next, engine-next, inference-next, and intelligence-next
   architecture documents.

## Documents M0 Maintains

These are M0 outputs or ledgers. They are not prerequisites to this plan.

1. `docs/architecture/v1-boundary-baseline.md`
2. `docs/architecture/v1-document-inventory.md`
3. `docs/architecture/v1-open-question-register.md`
4. `docs/architecture/v1-engineering-standards-baseline.md`
5. `docs/architecture/v1-test-inventory.md`
6. `docs/architecture/v1-progress-tracker.md`

All slices must follow `docs/architecture/v1-engineering-standards.md`:
permanent domain names, concept-budget discipline, file/function thresholds,
comment standards, and no roadmap labels in production code vocabulary.

## Implementation Track

| Epic | Title | Scope | Exit gate |
|---|---|---|---|
| `M0A` | Document inventory | Confirm every required architecture and contract document exists in `docs/architecture/v1-document-inventory.md`. | Missing documents are written or explicitly deferred. |
| `M0B` | Decision closure | Resolve or assign every load-bearing open question in `docs/architecture/v1-open-question-register.md`. | No crate construction depends on an unowned decision. |
| `M0C` | Standards alignment | Apply the V1 engineering standards to the roadmap and target crate-shape docs. | Planning docs clearly separate roadmap labels from code vocabulary. |
| `M0D` | Tracking setup | Establish milestone issue/PR labels and progress tracking in `docs/architecture/v1-progress-tracker.md`. | Contributors can find current milestone, epic, and test-track status. |

## Test Track

| Test epic | Title | Scope | Exit gate |
|---|---|---|---|
| `M0TA` | Document link checks | Verify links among V1 architecture documents. This is a transient verification gate recorded in `docs/architecture/v1-progress-tracker.md`, not a separate artifact. | No broken required-document links. |
| `M0TB` | Terminology scans | Scan docs for stale cleanup-era language where it would confuse V1 work. This is a transient verification gate recorded in `docs/architecture/v1-progress-tracker.md`, not a separate artifact. | Historical references are marked as historical or moved out of the V1 reading path. |
| `M0TC` | Boundary baseline | Capture current crate graph and known boundary debt in `docs/architecture/v1-boundary-baseline.md`. | Later milestones can compare against a recorded baseline. |
| `M0TD` | Standards baseline | Run the engineering-standards scans against current source and docs; record the result in `docs/architecture/v1-engineering-standards-baseline.md`. | Existing violations are classified as old-code debt or V1 blockers. |
| `M0TE` | Existing test inventory | Populate `docs/architecture/v1-test-inventory.md` and classify current tests. | Every existing test file has keep/rewrite/archive/delete action and target track where applicable. |

## Priority Order

M0 closes in this order unless a later item exposes a blocker that forces a
small correction to an earlier item.

| Priority | Code | Track | Closure condition | Why this order |
|---|---|---|---|---|
| 1 | `M0TA` | Test | Active V1 and non-archive docs have no broken markdown links or missing backticked document paths. | The reading path must resolve before any decision or inventory work can be trusted. |
| 2 | `M0TB` | Test | Active docs have no stale cleanup-era vocabulary except intentional standards examples; historical docs are archived or clearly marked historical. | New implementation work should not inherit old milestone language. |
| 3 | `M0TC` | Test | Current crate graph, dependency direction, and known boundary debt are captured in `docs/architecture/v1-boundary-baseline.md`. | Decision closure needs factual evidence about the current codebase. |
| 4 | `M0A` | Implementation | `docs/architecture/v1-document-inventory.md` lists every required architecture and contract document; each exists, is explicitly deferred, or is clearly historical evidence. | Document inventory should use the cleaned reading path and boundary baseline. |
| 5 | `M0B` | Implementation | `docs/architecture/v1-open-question-register.md` maps active open-question sections to milestone owners and closure points. | Later planning depends on knowing which decisions are still live. |
| 6 | `M0C` | Implementation | Roadmap and target crate-shape docs follow the V1 engineering standards. | Standards alignment should happen after decision ownership is stable. |
| 7 | `M0TD` | Test | Engineering-standard scans against source and docs are recorded in `docs/architecture/v1-engineering-standards-baseline.md` as old-code debt or V1 blockers. | The standards baseline should use the final M0 standards wording. |
| 8 | `M0TE` | Test | `docs/architecture/v1-test-inventory.md` lists every current test file with keep, rewrite, archive, or delete action and target milestone where applicable. | Test inventory needs the boundary and standards baseline so it does not preserve obsolete behavior by accident. |
| 9 | `M0D` | Implementation | `docs/architecture/v1-progress-tracker.md` establishes milestone, epic, slice, label, and test-track tracking for the V1 effort. | Tracking closes M0 after the work vocabulary and closure evidence are stable. |

## Convergence Notes

1. `M0TA` and `M0TB` close before `M0B` finalizes decision ownership.
2. `M0TC` is the next closure target after `M0TA` and `M0TB`.
3. `M0B` produces `docs/architecture/v1-open-question-register.md`; later
   milestones filter that register for their owner code before starting.
4. `M0TE` starts before substantial V1 code work and feeds every later
   milestone test track.
5. `M0A` explicitly verifies that `docs/architecture/next-charter.md` remains
   historical and is not part of the binding V1 reading path.
6. `M0TD` consumes the M0C standards wording and feeds M0TE by identifying
   milestone-named tests and other standards debt that the test inventory must
   classify.
7. `M0D` closes after M0TE so the tracker can record the full M0 closure record
   and name the next ready milestone without guessing.

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
milestone can start without guessing ownership. The Phase 0 exit criteria in
`docs/architecture/strata-v1-implementation-roadmap.md` remain the canonical M0
gate; this plan explains how M0 reaches them.
