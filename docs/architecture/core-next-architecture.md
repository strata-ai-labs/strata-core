# Core-Next Architecture

Status: V1 architecture draft

## Purpose

Core-next is Strata's smallest shared contract layer. It exists so storage,
engine, intelligence, executor, CLI, SDKs, and Strata AI can agree on a narrow
set of foundational types without forcing product behavior, storage mechanics,
or runtime policy into the bottom of the crate graph.

The governing rule is:

```text
core-next defines shared vocabulary, not shared behavior.
```

Core-next is not a general utility crate. Every public type in core-next should
answer two questions:

1. Which layers need this exact concept?
2. Why is this concept not owned more cleanly by storage-next, engine-next,
   intelligence-next, inference-next, executor, or CLI?

If those questions do not have clear answers, the type does not belong in
core-next.

## Related Documents

Read this with:

1. `docs/architecture/strata-v1-architecture.md`
2. `docs/product/strata-v1-product-requirements.md`
3. `docs/product/strata-v1-feature-inventory.md`
4. `docs/product/strata-v1-non-functional-requirements.md`
5. `docs/core/core-charter.md`
6. `docs/core/core-crate-map.md`

The current core crate is useful evidence, not the target by default.

## Current Codebase Findings

The current `strata-core` crate contains three different kinds of things:

1. True cross-layer atoms.
2. Engine/product vocabulary that became shared because multiple higher crates
   needed to talk about product behavior.
3. Storage-facing physical concepts that should be owned by storage, not core.

The V1 redesign should treat the current crate as a source inventory, not as an
ownership decision.

### True Cross-Layer Atoms Today

These are used deeply by both storage and engine and have a reasonable claim on
core-next ownership:

1. `BranchId`
   - Current role: opaque UUID branch identity used for storage namespaces,
     engine branch behavior, executor routing, intelligence contexts, and tests.
   - Caveat: `BranchId::from_user_name` mixes opaque identity with branch-name
     policy. Core-next should keep the identity representation; name-to-id
     derivation should be justified separately and defaults to engine policy.

2. `CommitVersion`
   - Current role: global MVCC visibility token used by storage segments,
     compaction, snapshots, branch fork points, recovery, and engine reads.
   - Verdict: strong core-next candidate.

3. `TxnId`
   - Current role: transaction-start identifier used by WAL records, watermark
     tracking, segment metadata, recovery, and engine coordination.
   - Updated V1 verdict: storage-next owns transaction/runtime identifiers.
     Public manual transaction sessions are removed, and engine-next should not
     expose storage transaction identity as product vocabulary.

4. `Timestamp`
   - Current role: microsecond timestamp representation used by storage TTL and
     history, engine search/time-travel tests, and product result metadata.
   - Caveat: the representation is a core-next candidate; ambient
     `Timestamp::now()` is not. Clock acquisition belongs above core.

5. `Value`
   - Current role: canonical user value enum used by engine, executor,
     intelligence, CLI, and storage persistence.
   - Updated V1 verdict: engine-next owns the user value model. Storage-next
     stores opaque row bytes and must not inspect product `Value` semantics.

### Product Vocabulary In Core Today

These currently live in core but describe engine-level product concepts:

1. `EntityRef`
   - Current role: product entity address across KV, events, JSON, vectors,
     graph, branches, search hits, RAG prompts, executor output, and errors.
   - Boundary issue: storage WAL writesets serialize `EntityRef` today, but that
     is storage depending on product-shaped addressing.
   - Default V1 owner: engine-next.

2. `PrimitiveType`
   - Current role: product data-capability taxonomy.
   - Boundary issue: it also carries WAL byte ranges and snapshot section IDs,
     which are physical storage-format facts.
   - Default V1 owner: split. Engine-next owns product primitive/capability
     taxonomy. Storage-next owns opaque storage space IDs and section envelopes.

3. `Version`
   - Current role: product-facing version enum with `Txn`, `Sequence`, and
     `Counter` variants.
   - Boundary issue: storage uses it mostly because `VersionedValue` leaks into
     the storage trait and `StoredValue` reconstructs product result DTOs.
   - Default V1 owner: engine-next. `CommitVersion` remains the lower-layer
     shared MVCC token.

