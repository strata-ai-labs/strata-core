# Storage-Next File Comment Rollout Plan

Status: draft cleanup plan

## Decision

Do this **directory by directory, with file-by-file review**, after any
near-term type-reduction split for the same directory.

Use the dedicated cleanup prefix `CLN-C*` for this work. PRs should keep the
normal engineering limit of 1,500 net LOC per slice. If a directory pass would
touch too many files, split it by subdirectory or ownership category.

Interlock with
`docs/architecture/cleanup/storage-next-type-proliferation-reduction-plan.md`:
for files scheduled for near-term extraction, write comments on the post-split
files instead of polishing the pre-split file. In particular, defer detailed
comments for `branch/state.rs` until the branch-state extraction slices land,
or make the comments the final step of each extraction slice.

## Goal

Add useful file-level comments to every Rust file in `crates/storage-next/src`
so the next developer can quickly understand:

1. what the file owns;
2. where it sits in the storage stack;
3. which invariants it protects;
4. what it intentionally does not own.

The target style is the old storage crate's module documentation, for example
`crates/storage/src/lib.rs` and `crates/storage/src/memtable.rs`: concise
`//!` comments at the top of the file that explain purpose, ownership, and
read/write behavior before any imports or declarations.

This is documentation cleanup. It must not change runtime behavior, durable
formats, error codes, public API semantics, or source-guard policy.

This cleanup is different from type reduction. For type reduction, the right
unit is operation family. For file comments, the right unit is directory
ownership because each directory should tell a coherent story:

1. `api` describes the public boundary.
2. `backend` describes persistence primitives.
3. `format` describes stable encodings.
4. `branch` describes in-memory branch state and read visibility.
5. `commit` describes commit admission and publication.
6. `lifecycle` describes recovery, maintenance, retention, and close.
7. `service`, `object`, `table`, and `row` describe lower storage services.
8. `testkit` describes reusable conformance harnesses.

Each pass should still be small enough to review. Avoid a single 322-file
comment dump.

Every PR should run:

```sh
cargo fmt --all -- --check
```

If a pass touches anything other than comments and documentation, it is no
longer a file-comment cleanup pass and must use the relevant implementation or
type-reduction verification commands instead.

## Non-Goals

This cleanup must not include:

1. runtime behavior changes;
2. physical format changes;
3. public API removals or signature changes;
4. error-code changes;
5. test additions or removals;
6. executor, engine, or product behavior changes;
7. cleanup of existing `cfg_attr(allow(unused_imports))`,
   `allow(unused_imports)`, or `expect(dead_code)` scaffolding. Those belong
   to the type-reduction plan.

## Comment Standard

Every production Rust file should start with a module-level comment:

```rust
//! <Short noun phrase> -- <one-sentence responsibility>.
//!
//! This module owns <state/operation/boundary>. It is used by <callers/layer>
//! to <reason>.
//!
//! Key invariants, if this file owns durable safety behavior:
//! - <invariant one>
//! - <invariant two>
//!
//! This module does not <nearby responsibility owned elsewhere>.
```

Use this as a guide, not a rigid template. Small files may only need two or
three lines. Complex files should explain the main state transition or safety
boundary, but should not become design documents.

Reserve explicit invariant lists for files where the invariant is durable and
load-bearing: format codecs, recovery boundaries, proof generation, reclaim,
publication windows, branch visibility, and commit durability. For ordinary
adapter or glue files, prefer ownership plus boundary language. Do not create a
parallel design document inside every file header.

### Required Qualities

Good file comments are:

1. **Ownership-first**: explain what the file owns, not just what types it
   contains.
2. **Boundary-aware**: use one of the six categories below.
3. **Invariant-oriented**: call out the safety rule the file protects when one
   exists.
4. **Negative-space aware**: say what the file intentionally leaves to another
   layer when confusion is likely.
5. **Stable**: avoid implementation trivia that will drift after a small
   refactor.

Boundary categories:

1. public API;
2. durable format;
3. lifecycle orchestration;
4. in-memory state;
5. service adapter;
6. test support.

If a file legitimately spans categories, name the primary category first and
the secondary category only if it changes how future edits should be reviewed.

Bad file comments:

1. repeat the file name in prose;
2. list every type in the file;
3. narrate obvious Rust syntax;
4. mention temporary implementation slice labels;
5. include stale TODOs instead of linking to the owning plan;
6. describe product or engine behavior from inside storage internals.

Existing module comments should be preserved when they already meet the
required qualities. The rollout should bring weak or missing comments up to
standard, not churn adequate comments for style uniformity.

## Examples

### Root Module

```rust
//! Storage substrate for Strata.
//!
//! This crate owns the local storage boundary used by the engine-facing API:
//! key/value rows, branch state, commit publication, durable formats, lifecycle
//! recovery, and maintenance services.
//!
//! Product objects and primitive-specific semantics live above this crate.
```

### State Module

```rust
//! Branch-local LSM state and visibility transitions.
//!
//! This module owns the mutable/frozen/owned/inherited layer model for one
//! branch. Commit, flush, compaction, materialization, and checkpoint code use
//! it to mutate branch storage without reaching into durable services directly.
//!
//! Key invariants:
//! - visible rows must not move backward across a committed version;
//! - inherited layers are addressed by stable handles before mutation;
//! - table-object deletion is never inferred from branch-local state alone.
```

### Durable Format Module

