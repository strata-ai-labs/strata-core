# V1 Progress Tracker

Status: M0D complete after review fixes; tracker active

## Purpose

This document is the single progress ledger for the V1 rewrite. It records the
current milestone status, the issue/PR label vocabulary, and the update protocol
for milestone, epic, slice, and test-track work.

The architecture and implementation-plan documents remain the source of truth
for scope and decisions. This tracker only records execution state.

## Tracking Rules

1. Track work with roadmap labels in issues, PR titles, branch names, and commit
   messages only.
2. Do not use roadmap labels in production crate names, module names, file names,
   type names, function names, test names, feature flags, error codes, metrics,
   CLI commands, config keys, or user-facing text.
3. Every implementation PR should name exactly one implementation slice or one
   milestone-level planning task.
4. Every test PR should name exactly one test slice or one milestone-level test
   task.
5. A milestone closes only after both its implementation track and test track
   close.
6. If a slice grows beyond one focused pass, split the slice before adding
   temporary compatibility code.
7. Update this tracker when a milestone, epic, or slice changes status.

## Status Vocabulary

| Status | Meaning |
|---|---|
| `Complete` | Scope is implemented, reviewed, fixed, and verified for the slice or milestone gate. |
| `Ready` | Inputs are present and the work may start. |
| `Planned` | Scope exists but an upstream milestone or decision still gates execution. |
| `In Progress` | Active implementation or review is underway. |
| `Review` | Implementation is complete and waiting on review or review response. |
| `Fix` | Review found issues and fixes are underway. |
| `Blocked` | Work cannot proceed until a named blocker closes. |
| `Deferred` | Intentionally outside the current V1 execution path. |

## Label Vocabulary

Use these labels for issues and PRs in any external tracker.

| Label kind | Shape | Example |
|---|---|---|
| V1 effort | `v1` | `v1` |
| Track | `track:implementation`, `track:test`, `track:docs` | `track:test` |
| Milestone | `milestone:M{n}` | `milestone:M3` |
| Epic | `epic:M{n}{letter}` or `epic:M{n}T{letter}` | `epic:M6E`, `epic:M6TD` |
| Slice | `slice:M{n}{letter}{number}` or `slice:M{n}T{letter}{number}` | `slice:M3C1`, `slice:M3TA1` |
| Status | `status:planned`, `status:ready`, `status:in-progress`, `status:review`, `status:fix`, `status:complete`, `status:blocked`, `status:deferred` | `status:review` |
| Risk | `risk:format`, `risk:recovery`, `risk:boundary`, `risk:performance`, `risk:security` | `risk:recovery` |

PR and issue titles should start with the slice or epic code followed by a short
domain title, for example:

```text
M3C1: Add manifest codec golden vectors
M6TD2: Cover recipe freshness degradation
```

## Current Milestone Status

| Milestone | Title | Implementation status | Test status | Gate status | Next action |
|---|---|---|---|---|---|
| `M0` | Architecture freeze and tracking | Complete | Complete | Complete | M0 closed. |
| `M1` | Core-next | Complete | Complete | Complete | M1 closed; start `M2A` and `M2TA` when storage-next implementation begins. |
| `M2` | Storage-next testkit and crate skeleton | Planned | Planned | Planned | Start after core-next atoms exist. |
| `M3` | Storage-next backend, layout, format, and durable services | Planned | Planned | Planned | Start after storage-next skeleton and harness exist. |
| `M4` | Storage-next table, branch, commit, recovery, and L9 API | Planned | Planned | Planned | Start after durable bytes and services are stable. |
| `M5` | Engine-next persistence adapter and control plane | Planned | Planned | Planned | Start after L9 is consumable. |
| `M6` | Engine-next product semantics | Planned | Planned | Planned | Start after engine persistence/control plane are stable. |
| `M7` | Inference-next hardening | Planned | Planned | Planned | May start now if inference-next should run parallel with storage path. |
| `M8` | Intelligence-next orchestration | Planned | Planned | Planned | Start after engine surfaces and inference task contracts are ready. |
| `M9` | Executor, CLI, SDK, tests, benches, and docs cutover | Planned | Planned | Planned | Start after product surfaces stabilize. |
| `M10` | V1 readiness hardening | Planned | Planned | Planned | Start after cutover. |

## Current Epic Status