4. `Versioned<T>`, `VersionedHistory<T>`, and `VersionedValue`
   - Current role: public read-result wrappers for product APIs.
   - Boundary issue: storage currently returns these through its public trait,
     which makes storage expose product-shaped result DTOs instead of storage
     rows.
   - Default V1 owner: engine-next. Move down only if storage-next and
     engine-next documents prove they need the exact same DTO at the lower
     boundary.

5. `BranchName`
   - Current role: validated branch-name newtype with little production use
     outside core.
   - Default V1 owner: engine-next, if it is kept at all. Core-next should only
     own it if branch-name validation becomes a proven cross-layer contract.

### Already Correctly Out Of Core Today

These should not move into core-next by default:

1. Storage `Key`, `Namespace`, and physical `TypeTag`.
   - Current owner: storage.
   - Caveat: current `Key` constructors encode primitive-shaped layouts. That
     should be handled in storage-next/engine-next boundary design, not by
     moving physical keys into core.

2. `StorageError`.
   - Current owner: storage.
   - Verdict: keep storage-owned.

3. `StrataError`.
   - Current owner: engine.
   - Verdict: keep engine-owned.

### Not Implemented Today

These are architecture candidates only. The current codebase does not define
them in production Rust:

1. `DatabaseId`
2. `ReplicaId`
3. `SpaceName`
4. `DatabaseAddress`
5. `BackendAddress`

They should not be added to core-next speculatively. Add one only when a later
storage-next, engine-next, sync, backend, or product-addressing document proves
that the same parsed/serialized concept must exist below engine.

## Resolved V1 Core Surface

After the storage-next and engine-next contracts, core-next should be smaller
than the first draft implied.

The V1 core-next surface should start with only these owned concepts:

| Concept | Core-next decision | Why it belongs below storage and engine |
|---|---|---|
| `BranchId` | Keep | Storage encodes branch identity in physical row keys and branch visibility; engine owns branch product behavior over the same opaque identity. |
| `CommitVersion` | Keep | Storage assigns and stores commit versions; engine exposes version reads, history, branch operations, and product diagnostics over the same ordering token. |
| `Timestamp` representation | Keep | Storage stamps commits and owns the commit timeline substrate; engine resolves `as_of` and branch-from-time using the same serialized timestamp representation. |
| Type-local validation errors | Keep only as needed | Parse/display/serde errors for core-owned transparent types are inseparable from those types. |

Core-next should not contain the following V1 concepts:

| Concept | V1 owner | Reason |
|---|---|---|
| `TxnId` | storage-next | Transaction identity is storage commit/WAL machinery. Public transaction sessions are not V1 product API. Engine may observe recovery facts without owning the type. |
| `Value` | engine-next | It is user/product data vocabulary used by executor, CLI, SDKs, intelligence, and engine. Storage-next stores bytes and should not know product value semantics. |
| `EntityRef` | engine-next | It is product identity across KV, JSON, events, vectors, graph, search, and RAG provenance. |
| `Versioned<T>`, `VersionedHistory<T>`, `VersionedValue` | engine-next | These are product read-result DTOs. Storage-next exposes storage row/history facts. |
| `Version` enum | engine-next | Product-facing version taxonomy belongs with temporal context and read APIs. |
| `BranchName` | engine-next | User-facing branch naming and validation are branch product policy. |
| `DatabaseId` / storage database UUID | storage-next for physical identity; engine-next for product identity | Storage may own a physical database UUID. Engine owns instance, dataset, provenance, and hub-facing identity. |
| `ReplicaId` | post-V1 sync layer | Sync is post-V1 and must not shape V1 core speculatively. |
| `SpaceName` | engine-next | Space semantics are product/data-capability vocabulary. |
| `DatabaseAddress` / `BackendAddress` | storage-next or engine-next, depending on open API | Backend parsing/capability belongs to storage/open policy, not foundational core by default. |
| `StorageSpaceId` | storage-next public boundary plus engine registry | It is a storage physical family byte consumed through the persistence adapter, not a universal core concept. |
| Global error code registry | engine/storage/inference owning layers; maybe a later tiny diagnostics crate | Core-next should not become a database-wide error crate unless the final wire protocol proves the exact type must be shared below engine. |

