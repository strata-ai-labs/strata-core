# Core Crate Map

Status: current evidence map for the pre-V1 `crates/core` crate

## Purpose

This document maps `crates/core` as it exists today. It is descriptive, not a
target design. The target V1 boundary is defined by
`docs/architecture/core-architecture.md`.

The current crate is already much smaller than the historical cleanup-era core
surface, but it is still not automatically the V1 answer. Core-next must keep
only the shared vocabulary that lower and higher layers genuinely need to agree
on.

## High-Level Shape

Current top-level source files:

- [crates/core/src/lib.rs](../../crates/core/src/lib.rs)
- [crates/core/src/branch.rs](../../crates/core/src/branch.rs)
- [crates/core/src/error.rs](../../crates/core/src/error.rs)
- [crates/core/src/id.rs](../../crates/core/src/id.rs)
- [crates/core/src/value.rs](../../crates/core/src/value.rs)

Current contract subtree:

- [crates/core/src/contract/mod.rs](../../crates/core/src/contract/mod.rs)
- [crates/core/src/contract/branch_name.rs](../../crates/core/src/contract/branch_name.rs)
- [crates/core/src/contract/entity_ref.rs](../../crates/core/src/contract/entity_ref.rs)
- [crates/core/src/contract/primitive_type.rs](../../crates/core/src/contract/primitive_type.rs)
- [crates/core/src/contract/timestamp.rs](../../crates/core/src/contract/timestamp.rs)
- [crates/core/src/contract/version.rs](../../crates/core/src/contract/version.rs)
- [crates/core/src/contract/versioned.rs](../../crates/core/src/contract/versioned.rs)
- [crates/core/src/contract/versioned_history.rs](../../crates/core/src/contract/versioned_history.rs)

## Public Surface

`crates/core/src/lib.rs` currently re-exports:

- `BranchId`
- `CommitVersion`
- `TxnId`
- `Value`
- `BranchName`
- `BranchNameError`
- `EntityRef`
- `PrimitiveType`
- `Timestamp`
- `Version`
- `Versioned`
- `VersionedHistory`
- `VersionedValue`
- `MAX_BRANCH_NAME_LENGTH`

## Current Module Findings

### `id.rs`

`id.rs` owns foundational identifier wrappers:

- `BranchId`: UUID-backed branch identity.
- `TxnId`: transaction-start identifier.
- `CommitVersion`: monotonic MVCC commit version.

V1 ownership notes:

- `BranchId` remains a strong core-next candidate.
- `CommitVersion` remains a strong core-next candidate.
- `TxnId` defaults to storage-next ownership because public manual transaction
  sessions are removed.
- `BranchId::from_user_name` includes branch-name policy. Core-next may keep
  the stable identity representation, but name-to-id derivation defaults to
  engine policy unless V1 explicitly keeps the current derivation as a durable
  compatibility fact.

### `branch.rs`

`branch.rs` currently contains a narrow helper for detecting non-literal aliases
of the reserved default-branch sentinel.

V1 ownership note:

- The helper is branch-name policy, not foundational identity. It defaults to
  engine-next unless a lower layer needs to reject the sentinel before engine
  opens.

### `error.rs`

`error.rs` is intentionally empty except for module documentation.

V1 ownership note:

- This matches the target direction: core-next should not own the database-wide
  error model. Storage, engine, inference, intelligence, executor, and CLI own
  their own boundary errors.

### `value.rs`

`value.rs` owns the current user value model:

- `Null`
- `Bool`
- `Int`
- `Float`
- `String`
- `Bytes`
- `Array`
- `Object`

It also owns value equality and convenience accessors.

V1 ownership note:

- The V1 architecture currently defaults the product value model to engine-next.
  Storage-next stores opaque row bytes and must not inspect product value
  semantics. Move `Value` down to core-next only if storage-next, engine-next,
  intelligence-next, and external SDKs all need the exact same Rust-owned value
  contract.

### `contract/entity_ref.rs`

`EntityRef` currently addresses product entities across KV, event, branch, JSON,
vector, and graph shapes. It includes branch identity and primitive-specific
fields.

V1 ownership note:

- Entity references are product-level addresses. They default to engine-next.
  Storage-next should not persist `EntityRef` directly in commit payloads or row
  keys.

### `contract/primitive_type.rs`

`PrimitiveType` currently names product data families.

V1 ownership note:

- Engine-next owns product capability taxonomy.
- Storage-next owns opaque storage-space IDs and durable section IDs.
- Do not use `PrimitiveType` as a storage routing contract in V1.

### `contract/timestamp.rs`

`Timestamp` currently provides a microsecond timestamp representation and
helpers.

V1 ownership note:

- The representation is a core-next candidate because storage timeline rows,
  engine time travel, and product metadata all need to agree on durable time
  encoding.
- Clock acquisition does not belong in core-next.

### `contract/version.rs`

`Version` is a product-facing version enum with variants such as transaction,
sequence, and counter forms.

V1 ownership note:

- Product-facing version labels default to engine-next.
- The lower shared MVCC token is `CommitVersion`.

### `contract/versioned.rs` And `contract/versioned_history.rs`

These modules currently define product read-result wrappers:

- `Versioned<T>`
- `VersionedValue`
- `VersionedHistory<T>`

V1 ownership note:

- These default to engine-next because they are product API result shapes.
  Storage-next should expose storage rows and timeline facts instead of
  product-shaped result DTOs.

### `contract/branch_name.rs`

`BranchName` owns user-facing branch-name validation and formatting.

V1 ownership note:

- This is a possible core-next type if multiple public layers need the same
  validated name contract.
- Branch creation policy still belongs in engine-next.

## Current Takeaway

The current crate has three categories:

1. likely core-next atoms: `BranchId`, `CommitVersion`, and possibly
   `Timestamp` / `BranchName`
2. engine/product vocabulary that should probably move up: `Value`, `EntityRef`,
   `PrimitiveType`, `Version`, `Versioned`, and `VersionedHistory`
3. storage/runtime vocabulary that should probably move down: `TxnId`

The V1 implementation should use this file as source evidence only. Final
ownership is governed by `docs/architecture/core-architecture.md`.