| Epic | Title | Track | Status | Blocks | Next action |
|---|---|---|---|---|---|
| `M0D` | Tracking setup | Implementation | Complete | none | M0 can close and M1 can start. |
| `M1A` | Core-next crate skeleton | Implementation | Complete | none | `M1A1` complete; continue M1 atom work. |
| `M1TA` | Core atom unit tests | Test | Complete | none | Atom tests cover construction, validation, ordering, parsing, display, and serde behavior through `M1C`. |
| `M1B` | Core atoms | Implementation | Complete | none | `M1B1` through `M1B4` complete. |
| `M1C` | Core atom encodings | Implementation | Complete | none | Parse, display, and serde behavior complete. |
| `M1TB` | Core atom property tests | Test | Complete | none | Generated ordering, boundary, invalid-input, and parse/display/serde round-trip coverage complete. |
| `M1TC` | Core-next dependency guard | Test | Complete | none | Guard fails if core-next declares or resolves a dependency on any other Strata crate. |
| `M1D` | Core-next boundary documentation | Implementation | Complete | none | Implemented public exports and rejected candidates are documented. |
| `M1TD` | Core-next API audit | Test | Complete | none | Public API snapshot guard is checked in and runs with the core-next test suite. |
| `M7A` | Inference task traits | Implementation | Planned | none | May start now if parallel inference work is scheduled. |

## M0 Closure Record

M0 is closed by the following artifacts and recorded verification gates:

| Code | Status | Artifact |
|---|---|---|
| `M0TA` | Complete | Transient verification gate; see the M0TA link check record below. |
| `M0TB` | Complete | Transient verification gate; see the M0TB terminology scan record below. |
| `M0TC` | Complete | `docs/architecture/v1-boundary-baseline.md` |
| `M0A` | Complete | `docs/architecture/v1-document-inventory.md` |
| `M0B` | Complete | `docs/architecture/v1-open-question-register.md` |
| `M0C` | Complete | Roadmap and target crate-shape standards alignment. |
| `M0TD` | Complete | `docs/architecture/v1-engineering-standards-baseline.md` |
| `M0TE` | Complete | `docs/architecture/v1-test-inventory.md` |
| `M0D` | Complete | `docs/architecture/v1-progress-tracker.md` |

## M0TA Link Check Record

`M0TA` is a transient verification gate with no separate artifact file. Its
scope was active V1 architecture, product, and spec documents outside archive
directories.

Closure required local markdown links and backticked local document paths to
resolve, except for classified future references to not-yet-created `*-next`
crates and explicit negative examples. During M0, no real broken required
document links were found.

## M0TB Terminology Scan Record

`M0TB` is a transient verification gate with no separate artifact file. Its
scope was active V1 documents, target crate-shape documents, and implementation
plans.

Closure required cleanup-era milestone vocabulary to be removed, moved to
historical context, or classified as intentional evidence. During M0, the only
domain anti-pattern hits in target crate-shape documents were explicit negative
examples that say not to create production types or modules named `Helper`.

## M1 Closure Record

M1 is closed by the following artifacts and verification gates:

| Code | Status | Artifact |
|---|---|---|
| `M1A` | Complete | `crates/core-next` crate skeleton and workspace membership. |
| `M1TA` | Complete | `crates/core-next/tests/atom_contract.rs` plus module-local atom tests. |
| `M1B` | Complete | `crates/core-next/src/branch.rs`, `crates/core-next/src/version.rs`, and `crates/core-next/src/time.rs`. |
| `M1C` | Complete | Core atom parse, display, serde, and durable representation tests. |
| `M1TB` | Complete | `crates/core-next/tests/atom_properties.rs`. |
| `M1TC` | Complete | `crates/core-next/tests/dependency_guard.rs`. |
| `M1D` | Complete | `docs/architecture/core-next-architecture.md` Implemented M1 Boundary section. |
| `M1TD` | Complete | `crates/core-next/tests/public_api_snapshot.rs` and `crates/core-next/tests/snapshots/public_api.txt`. |

## Slice Update Protocol

When a slice starts:

1. Add the slice code and title to the owning milestone implementation plan if it
   is not already listed.
2. Add or update a tracker row if the slice changes milestone gate status.
3. Link the slice to any open questions it closes.
4. Link the slice to the test-track work that proves it.

When a slice closes:

1. Record the verification commands in the PR or final implementation note.
2. Mark the slice `Complete` in the issue/PR tracker.
3. Update this file only if the slice changes an epic or milestone status.
4. Do not add completed slice codes to production names or user-facing docs.

## Next Work

The current implementation milestone is `M2`. The next slices are:

1. `M2A`: storage-next crate skeleton.
2. `M2TA`: storage-next testkit scaffold.

`M7` may also begin now if inference-next work should run in parallel with the
storage path.