This keeps core-next to identity and ordering atoms. Higher layers can still
re-export these atoms as part of product APIs, but core-next does not own the
product behavior attached to them.

## Implemented M1 Boundary

The M1 implementation establishes the first concrete `strata-core-next` public
surface. It is intentionally narrower than the current `strata-core` crate.

Crate-level rules:

1. `strata-core-next` has `#![deny(unsafe_code)]`.
2. The only normal dependency is `serde`.
3. Test-only dependencies are `bincode`, `proptest`, and `serde_json`.
4. The crate has no dependency on any other workspace or Strata crate.
5. The dependency guard rejects both named `strata-*` dependencies and any
   workspace-local package dependency other than `strata-core-next` itself.

### Public Exports

| Export | Shape | Ownership reason | Explicit non-ownership |
|---|---|---|---|
| `BranchId` | Opaque sixteen-byte identity | Storage needs the bytes for branch-scoped physical keys and visibility. Engine needs the same identity for branch product behavior. | Core does not create branch IDs, derive IDs from names, define default-branch policy, own branch lifecycle, or know branch DAG semantics. |
| `BranchIdError` | Type-local validation error | Byte and text decoding are inseparable from `BranchId`. Callers need a stable failure type without depending on engine or storage errors. | Core does not map this into product/database error codes. Engine/storage wrap it at their boundaries. |
| `CommitVersion` | Transparent `u64` ordering atom | Storage assigns and stores commit versions. Engine exposes version reads, history, diff/merge inputs, and diagnostics over the same token. | Core does not allocate commit versions, manage transactions, define public transaction sessions, or decide visibility policy. |
| `ParseCommitVersionError` | Type-local parse error | Decimal text parsing is part of `CommitVersion`'s public encoding contract. | Core does not own global error classes, retry policy, or user-facing command errors. |
| `Timestamp` | Transparent `u64` microseconds-since-Unix-epoch representation | Storage stamps commits and stores timeline substrate facts. Engine resolves `as_of`, history, TTL-facing metadata, and branch-from-time over the same representation. | Core does not read clocks, schedule retention, resolve time-travel selectors, or define wall-clock policy. |
| `ParseTimestampError` | Type-local parse error | Decimal text parsing is part of `Timestamp`'s public encoding contract. | Core does not own product time validation, global diagnostics, or command rendering. |

### Public Associated Items

`BranchId` exposes only representation and encoding operations:

1. `BYTE_LEN`
2. `from_bytes`
3. `try_from_slice`
4. `as_bytes`
5. `parse_str`
6. `Display`
7. `FromStr`
8. `Serialize` and `Deserialize`
9. `TryFrom<&[u8]>`

`BranchId` display and human-readable serde use canonical lowercase UUID text.
Parsing accepts uppercase or lowercase hex. Durable storage must use
`as_bytes()` or `try_from_slice()`, not display strings.

`CommitVersion` exposes only ordering and representation operations:

1. `ZERO`
2. `MAX`
3. `new`
4. `as_u64`
5. `checked_next`
6. Ordering traits
7. `Display`
8. `FromStr`
9. Transparent serde

`CommitVersion` display and parsing use unsigned decimal text. A leading `+`
or any non-decimal decoration is rejected. Commit version allocation and commit
ordering are storage/engine responsibilities, not core responsibilities.

`Timestamp` exposes only representation and deterministic arithmetic:

1. `EPOCH`
2. `MAX`
3. `from_micros`
4. `from_millis`
5. `from_secs`
6. `from_duration_since_epoch`
7. `as_micros`
8. `as_millis`
9. `as_secs`
10. `duration_since`
11. `saturating_add`
12. `saturating_sub`
13. Ordering traits
14. `Display`
15. `FromStr`
16. Transparent serde

`Timestamp` display and parsing use unsigned decimal microseconds. A leading
`+` or any non-decimal decoration is rejected. Core does not expose `now()` or
any ambient clock source.

### Public Trait Surface

The following trait implementations are part of the M1 public boundary. New
public trait implementations require the same review as new inherent methods.

