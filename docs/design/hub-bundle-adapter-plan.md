# StrataHub Bundle Adapter: Design + Slice Plan (Ask 3)

Status: accepted plan, 2026-07-10
Scope: StrataHub coordination Ask 3 (export adapter), with the crate laid
out to also absorb Asks 4-6 later.
Contracts: stratahub `docs/coordination/strata-core-requirements-for-stratahub-v1.md`
§3.1-§3.4, §3.8; `docs/architecture/stratahub-v1-bundle-format.md` §3-§5;
strata-core `docs/architecture/engine/dataset-clone-artifact-contract.md`.

## Verified ground truth (2026-07-10)

- `stratahub-protocol` is real and byte-pinned: `Manifest` (+ `BranchEntry`,
  `ManifestObject` with `path`, `EngineCompatibility`), `Hash`
  (`blake3:<64-lowercase-hex>`), JCS canonicalizer behind
  `Manifest::canonical_bytes()`, serde_jcs pinned `=0.2.0`, blake3 `1`,
  JCS/RFC-8785 vector suites, and a hash-anchored worked example
  (`stratahub_testkit::fixtures::titanic_manifest()` →
  `blake3:8ac589d2…fc41175`).
- `stratahub-ingest` is a stub: the M8E2 `Engine` trait exists only in the
  epic spec. We therefore ship the M8E2 *shapes* as inherent API now and add
  the literal `impl Engine for StrataCoreEngine` in a follow-up PR when
  their crate lands the trait. The byte-level contract is the hard part;
  the trait impl is glue.
- Their `PrimitiveType` is `{Kv, Json, Vectors, Events, Branches}`,
  `#[non_exhaustive]` — **no Graph variant**. Flagged cross-repo (see
  "Coordination flags"); until it exists, graph data rides in bundles but
  cannot be advertised as a primitive.

## Decisions

1. **One crate: `crates/hub`, package `strata-hub`.** Holds all hub-facing
   core features: export adapter (Ask 3), `import_bundle` (Ask 4),
   `RemoteTrackingRef` (Ask 5), clone orchestration behind executor
   `hub.*` (Ask 6). Position in the DAG: beside intelligence — imports
   `strata-engine` and stratahub crates; nothing in the workspace imports
   it except executor (Ask 6, later). Engine/storage never import it.
2. **Payloads in engine, packaging in hub.** The engine owns a
   deterministic, versioned, portable serialization of a branch's logical
   content (per data model, ordered row streams + branch control
   metadata) — per the clone-artifact contract, "engine owns artifact
   product semantics." `strata-hub` maps payload streams to transport:
   chunking into objects, `ObjectPath` layout, blake3 hashing, `Manifest`
   construction, JCS bytes. The engine never sees stratahub types.
3. **Logical serialization, never storage files.** Raw storage files
   (WAL segments, tables) depend on commit timing and compaction state
   and can never satisfy the reproducibility fixtures ("run build.py
   twice → byte-identical bundles"). Export bytes are a pure function of
   logical content: key-ordered rows with their commit versions and
   timestamps, per data model, at one MVCC read point (branch head
   pinned at export start; everything read `as_of` that point).
4. **`Manifest.created` derives from content, not the wall clock.** The
   round-trip conformance case (export → import → re-export must produce
   the identical manifest hash) forbids wall-clock fields. `created` =
   the maximum commit timestamp across exported branches, rendered
   RFC 3339 UTC. Flagged cross-repo.
5. **Read-only source via scratch copy (V1).** `Database::open_local`
   writes lock state, and §3.3 invariant 7 says `source_path` MUST NOT
   be mutated. V1: the adapter copies the source directory to a scratch
   dir and opens the copy (explicitly permitted side-effect). A true
   read-only engine open is a later optimization, not a blocker.
6. **Compatibility strings.** `required_engine_version`: `">=1.0.0, <2.0.0"`.
   `capability_registry_version`: `1` = {kv, json, vectors, events,
   branches} (graph joins the registry when their enum can carry it —
   registry versions are supersets, never removals).
7. **Object layout.** `control/branches.json` (branch lineage + heads),
   then `branches/<branch>/<model>/<nnnn>.rows` payload chunks, target
   chunk 64 MB (hard cap 512 MB per bundle-format §3.3; manifest cap
   1 MB ⇒ fewer, larger objects when object count grows). Every path
   obeys the §4.4 path constraints.
8. **`head_commit`** (their `BranchEntry`): blake3 hash of the branch's
   canonical control record — hash-shaped as their docs expect, derived
   purely from logical content.

## Slices

- **HB1 — crate skeleton + `engine_info` + Phase A byte-compat CI.**
  `crates/hub` with git-pinned `stratahub-protocol` (+ testkit as
  dev-dep), serde_jcs `=0.2.0`, blake3 `1`. M8E2 shapes (`EngineInfo`,
  `EngineExportOptions`, `EngineObject`, `AuxiliaryHashes`, error enum)
  as local types. Conformance tests: blake3 anchors, JCS vectors, and
  the titanic manifest anchor reproduced end-to-end through our pins.
  This closes coordination §4 Phase A.
- **HB2 — engine: deterministic branch export payloads (D4).** Engine
  API producing ordered, versioned row streams per data model plus the
  canonical branch control record, at a pinned read point. Proofs:
  byte-determinism across repeated exports and across
  logically-identical databases; MVCC isolation from concurrent writes;
  coverage of all five data models.
- **HB3 — `StrataCoreEngine::open` + `export_bundle`.** Scratch-copy
  open, payload→object chunking, manifest assembly, canonical bytes,
  all seven §3.3 invariants, empty-bundle case, golden manifest fixture
  for a seeded fixture DB. Exit = their §4 Phase B: their `ingest` runs
  end-to-end against a real engine (their side supplies M8E1).
- **HB4 — schema + preview blobs (their Phase C; can lag).**
  `emit_schema_preview` honored: `DatasetSchema` + `SamplePreview`
  blobs, JCS-canonical, truncation convention. Until then the option
  returns `None` blobs (explicitly allowed).
- **HB5 — trait impl PR** when stratahub lands M8E2's `Engine` trait:
  `#[async_trait] impl Engine for StrataCoreEngine` delegating to the
  inherent API (sync internals behind `spawn_blocking`).

Asks 4-6 (import, RemoteTrackingRef, hub.* orchestration) continue in
this crate after HB3; import reuses HB2's payload format in reverse with
staged, all-or-nothing materialization.

## Coordination flags (to raise in the stratahub reply thread)

1. `PrimitiveType` needs a `Graph` variant (non-breaking; enum is
   `#[non_exhaustive]`) — V1 strata ships a graph data model.
2. `Manifest.created` must be content-derived for round-trip hash
   stability (decision 4) — worth a sentence in their bundle-format doc
   so V2 producers don't reintroduce wall-clock drift.
3. The M8E2 `Engine` trait doesn't exist in `stratahub-ingest` yet; we
   ship shape-compatible inherent API and take the trait impl as HB5.