```rust
//! Binary encoding for durable table manifests.
//!
//! This module owns the on-disk representation of table reachability facts.
//! The decoder is fail-closed and allocation-bounded; callers must treat decode
//! failure as corruption or unavailable history according to their boundary.
//!
//! Encoding compatibility is owned here, not by lifecycle recovery code.
```

### Test Module

```rust
//! Branch-state regression tests.
//!
//! These tests exercise branch-local invariants directly, without going through
//! the public storage API. API-level behavior is covered by the conformance
//! tests under `tests/`.
```

## Rollout Order

### `CLN-C0`: Inventory

Create a strictly mechanical inventory of files missing module comments.

Suggested command:

```sh
find crates/storage-next/src -type f -name '*.rs' -print | sort
```

For each file, record:

1. whether it starts with `//!`;
2. whether it is production, test-only, or testkit;
3. whether it is in a directory scheduled for near-term type reduction;
4. whether it is explicitly exempt.

The first inventory can live in this cleanup directory or in the porting log
for the cleanup workstream.

Do not score comment quality in this pass. Quality review happens in the
per-directory pass when the editor has enough local context.

### `CLN-C1`: Crate Map And Public Boundary

Files:

1. `src/lib.rs`;
2. `src/api/*`;
3. `src/config/*`;
4. `src/error/*`;
5. `src/observability/*`.

Goal:

Explain the public storage boundary before documenting internals. These files
should make clear which concepts engine callers may depend on and which facts
remain storage-local diagnostics.

Special care:

1. Public API comments must not expose lower-layer concrete types.
2. Comments should not promise distributed durability, serializable isolation,
   product transactions, or object-store semantics.

### `CLN-C2`: Durable Boundaries

Files:

1. `src/backend/*`;
2. `src/format/*`;
3. `src/object/*`;
4. `src/service/*`.

Goal:

Document durable ownership and failure surfaces before documenting lifecycle
users. Format files should state compatibility and fail-closed behavior.
Service files should state which durable family they publish or read.

Special care:

1. Physical format comments must not imply future format changes are casual.
2. Backend comments should distinguish local durability from distributed or
   object-store durability.

### `CLN-C3`: Branch And Commit Core

Files:

1. `src/branch/*`;
2. `src/commit/*`;
3. `src/row/*`;
4. `src/table/*`;
5. `src/layout/*`.

Goal:

Document the core in-memory and table/read structures that most contributors
will touch first. These comments should expose ownership boundaries that the
type-reduction cleanup will later reinforce.

Special care:

1. Large files such as `branch/state.rs` should get comments that describe the
   current file and also identify the intended future split only if the split
   is not imminent. If the type-reduction split is scheduled next, skip the
   detailed comment and write comments on the extracted files instead.
2. Comments should not hide type proliferation by describing every private
   scaffold as a first-class subsystem.

### `CLN-C4`: Lifecycle

Files:

1. `src/lifecycle/*`;
2. `src/lifecycle/durable/*`;
3. any lifecycle-specific test support.

Goal:

Document recovery, maintenance, checkpoint, retention, quarantine, budget, and
close responsibilities in terms of storage safety windows.

Special care:

1. Comments should distinguish direct APIs from queued maintenance tasks.
2. Comments should call out where health debt is recorded, surfaced, or only
   reported.
3. Comments should say when an operation is conservative by design, such as
   failing closed instead of reclaiming.

### `CLN-C5`: Testkit And Tests

Files:

1. `src/testkit/*`;
2. `src/test_support/*`;
3. module-local `tests.rs` files.

Goal:

Explain what each test harness proves and what it deliberately does not prove.
This prevents counter-only assurance tests from looking stronger than they are.

Special care:

1. Test comments should name the behavioral contract, not the slice that added
   the test.
2. Generated or property-style harnesses should state what input bytes control.
3. Per-operation test directories may use one comment on `mod.rs`; individual
   test files under that module inherit it unless the file owns a distinct
   harness or generated input contract.

## Per-File Checklist

Before adding a file comment, answer:

1. What state, format, or boundary does this file own?
2. Which layer calls into it?
3. Which layer must it not call into?
4. What invariant would be dangerous if a future edit broke it?
5. What nearby responsibility is intentionally owned elsewhere?
6. Is this file likely to be split during type-reduction cleanup?

If the answer is unclear, do not invent a confident comment. Write a smaller
comment that states only known ownership, then add the ambiguity to the cleanup
ledger.

## Acceptance Criteria

1. Every production Rust file under `crates/storage-next/src` starts with a
   useful `//!` module comment.
2. Test and testkit files either have `//!` comments or a deliberate exemption
   recorded in the cleanup ledger.
3. Comments do not mention temporary implementation slice labels.
4. Comments do not overclaim durability, isolation, distributed behavior,
   product semantics, or engine-level guarantees.
5. Comments identify important safety boundaries in branch, commit, lifecycle,
   format, and service code.
6. Files scheduled for near-term type extraction are either skipped or receive
   comments only as part of the extraction that creates their final file.
7. Existing adequate comments are left alone.
8. `cargo fmt --all -- --check` remains clean after each pass.

## Optional Guard

After the rollout is complete, add a source guard that checks for missing
module-level comments in production files. The guard should allow targeted
exemptions for generated files or small module glue files, but exemptions must
be listed explicitly.

The guard should check only for the presence of a module comment. Reviewers
still own comment quality.
