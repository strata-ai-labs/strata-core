# Strata V1 Engineering Standards

Status: V1 architecture draft

## Purpose

This document defines the engineering standards for the V1 rewrite. It is the
permanent style and structure contract that implementation plans must follow.

The roadmap uses milestone labels such as `M1`, `M1A`, and `M1A1` to organize
work. Those labels are planning metadata only. They must not become production
vocabulary in file names, module names, type names, function names, test names,
comments, error codes, telemetry keys, public APIs, or user-facing text.

The V1 codebase should read as if it was designed directly around Strata's
domain model, not as if it accumulated cleanup phases.

## Related Documents

1. `docs/architecture/strata-v1-architecture.md`
2. `docs/architecture/strata-v1-implementation-roadmap.md`
3. `docs/architecture/v1-existing-test-inventory-and-porting-plan.md`
4. `docs/architecture/v1-error-and-diagnostics-contract.md`
5. `docs/architecture/v1-testing-and-conformance-plan.md`
6. `docs/architecture/core-next-architecture.md`
7. `docs/architecture/storage-next/implementation-patterns.md`
8. `docs/architecture/storage-next/target-crate-shape-and-test-harness.md`
9. `docs/architecture/engine-next/target-crate-shape-and-test-harness.md`
10. `docs/architecture/inference-next-architecture.md`
11. `docs/architecture/intelligence-next-architecture.md`
12. `docs/architecture/v1-open-question-register.md`
13. `docs/architecture/v1-engineering-standards-baseline.md`

## Requirement Language

1. Must means the rule is required for V1 implementation.
2. Should means the rule is expected unless a local implementation plan records
   a concrete reason to diverge.
3. May means allowed but not required.

## Planning Labels Are Not Code Vocabulary

Allowed uses of roadmap labels:

1. Architecture and implementation-plan documents.
2. Roadmap checklists.
3. PR titles and descriptions.
4. Issue tracker labels.
5. Commit messages, if useful.

Forbidden uses of roadmap labels:

1. Rust crate names, modules, files, types, traits, functions, fields, tests, or
   feature flags.
2. Public API names.
3. CLI commands, flags, config keys, error codes, telemetry names, metric names,
   or log fields.
4. Production comments and doc comments.
5. Fixture names, unless the fixture is explicitly part of a roadmap document
   test for this standards policy.

Forbidden cleanup-era vocabulary includes:

1. Milestone and slice labels such as `M1`, `M1A`, `M1A1`, `M1T`, and
   variants like `m1t1c`.
2. Historical cleanup labels such as `ES`, `EG`, `STAB`, `ESTAB`, `EX`, and
   similarly shaped phase labels.
3. `phase`, `epic`, `slice`, `cleanup`, `legacy`, or `next` in production names
   when the word describes project history rather than domain meaning.

Temporary `*-next` crate or package names are allowed only as build-branch
scaffolding during the V1 effort. They must be removed at cutover. Code inside
those crates should still use permanent domain names.

## Naming Standards

Names must describe the domain object, behavior, or boundary being implemented.
They should not describe how the work was scheduled.

Preferred type suffixes:

1. `Id`: opaque identity with stable equality.
2. `Name`: validated human or object name.
3. `Address`: parsed location or backend address.
4. `Key`: ordered storage key or lookup key.
5. `Options`: caller-supplied knobs before validation.
6. `Config`: validated or resolved runtime configuration.
7. `Plan`: preflighted work that has not mutated state yet.
8. `Record`: durable log, table, manifest, or snapshot unit.
9. `Entry`: iterator item or key/value item.
10. `Facts`: observed durable state or capability facts.
11. `Outcome`: result of work that has completed.
12. `Stats`: counters only.
13. `Report`: diagnostic or user-facing summary.
14. `Error`: typed failure.

Names to avoid unless the implementation plan justifies them:

1. `Manager`
2. `Coordinator`
3. `Runtime`
4. `Context`
5. `Helper`
6. `Util`
7. `Facade`
8. `Bridge`
9. `Adapter`

These names are not banned, but they are often signs that the code is hiding an
unclear ownership boundary. A type should usually be named for what it owns,
not for the fact that it calls another type.

## Concept Budget

Strata already has too many one-off named concepts. V1 should use a smaller set
of repeatable shapes.

Before adding a new public or crate-wide type, the implementation must answer:

1. Is this a domain concept users or maintainers need to reason about?
2. Is this a durable format concept?
3. Is this a real boundary between layers?
4. Is this a reusable test harness concept?
5. Can this be represented by an existing `Plan`, `Record`, `Outcome`,
   `Facts`, `Config`, or `Error` shape?

If the answer to all five questions is no, the type is probably unnecessary.

Small private structs are allowed when they reduce local complexity, but they
should not introduce new vocabulary that spreads across modules. Prefer plain
functions and existing records for local grouping.

