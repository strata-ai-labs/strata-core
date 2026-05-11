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
   - Verdict: core-next candidate while transaction identity is shared between
     storage-next and engine-next. Revisit if storage-next fully owns
     transaction runtime identity.

4. `Timestamp`
   - Current role: microsecond timestamp representation used by storage TTL and
     history, engine search/time-travel tests, and product result metadata.
   - Caveat: the representation is a core-next candidate; ambient
     `Timestamp::now()` is not. Clock acquisition belongs above core.

5. `Value`
   - Current role: canonical user value enum used by engine, executor,
     intelligence, CLI, and storage persistence.
   - Verdict: core-next candidate as the canonical user value model. Storage-next
     should still avoid semantic dependence on `Value`; preserving durability
     should not require understanding JSON paths, graph properties, search
     text, embeddings, or product validation limits.

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
2. `DatabaseId`, if V1 needs a stable database identity below engine
3. `ReplicaId`, if sync or backend identity requires a lower-layer concept
4. `TxnId`, if storage and engine both need the same transaction identifier
5. `CommitVersion`, if storage and engine both need the same commit/version
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

### Canonical Value Model

Core-next may own the canonical user value model if the architecture keeps one
shared value type across engine, executor, SDKs, and intelligence.

The value model may define:

1. Null.
2. Boolean.
3. Integer.
4. Float.
5. String.
6. Bytes.
7. Array.
8. Object.

The value model may define type equality and simple type accessors.

The value model must not define:

1. JSON path mutation.
2. JSON merge-patch behavior.
3. Search extraction.
4. Embedding extraction.
5. Graph property interpretation.
6. Storage encoding policy.
7. Product validation limits.

Storage-next should not need to inspect `Value` to preserve durability.
Storage-next may store opaque bytes or encoded values; engine-next owns the
meaning of those bytes.

JSON interop is allowed only if it is a pure representation adapter. If it
starts to encode JSON product semantics, it belongs in engine-next or executor.

### Address And Name Vocabulary

Core-next may own address or name newtypes only when they are cross-layer
contracts.

Candidates:

1. `BranchId`
2. `SpaceName`, if storage and engine both need identical namespace validation
3. `DatabaseAddress`, if storage and engine both need a shared parsed address
   syntax
4. `BackendAddress`, if backend selection needs a cross-layer syntax contract

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

Likely core-owned:

1. `BranchId`
2. `CommitVersion`
3. `TxnId`
4. Canonical `Value`, if V1 keeps one shared user value model
5. Minimal timestamp representation
6. Minimal lower-layer version wrappers, only where they are not product
   capability vocabulary

### Keep Only If Proven Cross-Layer

Possible core-owned, but not automatic:

1. `DatabaseId`
2. `ReplicaId`
3. `SpaceName`
4. `DatabaseAddress`
5. `BackendAddress`
6. Branch-name validation, if storage-next and engine-next both require exactly
   the same validated user-facing name
7. Versioned result wrappers, only if storage-next and engine-next explicitly
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
4. Limits and validation policy.
5. JSON path and patch helpers.
6. Event-chain verification.
7. Vector model presets.
8. Search text extraction.
9. Runtime, filesystem, or network helpers.

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
3. Deterministic ID derivation tests where derivation is part of the contract.
4. Timestamp and version boundary tests.
5. Value equality and conversion tests.
6. Public error display/source tests for type-local errors.
7. Compile-time or guard tests proving no Strata-crate dependencies.
8. Public surface snapshot tests or equivalent review guard.

Property tests should cover:

1. ID round trips.
2. Timestamp arithmetic.
3. Value serialization and equality edge cases.
4. Name/address parsing if those types remain in core-next.

Core-next tests must not require filesystem, network, model providers,
background runtimes, or a database instance.

## Open Design Questions

The `core-next` architecture pass must resolve:

1. Does `Value` remain in core-next, or does engine-next own the user value
   model while storage-next stores opaque bytes?
2. Does storage-next need `TxnId` and `CommitVersion` from core-next, or should
   storage-next own storage-local commit identifiers and engine map them into
   product versions?
3. Is `Timestamp` only a representation type, or should the clock source be
   entirely engine-owned?
4. Does backend address syntax belong in core-next, storage-next, or engine-next?
5. Which serialization formats are core contracts versus adapters owned by
   higher layers?

Until these questions are answered, the current core crate should not be copied
forward wholesale.

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

After this document is stable, write:

1. `docs/architecture/storage-next-architecture.md`
2. `docs/architecture/engine-next-architecture.md`
3. `docs/architecture/v1-command-boundary-contract.md`
4. `docs/architecture/v1-error-and-diagnostics-contract.md`

The storage-next document should explicitly state which core-next types it
needs. The engine-next document should explicitly state which current core types
it absorbs as product vocabulary.