| Type | Allowed public trait surface | Boundary note |
|---|---|---|
| `BranchId` | `Clone`, `Copy`, `Debug`, `PartialEq`, `Eq`, `Hash`, `Display`, `FromStr`, `TryFrom<&[u8]>`, `Serialize`, `Deserialize` | Equality and hashing use the opaque bytes. There is intentionally no `Default` implementation because core-next must not create a sentinel or default branch identity. |
| `BranchIdError` | `Clone`, `Copy`, `Debug`, `PartialEq`, `Eq`, `Display`, `std::error::Error` | Comparison is allowed because the error vocabulary is closed and type-local. |
| `CommitVersion` | `Clone`, `Copy`, `Debug`, `Default`, `PartialEq`, `Eq`, `PartialOrd`, `Ord`, `Hash`, `Display`, `FromStr`, `Serialize`, `Deserialize` | `Default` is intentionally `CommitVersion::ZERO`; it is representation defaulting only, not commit allocation policy. |
| `ParseCommitVersionError` | `Debug`, `Display`, `std::error::Error` | The parse source is private; callers should use the local error type or wrap it at a higher boundary. |
| `Timestamp` | `Clone`, `Copy`, `Debug`, `Default`, `PartialEq`, `Eq`, `PartialOrd`, `Ord`, `Hash`, `Display`, `FromStr`, `Serialize`, `Deserialize` | `Default` is intentionally `Timestamp::EPOCH`; it is representation defaulting only, not a clock or product timestamp policy. |
| `ParseTimestampError` | `Debug`, `Display`, `std::error::Error` | The parse source is private; callers should use the local error type or wrap it at a higher boundary. |

### Explicitly Rejected From M1

The following candidates remain out of `strata-core-next` after M1:

| Candidate | Owner for V1 | Reason it stays out of core-next |
|---|---|---|
| `TxnId` | storage-next | It is commit/WAL machinery. Public transaction sessions are removed from the V1 product surface. |
| `Value` | engine-next | It is user/product data vocabulary. Storage-next stores opaque row bytes. |
| `EntityRef` | engine-next | It is product identity across capabilities and relationship/search surfaces. |
| `Version`, `Versioned<T>`, `VersionedHistory<T>`, `VersionedValue` | engine-next | These are product read-result DTOs, not storage/core atoms. |
| `BranchName` | engine-next | User-facing branch naming, validation, and alias policy are branch product behavior. |
| `StorageSpaceId` | storage-next boundary plus engine registry | It is a physical storage-family byte and registry concern, not a universal core atom. |
| `DatabaseId` / `ReplicaId` | storage-next, engine-next, or post-V1 sync depending on use | V1 does not need one shared below-engine identity type. |
| `DatabaseAddress` / `BackendAddress` | storage-next or engine-next open policy | Backend parsing and capability checks are not foundational core behavior. |
| `StrataError`, `StorageError`, global error-code registry | owning higher layers | Core owns only errors inseparable from core-owned types. |
| Runtime, filesystem, networking, model-provider, OpenDAL, async, lock, or IPC types | owning higher layers | These carry deployment or product behavior and would pollute the bottom of the crate graph. |

M1 is complete only while this table and the Rust public surface stay aligned.
M1TD's `public_api_snapshot.rs` and `tests/snapshots/public_api.txt` snapshot
the public API and fail on drift so later additions require an explicit
architecture update.

## Layer Position

The V1 target stack is:

```text
core-next -> storage-next -> engine-next -> intelligence-next -> executor / cli / SDK / Strata AI
                                      intelligence-next -> inference-next
```

Core-next has no normal production dependency on any other Strata crate.

Allowed dependencies should be boring and justified:

1. `serde` for stable serialization contracts.
2. Small external crates for strongly justified foundational types, such as UUID
   handling, if the type contract requires them.
3. No storage, engine, executor, intelligence, inference, CLI, OpenDAL, runtime,
   networking, model-provider, or filesystem dependencies.

Core-next must remain usable by every higher layer without pulling in runtime
policy or deployment assumptions.

## Design Rules

### 1. Core By Necessity

Core-next owns a type only when more than one architecture layer needs the same
contract and no higher layer is the natural owner.