## Module Standards

Module names must be stable domain names.

Forbidden module shapes:

1. Numbered architecture layers such as `l1`, `l2`, or `l9`.
2. Roadmap labels such as `m1a`, `eg4`, or `stab1`.
3. Catch-all modules such as `misc`, `utils`, `helpers`, or `common`.
4. Revived mega-modules that collect unrelated behavior under historical names.
5. Public modules created only to preserve an old import path.

Storage-next should fold the L1-L9 architecture into domain modules such as
backend, layout, format, service, table, branch, commit, lifecycle, and api.

Engine-next should use domain buckets such as api, runtime, commit, branch,
data, entity, control, orchestration, retrieval, persistence, diagnostics,
command, clone, and config.

The exact crate shape is governed by the target crate-shape documents. This
document provides the general rule: module names should be meaningful after the
roadmap is forgotten.

## File Size Standards

File-size limits are review thresholds. They are not a substitute for judgment,
but crossing them requires an explicit split decision.

| File kind | Target | Review required | Split or justify |
|---|---:|---:|---:|
| `lib.rs` | 150 LOC | 200 LOC | 300 LOC |
| `mod.rs` | 250 LOC | 350 LOC | 500 LOC |
| Production module | 400 LOC | 500 LOC | 800 LOC |
| Integration test file | 500 LOC | 700 LOC | 1000 LOC |
| Unit test module | 400 LOC | 600 LOC | 900 LOC |
| Generated or golden data | no target | review at 1500 LOC | must be marked generated or fixture |

Lines of code means non-blank, non-comment source lines when practical. A
simple `wc -l` audit is acceptable for early review, but large comments and
fixtures should be interpreted with judgment.

A file should be split before the numeric threshold if:

1. It has multiple independent responsibilities.
2. It needs large section comments to remain navigable.
3. Tests for unrelated behavior live together.
4. Public API, internal machinery, and test support are interleaved.
5. Helpers are only understandable by reading the whole file.
6. The file crosses an architecture boundary.

Permitted exceptions:

1. Durable format tables.
2. Generated bindings or generated lookup data.
3. Golden fixtures.
4. Parser or encoder modules that are intentionally table-driven.
5. Long conformance files where splitting would obscure the matrix.

Every exception should be obvious from the file name or a short module-level
comment.

## Function Size Standards

Function limits:

| Function kind | Target | Review required | Split or justify |
|---|---:|---:|---:|
| Normal production function | 40 LOC | 60 LOC | 100 LOC |
| Complex recovery or commit path | 60 LOC | 90 LOC | 140 LOC |
| Test function | 50 LOC | 80 LOC | 130 LOC |

Large functions are acceptable only when splitting would hide a critical
ordered sequence, such as a recovery state machine or commit publication path.
In that case, keep the function linear, name each step clearly, and push
mechanical sub-work into private helpers.

## Trait Standards

Traits should represent real substitution boundaries.

Good trait reasons:

1. Multiple backend families implement the same storage boundary.
2. Fault-injection or conformance harnesses need a controlled implementation.
3. Engine needs a stable capability boundary over persistence or inference.
4. The trait is part of a public extension surface.

Bad trait reasons:

1. There is one implementation and no planned second implementation.
2. The trait exists only to break a compile cycle.
3. The trait hides ownership confusion.
4. The trait is a temporary facade during migration.

If a trait has one implementation at V1 freeze, the implementation plan should
state why the trait still belongs in the final architecture.

## Error Handling Standards

Errors must follow `docs/architecture/v1-error-and-diagnostics-contract.md`.

Rules:

1. Use typed errors with stable error codes and classes.
2. Do not match on display strings.
3. Do not introduce ad hoc string errors for recoverable product behavior.
4. Do not suppress errors without a rationale comment.
5. Avoid `unwrap`, `expect`, and panics in production code. Use them only for
   impossible invariants, and include a precise message.
6. Do not use `.ok()`, `let _ =`, or `.unwrap_or_default()` on fallible work
   unless the ignored failure is intentional and documented.
7. Ambiguous commit outcomes must remain distinguishable from clean failures.
8. Secret values, prompts, document contents, and API keys must not appear in
   error messages or logs by default.

Panic policy:

1. Panics are for programmer bugs and impossible states.
2. User input, backend failures, corrupt data, missing models, unavailable IPC,
   and unsupported features must return structured errors.

## Comment Standards

Comments should explain why code exists, which invariant it preserves, or which
failure window it controls. They should not narrate obvious code.

Good comments explain:

1. Durable format invariants.
2. Recovery ordering.
3. Lock ordering.
4. Branch visibility rules.
5. Cache-vs-durable behavior.
6. Error classification choices.
7. Unsafe code preconditions.
8. Test harness fault windows.

Bad comments:

