# M1 / M1T Implementation Plan: Core-Next

Status: draft implementation plan

## Goal

Build the smallest shared contract crate. Core-next must contain only
cross-layer atoms that genuinely belong below both storage-next and engine-next.

## Inputs

1. `docs/architecture/core-next-architecture.md`
2. `docs/architecture/strata-v1-implementation-roadmap.md`
3. `docs/architecture/v1-engineering-standards.md`

All slices must follow the V1 engineering standards: permanent domain names,
concept-budget discipline, file/function thresholds, comment standards, and no
roadmap labels in production code vocabulary.

## Implementation Track

| Epic | Title | Scope | Exit gate |
|---|---|---|---|
| `M1A` | Crate skeleton | Create the core-next crate with crate-level policy, lints, and no Strata dependencies. | Crate builds alone and exposes no accidental public surface. |
| `M1B` | Core atoms | Implement `BranchId`, `CommitVersion`, timestamp representation, and type-local validation errors. | Public surface matches the core-next ownership table. |
| `M1C` | Parsing and serialization | Add parse/display/serde behavior where required by lower layers. | Encodings are explicit and round-trip tested. |
| `M1D` | Boundary documentation | Document why each public type belongs in core-next. | No public item lacks an ownership reason. |

## Test Track

| Test epic | Title | Scope | Exit gate |
|---|---|---|---|
| `M1TA` | Atom unit tests | Validate constructors, parsing, display, ordering, and rejected inputs. | Every core atom has positive and negative tests. |
| `M1TB` | Property tests | Exercise ordering, round trips, and boundary values. | Property tests cover generated values for each atom. |
| `M1TC` | Dependency guards | Prove core-next has no dependency on storage, engine, inference, intelligence, executor, or CLI. | Guard fails on any upward dependency. |
| `M1TD` | API audit | Snapshot the public surface. | Additions require an explicit plan update. |

## Convergence Notes

1. `M1TA` lands with the atom implementation epics it covers.
2. `M1TB` lands before downstream crates treat core encodings as stable.
3. `M1TC` and `M1TD` close before M1 is available to storage-next or
   engine-next.

## Slice Policy

Implementation slices should be per atom or per shared behavior. Do not create a
general-purpose prelude, value model, entity model, backend vocabulary, or
database runtime in core-next.

## Non-Goals

1. No `Value`.
2. No `EntityRef`.
3. No storage transaction IDs.
4. No filesystem, network, runtime, or database behavior.
5. No compatibility shims for old core shapes.

## Milestone Exit Gate

M1 is complete when storage-next and engine-next can depend on core-next without
inheriting product semantics or storage implementation details. The roadmap
Test Gate Summary remains the canonical milestone gate; this plan explains how
M1 reaches it.