Shared use alone is not enough. A concept can be used by many crates and still
belong in engine if it represents product semantics.

### 2. Opaque Identity Over Policy

Core-next may define identifiers and transparent newtypes. It should not define
the lifecycle, allocation policy, validation policy, or user workflow attached
to those identifiers unless the behavior is inseparable from the type.

Example:

1. `BranchId` may belong in core-next.
2. Branch creation, default-branch bootstrap, branch deletion, branch DAG
   policy, merge policy, and branch-from-history belong in engine-next.

### 3. Explicit Construction Over Ambient State

Core-next types should prefer explicit constructors over ambient state.

Wall-clock access, randomness, process globals, filesystem access, network
access, model calls, and background runtime assumptions do not belong in
core-next. If a higher layer needs a timestamp or ID from the environment, that
layer should provide it explicitly.

### 4. Stable Serialization Is A Contract

When core-next exposes serialized types, the wire shape is part of the contract.
Types should use transparent wrappers where appropriate and should have tests
that lock down JSON and binary-compatible behavior where those formats are
claimed.

### 5. No Product Surface By Accident

Core-next should not define user-facing product taxonomy just because multiple
crates need to mention it today.

Data capabilities, command names, error messages, IPC behavior, search stages,
model-provider names, storage backend capabilities, and CLI affordances are not
core concepts by default.

### 6. Small Enough To Audit

Core-next should be small enough that a contributor can read the whole crate
quickly and understand why every module exists. If a module needs a long local
architecture document to justify itself, it probably belongs higher.

## Allowed Responsibilities

### Stable Identifiers

Core-next may own identifiers that are cross-layer facts:

1. `BranchId`
2. `CommitVersion`, because storage and engine both need the same commit/version
   ordering newtype

Rules:

1. Use transparent newtypes for primitive-backed identifiers.
2. Prefer explicit `from_bytes`, `from_u64`, parse, display, and serde
   behavior over allocation helpers.
3. Keep allocation policy out of core unless the deterministic derivation is
   part of the identifier contract.
4. Random ID generation belongs above core unless a tiny generator helper is
   explicitly approved as part of the identifier contract.
5. Deriving a branch ID from a user-facing branch name is engine policy by
   default; core should own only the opaque branch ID representation unless a
   later branch identity contract proves otherwise.
6. Keep lifecycle policy out of core.
7. Do not put branch DAG, sync, retention, or merge behavior on the identifier.

### Time And Version Vocabulary

Core-next may own simple time and version vocabulary when multiple layers
need the same representation.

Allowed:

1. Timestamp representation.
2. Explicit timestamp constructors.
3. Commit/version newtypes and ordering wrappers that are shared below engine.

Not allowed:

1. Ambient `now()` as the default way to create versioned data.
2. Clock source ownership.
3. Retention policy.
4. Time-travel resolution.
5. Branch-from-time behavior.
6. Product-facing result wrappers by default.
7. Primitive-specific version enums by default.

Engine-next owns the meaning of timestamp selectors, retained history, and
time-travel failures.

### User Value Model

Core-next does not own the V1 user value model.

The canonical value type belongs in engine-next because it is product/API
vocabulary. Executor, CLI, SDKs, intelligence, and Strata AI may consume or
re-export the engine-owned value type. Storage-next stores opaque row bytes and
does not need to inspect value semantics to preserve durability.

Engine-next may define:

1. Null.
2. Boolean.
3. Integer.
4. Float.
5. String.
6. Bytes.
7. Array.
8. Object.

Engine-next also owns JSON path mutation, JSON merge behavior, search
extraction, embedding extraction, graph property interpretation, product
validation limits, and value encoding for engine-owned rows.

### Address And Name Vocabulary

Core-next may own address or name newtypes only when they are cross-layer
contracts.

Candidates:

1. `BranchId`
2. No other V1 address/name type is currently approved for core-next.

Default ownership:

1. Branch name validation belongs in engine-next unless storage-next needs the
   same validated user-facing name.
2. Space product semantics belong in engine-next.
3. Backend capability decisions belong in storage-next.
4. CLI address parsing belongs in CLI/executor unless engine and storage need
   the same parsed contract.

### Type-Local Errors

Core-next may own small validation errors that are inseparable from core-owned
types.

