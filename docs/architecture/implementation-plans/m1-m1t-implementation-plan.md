# M1 / M1T Implementation Plan: Core-Next

Status: complete

## Goal

Build the smallest shared contract crate. Core-next must contain only
cross-layer atoms that genuinely belong below both storage-next and engine-next.

## Inputs

1. `docs/architecture/core-architecture.md`
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

## Priority Order

M1 closes in this order unless implementation exposes a smaller sequencing
blocker.

| Priority | Code | Track | Closure condition | Why this order |
|---|---|---|---|---|
| 1 | `M1A` | Implementation | `crates/core-next` exists, builds alone, inherits workspace lints, denies unsafe code, and exposes no accidental public surface. | Later atom work needs the crate boundary and policy in place first. |
| 2 | `M1TA` | Test | Initial atom unit-test scaffold exists and runs with the crate. | Tests should grow with the atoms rather than arrive after the public surface has settled. |
| 3 | `M1B1` | Implementation | `CommitVersion` is implemented with explicit construction, ordering, and local validation. | It is the simplest shared ordering atom and is required by both storage rows and engine version reads. |
| 4 | `M1B2` | Implementation | Timestamp representation is implemented without clock acquisition or runtime behavior. | Storage timeline and engine `as_of` semantics need the same serialized timestamp representation. |
| 5 | `M1B3` | Implementation | `BranchId` is implemented as an opaque identity with no branch-name policy. | Branch identity is shared, but branch naming and product behavior must stay out of core-next. |
| 6 | `M1B4` | Implementation | Type-local validation errors exist only where inseparable from core-owned atoms. | Error vocabulary should stay narrow and must not become a global Strata error layer. |
| 7 | `M1C` | Implementation | Required parse, display, and serde encodings round trip and match the stable core-next contract. | Encodings must settle before storage-next writes durable bytes that depend on these atoms. |
| 8 | `M1TB` | Test | Property tests cover ordering, generated valid values, rejected invalid values, and parse/display/serde round trips. | Property coverage hardens the atom contracts before downstream crates depend on them. |
| 9 | `M1TC` | Test | Dependency guard fails if core-next depends on any higher Strata crate. | Dependency direction must be enforced before M2 starts. |
| 10 | `M1D` | Implementation | Boundary documentation explains every public core-next type and every rejected candidate concept. | The public surface should be explainable before it is treated as available to storage-next and engine-next. |
| 11 | `M1TD` | Test | Public API snapshot exists and matches the M1 boundary documentation. | The final M1 gate should catch accidental API drift. |

## Slice Record

| Slice | Parent | Title | Scope | Verification |
|---|---|---|---|---|
| `M1A1` | `M1A` | Core-next crate skeleton | Add `crates/core-next`, workspace membership, crate-level unsafe policy, empty default feature set, and no dependencies on existing Strata crates. | `cargo check -p strata-core-next`; cargo metadata dependency check. |
| `M1TA1` | `M1TA` | Core-next contract-test scaffold | Add the first integration test target for core-next so atom tests can grow with `M1B1` through `M1C`. Module-local unit tests begin when the first atom module lands. | `cargo test -p strata-core-next --locked`. |
| `M1B1` | `M1B` | Commit version atom | Add `CommitVersion` as an opaque `u64` ordering atom with explicit construction, numeric access, constants, ordering, and checked successor behavior. | `cargo test -p strata-core-next --locked`; `cargo clippy -p strata-core-next --all-targets --locked -- -D warnings`. |
| `M1B2` | `M1B` | Timestamp representation atom | Add `Timestamp` as an opaque microsecond-since-Unix-epoch representation with explicit constructors, numeric accessors, ordering, and deterministic duration arithmetic. Do not add clock acquisition. | `cargo test -p strata-core-next --locked`; `cargo clippy -p strata-core-next --all-targets --locked -- -D warnings`. |
| `M1B3` | `M1B` | Branch identity atom | Add `BranchId` as an opaque sixteen-byte identity with explicit byte construction and access. Do not add branch-name derivation, random allocation, default branch policy, parse/display, or lifecycle behavior. | `cargo test -p strata-core-next --locked`; `cargo clippy -p strata-core-next --all-targets --locked -- -D warnings`. |
| `M1B4` | `M1B` | Type-local validation errors | Add type-local validation errors only where current core-owned atoms can reject input. `BranchId` owns invalid byte-length errors; `CommitVersion` and `Timestamp` do not get speculative errors because their raw domains are valid. | `cargo test -p strata-core-next --locked`; `cargo clippy -p strata-core-next --all-targets --locked -- -D warnings`. |
| `M1C` | `M1C` | Parse, display, and serde encodings | Add raw decimal parse/display/serde encodings for `CommitVersion` and `Timestamp`; add canonical UUID text parse/display plus human-readable serde for `BranchId`. Keep durable byte access explicit through atom accessors. | `cargo test -p strata-core-next --locked`; `cargo clippy -p strata-core-next --all-targets --locked -- -D warnings`; `cargo doc -p strata-core-next --locked --no-deps`. |
| `M1TB1` | `M1TB` | Core atom property tests | Add generated coverage for ordering, boundary values, rejected invalid inputs, and parse/display/serde round trips for every core atom. | `cargo test -p strata-core-next --locked`; `cargo clippy -p strata-core-next --all-targets --locked -- -D warnings`. |
| `M1TC1` | `M1TC` | Core-next dependency guard | Add an executable test that fails if core-next declares or resolves any dependency on another Strata crate. | `cargo test -p strata-core-next --locked`; cargo metadata dependency check. |
| `M1D1` | `M1D` | Core-next boundary documentation | Document the implemented M1 public exports, ownership reasons, allowed associated items, allowed trait implementations, encoding commitments, and rejected candidate concepts. | Documentation review against `crates/core-next/src/lib.rs` exports; `cargo test -p strata-core-next --locked`; `cargo doc -p strata-core-next --locked --no-deps`. |
| `M1TD1` | `M1TD` | Core-next public API snapshot | Add a checked-in public API snapshot and a test that derives declared modules and public types from `src/lib.rs` and atom modules. The snapshot covers root exports, public types, public attributes, enum variants, private field shape for public structs, inherent items, public trait implementations, and public associated types. | `cargo test -p strata-core-next --locked`; `cargo clippy -p strata-core-next --all-targets --locked -- -D warnings`; `cargo doc -p strata-core-next --locked --no-deps`. |

## Convergence Notes

1. `M1TA` starts with `M1A` and expands with `M1B1` through `M1C`.
2. `M1C` may land with the owning atom when the encoding is inseparable from
   that atom, but no downstream crate may depend on the encoding until `M1TB`
   passes.
3. `M1TB` lands before downstream crates treat core encodings as stable.
4. `M1TC` and `M1TD` close before M1 is available to storage-next or
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