1. Roadmap history.
2. "Temporary for M1A1" style notes.
3. Restatements of code.
4. TODOs without a removal condition.
5. Explanations of deleted architecture that no longer exists.

Public items must have doc comments when they are part of a public API,
cross-crate testkit, durable format, or error-code contract. Private items
should have comments only when the code is not self-explanatory.

TODO format:

```text
TODO(<area>): <specific action>; remove when <condition>.
```

The area should be a domain area such as `storage-format`, `ipc`, `timeline`,
or `inference-provider`, not a roadmap label.

Unsafe code must use a nearby `SAFETY:` comment that states the exact invariant
that makes the block sound.

## Test Standards

Test names must describe behavior, not milestones.

Good:

1. `cache_open_does_not_create_durable_objects`
2. `branch_from_timestamp_uses_visible_commit_before_cutoff`
3. `source_wins_refuses_divergent_event_appends`
4. `ipc_read_only_client_rejects_writes`

Bad:

1. `m1a1_cache_test`
2. `eg4_graph_cutover_works`
3. `stab1_regression`
4. `test_new_architecture_phase_two`

Existing tests are evidence, not authority. Ported tests must be renamed to
describe V1 product behavior or V1 invariants. Tests that preserve old behavior
only because it existed should be rewritten, archived, or deleted according to
`docs/architecture/v1-existing-test-inventory-and-porting-plan.md`.

Every milestone test track should include:

1. Fast deterministic unit tests.
2. Integration tests over public or layer-boundary APIs.
3. Dependency guard tests for architecture boundaries.
4. Error-code tests for classified failures.
5. Conformance or property tests where the architecture contract requires
   broad coverage.

## Public API Standards

Public APIs must be product-shaped and minimal.

Rules:

1. `pub(crate)` is the default.
2. `pub` requires a documented public surface reason.
3. Do not expose storage internals above engine.
4. Do not expose engine internals through executor or CLI.
5. Do not add a second public way to do the same operation.
6. Do not preserve old names only for compatibility during the V1 branch.
7. Do not expose manual maintenance commands as ordinary product workflows.
8. Do not expose manual transaction sessions as the V1 product write model.

Feature flags must describe capability, not implementation phase. Examples:
`localfs`, `testkit`, `fault-injection`, `retrieval-augmentation`, and
`openai` are acceptable. `m2`, `new-engine`, or `rewrite` are not.

## Dependency Standards

The normal production dependency direction is:

```text
core -> storage -> engine -> intelligence -> executor / cli / sdk / Strata AI
                         intelligence -> inference
```

Rules:

1. Only engine consumes storage directly in normal production code.
2. Intelligence consumes engine and inference, not storage.
3. Inference does not depend on Strata database crates.
4. Executor and CLI route commands; they do not own product semantics.
5. Testkit dependencies must be feature-gated and marked non-production.
6. Boundary violations need dependency guard tests, not only comments.

## Concurrency And Global State Standards

Process-global mutable state is disallowed unless an architecture document
names it and explains why it cannot be scoped.

Rules:

1. Prefer database-local state.
2. Guard unavoidable globals with deterministic reset behavior in tests.
3. Do not let one test's global state affect another test.
4. Lock order must be documented for multi-lock paths.
5. Crash and recovery tests must use deterministic fault points.
6. Background maintenance must be observable and stoppable during tests.

## Generated Code And Fixtures

Generated files must begin with a short generated-file notice and must not be
manually edited.

Fixtures must:

1. Live under a fixture or golden-data path.
2. State which format or behavior they pin.
3. Avoid roadmap labels unless they test this standards document.
4. Avoid embedding secrets or machine-local paths.
5. Be updated only through explicit fixture-update commands.

## Enforcement

Implementation plans should include these checks where applicable:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
rg -n "\\b(M[0-9]+[A-Z0-9]*|EG[0-9]+[A-Z]?|ES[0-9]+[A-Z]?|STAB[0-9]+[A-Z]?|ESTAB|m[0-9]+t[0-9]+c)\\b" crates tests
```

File-size audits may use `tokei`, `scc`, or `wc -l`. The tool is less
important than recording which files crossed the review thresholds and why.

Dependency guards should use `cargo metadata` or equivalent workspace analysis
when boundary rules cannot be enforced by Rust visibility alone.

## Review Checklist

Before a V1 implementation slice is accepted:

1. No production name contains roadmap or cleanup-era vocabulary.
2. New concepts fit the concept budget or are justified by a boundary.
3. Files and functions are under the size thresholds or have explicit
   exceptions.
4. Comments explain invariants, not project history.
5. Public API additions are intentional and minimal.
6. Errors are structured and classified.
7. Tests are named for behavior and do not freeze obsolete implementation
   details.
8. Dependency boundaries are enforced by tests or visibility.
9. No new process-global semantic state was added without an architecture
   decision.
10. The implementation can be understood without reading roadmap history.