Examples:

1. Invalid transparent ID parse.
2. Invalid core-owned name newtype.
3. Invalid core-owned timestamp or version representation.

Core-next must not own the parent database error. Engine-next owns the product
parent error. Storage-next owns the storage parent error. Inference-next owns
provider/model execution errors.

## Explicit Non-Responsibilities

Core-next must not own:

1. Storage provider traits.
2. Storage backend capability checks.
3. Physical keys, type tags, namespaces, segments, manifests, WAL records,
   snapshots, checkpoints, compaction, retention, or recovery mechanics.
4. Database open policy.
5. IPC behavior.
6. Branch lifecycle, DAG, merge, diff, restore, copy, promote, or
   branch-from-history behavior.
7. Public transaction sessions or commit orchestration.
8. JSON document semantics.
9. Event append/query semantics.
10. Graph ontology, traversal, analytics, or relationship-layer semantics.
11. Vector collection, embedding, or index semantics.
12. Search ranking, indexing, query expansion, reranking, or RAG semantics.
13. Model provider names, model runtime behavior, tokenization, embedding, or
    generation policy.
14. CLI commands, render modes, command routing, or SDK ergonomics.
15. Product defaults, feature gates, or optional feature availability.
16. Global error taxonomy for the whole database.
17. Generic helper modules that are not inseparable from a core-owned type.

## Candidate Public Surface

The first `core-next` design pass should classify candidate types into one of
four groups.

### Keep In Core

V1 core-owned:

1. `BranchId`
2. `CommitVersion`
3. `Timestamp` representation
4. Type-local parse/validation errors for those types

### Keep Only If Proven Cross-Layer

Possible core-owned, but not automatic:

1. `DatabaseId`
2. `ReplicaId`
3. `TxnId`
4. `SpaceName`
5. `DatabaseAddress`
6. `BackendAddress`
7. Branch-name validation, if storage-next and engine-next both require exactly
   the same validated user-facing name
8. Versioned result wrappers, only if storage-next and engine-next explicitly
   choose the same lower-boundary DTO

These require explicit proof that storage-next and engine-next both need the
same type and that neither layer is the natural owner.

### Default To Engine

Default engine-owned:

1. Data capability taxonomy.
2. Entity references across KV, JSON, events, graph, vectors, and search.
3. Branch names and branch aliases.
4. Versioned product result shapes.
5. Product-facing `Version` enums such as transaction, sequence, and counter
   variants.
6. Time-travel selectors.
7. Relationship-layer references.
8. Graph, vector, search, JSON, event, and KV product DTOs.
9. Canonical user `Value`.
10. Dataset identity, instance identity, provenance identity, and hub-facing
    identity.

The current `PrimitiveType` and `EntityRef` shapes are useful evidence, but
they are data-capability product vocabulary. They should move to engine-next
unless a later command-boundary contract proves a narrower core-owned reference
is required.

The current `Versioned<T>`, `VersionedHistory<T>`, and `VersionedValue` shapes
are also useful evidence, but they are public read-result vocabulary. They
default to engine-next unless the storage-next consumption contract deliberately
chooses them as the storage/engine boundary type.

The current storage-next boundary does not choose those product DTOs. L9 should
define storage-local row/result DTOs, and engine-next should translate them into
product-facing `Versioned` and history shapes.

Storage recovery health vocabulary is also not a core-next default.
`RecoveryHealth`, `DegradationClass`, and `RecoveryFault` are storage-owned
facts produced by L8. Engine-next may re-export or wrap them as part of its D4
diagnostic surface, but core-next should not own recovery semantics.

### Do Not Carry Forward

Do not put these in core-next:

1. Storage traits.
2. Storage `Key`, `Namespace`, or physical `TypeTag`.
3. Global `StrataError`.
4. Storage-owned transaction/runtime IDs unless a later boundary proves engine
   must own the exact same type.
5. `StorageSpaceId`.
6. Limits and validation policy.
7. JSON path and patch helpers.
8. Event-chain verification.
9. Vector model presets.
10. Search text extraction.
11. Runtime, filesystem, or network helpers.

## Error And Result Vocabulary

Core-next should avoid owning broad errors.

Allowed:

1. Type-local validation errors.
2. Parse errors for core-owned IDs or names.
3. Conversion errors for core-owned representation adapters.

Not allowed:

1. `StrataError` as a core-owned universal parent error.
2. `StorageError`.
3. Engine lifecycle errors.
4. IPC transport errors.
5. Search/vector/graph/intelligence/model-provider errors.
6. Backend capability errors.

Layer ownership:

1. Storage-next owns storage and backend errors.
2. Engine-next owns product/database errors.
3. Intelligence-next owns retrieval orchestration errors.
4. Inference-next owns provider/model execution errors.
5. Executor/CLI own command parsing and rendering errors.

Core-owned errors should be small, `#[non_exhaustive]` where public, and tested
through their owning type.

## Serialization And Compatibility Rules

Core-next types are low in the stack. Changing their serialized shape has wide
blast radius.

Rules:

1. Use `#[repr(transparent)]` and `#[serde(transparent)]` for primitive-backed
   newtypes where possible.
2. Do not expose serialized enums casually; every enum variant becomes a
   compatibility commitment once public.
3. Prefer explicit versioning for serialized contract families that may evolve.
4. Do not mix product labels with storage encoding tags.
5. Do not use display strings as durable format.
6. Keep JSON adapters separate from binary/durable encoding adapters.
7. Test serde round trips and canonical representations for every public
   serialized type.

Pre-V1 allows breaking changes, but the architecture should still force every
break to be deliberate.

## Dependency Rules

Core-next must have no dependency on any Strata crate.

Core-next must not depend on:

1. storage-next
2. engine-next
3. intelligence-next
4. inference-next
5. executor
6. CLI
7. OpenDAL
8. async runtimes
9. filesystem locking crates
10. networking clients
11. model-provider clients

External dependencies must be few and justified in the crate-level docs.

If a proposed dependency exists only for convenience, reject it. If a dependency
pulls runtime behavior into core-next, reject it.

## Testing Requirements

Core-next tests should be small, fast, and exhaustive for the owned contracts.

Required tests:

1. Transparent newtype serialization tests.
2. Stable ID parse/display/round-trip tests.
3. Timestamp and version boundary tests.
4. Public error display/source tests for type-local errors.
5. Compile-time or guard tests proving no Strata-crate dependencies.
6. Public surface snapshot tests or equivalent review guard.

Property tests should cover:

1. ID round trips.
2. Timestamp arithmetic.
3. Name/address parsing only if a later approved core-owned type requires it.

Core-next tests must not require filesystem, network, model providers,
background runtimes, or a database instance.

## Closed Design Questions

The storage-next and engine-next contracts close the first core ownership pass:

1. `Value` moves to engine-next.
2. `TxnId` defaults to storage-next.
3. `CommitVersion` remains core-next.
4. `Timestamp` is a core representation type only; clock acquisition is not
   core behavior.
5. Backend address syntax is not core-next for V1. Storage-next owns backend
   capability/address mechanics exposed through its open boundary, and
   engine-next owns product open policy.
6. `DatabaseId` is not core-next for V1. Storage may own a physical database
   UUID; engine owns product instance/dataset/provenance identity.

The current core crate must not be copied forward wholesale. Core-next should
start from the resolved V1 surface above and add nothing without a written
owner justification.

## Acceptance Criteria

Core-next is correctly designed when:

1. Its public surface can be listed in one short table.
2. Every public type has a written owner justification.
3. It has no Strata-crate dependencies.
4. It has no filesystem, network, runtime, model-provider, OpenDAL, or storage
   backend dependency.
5. It contains no storage mechanics.
6. It contains no data-capability behavior.
7. It contains no branch lifecycle or graph behavior.
8. It contains no global database error.
9. It can be tested without opening a database.
10. Storage-next and engine-next can depend on it without inheriting product
    policy from below.

## Next Documents

This document now feeds:

1. `docs/architecture/storage-next-architecture.md`
2. `docs/architecture/engine-next-architecture.md`
3. `docs/architecture/strata-v1-implementation-roadmap.md`

The first core-next implementation plan should use the resolved V1 surface as
its starting checklist and reject convenience additions by default.
