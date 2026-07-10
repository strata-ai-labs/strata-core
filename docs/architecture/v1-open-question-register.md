# V1 Open Question Register

Status: M0B decision ownership register

## Purpose

This register consolidates load-bearing open questions from the active V1
architecture and product documents.

It does not answer every implementation detail up front. Its job is to make
sure no stable crate construction depends on an unowned decision. Every question
that can affect crate boundaries, durable bytes, product semantics, public APIs,
or conformance gates has an owner and a closure point.

## Closure Rules

1. **Closed** means the V1 baseline is already decided in the architecture
   documents.
2. **Milestone-owned** means the named milestone or epic must close the question
   before its exit gate.
3. **Precondition** means a later milestone may not begin the named work until
   the question is closed.
4. **Post-V1** means the question is intentionally outside V1 implementation.
   V1 may preserve substrate hooks, but must not build the product feature.
5. **Implementation-local** means the milestone may choose exact Rust names or
   private shapes without reopening architecture, as long as the public
   contract and tests match the target docs.

## Register

| ID | Source | Question group | Owner | Closure point | Status |
|---|---|---|---|---|---|
| `V1Q-001` | `strata-v1-implementation-roadmap.md` | Core-next public surface. | `M1B`, `M1D` | Before M1 exit. | Closed baseline: core starts with `BranchId`, `CommitVersion`, timestamp representation, and type-local validation errors. `Value` and `EntityRef` move to engine; transaction/runtime IDs move to storage. |
| `V1Q-002` | Roadmap, storage L3/L5/L6/L7/L9, format spec | Durable storage bytes, including manifest, WAL, table, snapshot, row-key, and commit payload encoding. | `M3C`, `M3TA` | `M3C` must close before `M3D` or `M3E` use durable bytes. | Milestone-owned. Format spec remains draft until M3 golden vectors freeze it. |
| `V1Q-003` | Roadmap, storage target crate shape, storage testing plan | Storage test harness names, feature gates, and manual invocation. | `M2D`, `M2E`, `M2TB`, `M2TD` | Before M3 fault/golden/conformance harnesses depend on the testkit. | Milestone-owned. |
| `V1Q-004` | Roadmap, engine retrieval, intelligence-next | Shared engine `StageOutcome` shape for retrieval, expansion, rerank, RAG, and later Autosearch. | `M6E`, `M6TD` | Before M8 consumes retrieval stage outcomes. | Milestone-owned. |
| `V1Q-005` | Roadmap, cutover plan | Exact cutover PR series, crate renames, dependency cuts, retirement guards, and promotion sequence. | `M10G` | `M10G` closes before `M10B` crate rename work. | Milestone-owned. |
| `V1Q-006` | Storage-next architecture, L2, core-next | Identifier and encoding ownership between core-next and storage-next. | `M1B`, `M1C`, `M3B` | Core atom encodings close in M1; storage object/path encodings close in M3B. | Milestone-owned. |
| `V1Q-007` | Storage L1, storage-next architecture | Backend capability vocabulary, conditional create/update shape, fence token vocabulary, list consistency, range-read requirements, and cache object-name validation. | `M2B`, `M3A`, `M3TE` | Capability vocabulary closes before M3 backend operations are treated as durable substrate. | Milestone-owned. Future object/OpenDAL details remain post-V1 unless explicitly pulled forward. |
| `V1Q-008` | Storage L2 | Object layout details: manifest history, branch/table manifest split, checksum-vs-ID object names, table ID addressing, snapshot ID shape, object-store prefix partitioning, human-readable local names, table generation, temp object scope, and quarantine metadata. | `M3B`, `M3D`, `M3E`, `M3TC` | V1 durable-local object naming decisions close before M3 durable services write durable object names. Object-store prefix partitioning is post-V1 unless explicitly pulled forward. | Milestone-owned. Version-prefix and global quarantine placement are closed baselines. |
| `V1Q-009` | Storage timeline substrate, storage-space registry | Timeline row prefixes and cache-mode timeline exposure. | `M4C`, `M4TC`, `M4E` | Before L9 exposes timeline APIs to engine. | Milestone-owned. |
| `V1Q-010` | Storage L4 | WAL append/chunk model, manifest history, table manifest service split, background sync shape, publish uncertainty health, sidecar corruption handling, durable deletion, and future object-store fencing. | `M3D`, `M3E`, `M3TC`, `M4D` | Local durable semantics close in M3; recovery health effects close in M4D. | Milestone-owned for local/cache V1. Future object-store fencing is post-V1. |
| `V1Q-011` | Storage L5 | Table key shape, row metadata placement, table format version, bloom sidecars, partitioned index retention, async reader shape, and stable table stats. | `M3C`, `M4A`, `M4TA`, `M4E` | Format-affecting decisions close in M3C; runtime/table stats close before M4E. | Milestone-owned. |
| `V1Q-012` | Storage L6 | Branch-aware LSM row-key shape, L7 allocation API, persisted reachability, materialization target, inherited depth, fork-at-history substrate, L8 facts, and branch metrics shape. | `M4B`, `M4C`, `M4D`, `M4E` | Before M4 L9 API is declared complete. | Milestone-owned. |
| `V1Q-013` | Storage L7 | Snapshot-isolation claim, global versus branch-visible versions, version gaps, commit row representation, branch generation guards, transaction pool fate, and L7 metrics. | `M4C`, `M4E`, `M6D` | Storage mechanics close before M4E; product branch-generation expectations close before M6D. | Milestone-owned. Public manual transaction sessions remain removed. |
| `V1Q-014` | Storage L8, storage L9 | Lossy WAL fallback, WAL flush thread ownership, automatic checkpoint policy, storage health surface, maintenance scheduler shape, maintenance controls, and object-name diagnostics. | `M4D`, `M4F`, `M4TE`, `M11A` | Normal V1 open/recovery semantics close in M4; release hardening closes residual recovery risks in M11A. | Milestone-owned. Lossy fallback defaults to diagnostic tooling unless M4 explicitly retains it. |
| `V1Q-015` | Storage-next architecture, runtime profile architecture | Resolved storage budget shape passed through L9. | `M5E`, `M5TD` | M4 ships with explicit storage defaults; M5E wires engine-resolved budgets through L9. | Milestone-owned. |
| `V1Q-016` | Storage-next architecture, storage tests | Storage concurrency testing approach for L7/L8 interleavings. | `M4TC`, `M4TD`, `M11TA` | Before M4 commit/recovery tests are treated as complete. | Milestone-owned. M4 may use the deterministic harness pattern unless implementation proves a standard tool is needed. |
| `V1Q-017` | Storage-next architecture, engineering standards | Which current hardening structs are real contracts versus cleanup-era scaffolding. | `M2A`, `M4A`, `M4D`, `M0C` | Reviewed during module construction and standards alignment. | Implementation-local with standards guard. |
| `V1Q-018` | Engine contracts broadly | Exact Rust type, trait, and module names. | `M0C`, `M5A`, `M6A`, `M10D` | Naming policy closes in M0C; engine module vocabulary closes in M5A/M6A; public residue closes in M10D. | Implementation-local. Use domain names and concept-budget rules from engineering standards. |
| `V1Q-019` | Engine control-plane layout, storage-space registry | Control-plane row keys, branch lineage projection, recipe override merge policy, clone/export derived row coverage, raw health diagnostics, and tags/notes deletion. | `M5D`, `M6E`, `M6G`, `M6H`, `M10D` | Row keys/registry close in M5D; retrieval/clone/removed-surface behavior closes in M6/M10. | Milestone-owned. Authoritative branch lineage is `0x30`; graph-shaped projection is optional derived state under `0x45`. |
| `V1Q-020` | Branch operation contract, branching product direction | Empty branch status, branch-point syntax, promotion default, source-wins naming, copy API split, event divergence policy, graph copy depth, compare/promotion control-plane coverage, archive/delete, comparison pagination, and StrataHub branch metadata. | `M6D`, `M6C`, `M6G`, `M6H`, `M9A`, `M10C`, `M10D` | Product semantics close in M6; StrataHub default-branch remote metadata closes in M9; public syntax and cleanup close in M10. | Milestone-owned. Empty branch creation is required; strict conflict handling is the V1 promotion default; event source-wins refuses divergent appends; delete is destructive for V1; multi-branch Hub collaboration is post-V1. |
| `V1Q-021` | EntityRef and graph relationship docs | EntityRef URI grammar, cross-branch refs, direct relationship commands, JSON path refs, event identity, edge identity, and control-plane/fleet refs. | `M5C`, `M6C`, `M6D`, `M10C` | Structured identity closes in M5/M6; public syntax closes in M10. | Milestone-owned. Cross-branch relationship targets are post-V1. |
| `V1Q-022` | Error contracts | Public error type name, compatibility wrapper fate, stable detail keys, source-chain summaries, health output, CLI rendering, trace ID preservation, numeric code IDs. | `M5F`, `M6F`, `M9C`, `M10C`, `M11E5` | Error mapping closes in M5/M6; StrataHub clone/info errors close in M9; CLI/wire display closes in M10; final registry audit closes in M11. | Milestone-owned. Idempotency keys are not V1; ambiguous commit retry is `unknown`. |
| `V1Q-023` | IPC contract | IPC protocol version, wire format, Windows local IPC primitive, `database()` escape hatch, server identity exposure, and read-only owner behavior. | `M6F`, `M10C`, `M10TB` | Command semantics close in M6; transport/CLI behavior closes in M10. | Milestone-owned. `strata up` owns a writable handle for V1; read-only IPC owner is deferred. |
| `V1Q-024` | Persistence adapter, storage L9 | Engine adapter storage-key shape, commit builder shape, control-plane keys, timeline product policy, derived-state validation depth, and temporary cutover shims. | `M5B`, `M5D`, `M6D`, `M6E`, `M10A`, `M10TE` | Persistence seam closes in M5; product policy closes in M6; temporary shims close in M10. | Milestone-owned. |
| `V1Q-025` | Retrieval and derived-state contract | Public constrained discovery/find surface, filter syntax, built-in recipes, derived row versus sidecar implementation, cache/browser stage availability, and AutoResearch metrics. | `M6E`, `M8D`, `M10C` | Retrieval substrate closes in M6E; model-assisted stages close in M8; public CLI/API syntax closes in M10. | Milestone-owned for retrieval substrate. Public constrained discovery and AutoResearch optimizer are post-V1 unless pulled forward. |
| `V1Q-026` | Temporal contract, time-travel product direction | Temporal type names, CLI selectors, retained-history defaults, approximate historical search policy, event-domain time filtering, pinned historical relationship refs, cache-mode timeline metadata. | `M4C`, `M6D`, `M6E`, `M10C` | Storage substrate closes in M4; product semantics close in M6; CLI syntax closes in M10. | Milestone-owned. |
| `V1Q-027` | Engine testing plan | Engine testkit location, CLI output fixture stability, optional retrieval/model pathways requiring conformance, fake-provider test placement, characterization-test deletion, and stable error detail fixtures. | `M5A`, `M6TA`, `M8TA`, `M9TF`, `M10TA`, `M11E` | Testkit closes in M5; product/fake-provider coverage closes in M6/M8/M9/M10; release fixture audit closes in M11. | Milestone-owned. |
| `V1Q-028` | Dataset clone artifact, StrataHub direction | `.strata` container, manifest encoding, checksums/signing, compression, encryption, artifact scope, derived-state inclusion, fetch schemes, URL ownership, license/trust/PII metadata, reader reuse, backup subtype. | `M6G`, `M9A`, `M9B`, `M9C`, `M9TF`, `M10C`, `M10TF`, `M11E` | Clone substrate closes in M6G; Hub protocol, fetch, and CLI clone surface close in M9; cutover closes public CLI integration in M10; release audit closes in M11. | Milestone-owned. Encryption/signing beyond checksums are deferred unless M6G or M9 explicitly pulls them into V1. |
| `V1Q-029` | Runtime resource profile architecture | Numeric profile defaults, profile names, host facts, explicit pinning, organization caps, derived-state budget split, minimum durable device class, future fleet facts. | `M5E`, `M5TD`, `M11D` | First deterministic policy closes in M5E; benchmark-derived threshold tuning closes in M11D. | Milestone-owned. Fleet reporting details are post-V1. |
| `V1Q-030` | Inference-next, M7 plan | llama.cpp unsafe audit completion. | `M7E`, `M7TE` | Before local llama.cpp runtime is V1-ready. | Milestone-owned. Placeholder audit exists at `docs/audits/llama-ffi-unsafe-audit.md`. |
| `V1Q-031` | Inference-next | Cloud ranking, streaming generation, and external on-prem model endpoint adapters. | Post-V1 unless product explicitly pulls forward | No V1 milestone may depend on them. | Closed deferral. V1 reserves extension points only. |
| `V1Q-032` | Intelligence-next | Public stage diagnostics versus trace context, first Autosearch substrate, and post-V1 endpoint capability schema. | `M6E`, `M8E`, Post-V1 | Stage diagnostics close in M6/M8; endpoint capability schema is post-V1. | Milestone-owned plus post-V1 deferral. |
| `V1Q-033` | Graph relationship product direction | Public entity-ref endpoints, deletion default, missing-reference behavior, ontology depth, relationship repair, and derived relationship labels. | `M6C`, `M6E`, `M10C`, `M11B` | Product semantics close in M6; public syntax closes in M10; hardening closes in M11. | Milestone-owned. |
| `V1Q-034` | StrataHub substrate and product direction | Hub artifact family, hashes/signatures, local provenance metadata, clone identity, sync and auto-sync workflow, credential stores, fleet metadata, provider capability expression, and Hub-client conformance fixtures. | `M6G`, `M9A`, `M9B`, `M9D`, `M9TF`, Post-V1 | V1 clone/provenance substrate closes in M6; Hub clone/info protocol and deterministic client conformance fixtures close in M9. Push, auth, sync, auto-sync, hosted runtime, Hub catalog hosting, publishing, and fleet management are post-V1 or StrataHub-owned. | Milestone-owned plus post-V1 deferral. |
| `V1Q-035` | Autosearch product direction | Evaluation-set placement, golden answer shape, metric set, planner scope, cleanup, fixed-version runs, reports, recipe sharing, and device budgets. | Post-V1 substrate owner: `M6E`, `M8E` | V1 may preserve recipe/provenance substrate only. | Post-V1. |
| `V1Q-036` | Storage format spec | `mutation_count` exact integer width and other draft storage-format placeholders. | `M3C`, `M3TA` | Before storage format spec is promoted from draft. | Milestone-owned. |
| `V1Q-037` | Storage format spec, L3 | AES-GCM/encryption productization. | Post-V1 unless separately pulled forward; V1 format owner `M3C` | M3 must document identity codec as required stable V1 support. | Closed deferral for required V1. Optional code may exist only behind explicit feature/support policy. |

## Source Coverage

This table maps active open-question sections, unresolved/TBD lines, and source
documents whose questions were closed by this register. It is the M0B
verification aid: new active open-question sections must add a row here or
close the question in place.

| Source document | Covered by |
|---|---|
| `docs/architecture/strata-v1-implementation-roadmap.md` | `V1Q-001` through `V1Q-005` |
| `docs/architecture/storage-architecture.md` | `V1Q-006`, `V1Q-007`, `V1Q-015`, `V1Q-016`, `V1Q-017` |
| `docs/architecture/storage/l1-backend-io.md` | `V1Q-007` |
| `docs/architecture/storage/l2-object-layout.md` | `V1Q-006`, `V1Q-008` |
| `docs/architecture/storage/commit-timeline-substrate.md` | `V1Q-009` |
| `docs/architecture/storage/storage-space-id-registry.md` | `V1Q-009` |
| `docs/architecture/storage/l4-log-manifest-snapshot-services.md` | `V1Q-010`, `V1Q-014` |
| `docs/architecture/storage/l5-table-runtime.md` | `V1Q-011` |
| `docs/architecture/storage/l6-branch-isolated-lsm-runtime.md` | `V1Q-012` |
| `docs/architecture/storage/l7-commit-runtime.md` | `V1Q-013`, `V1Q-016` |
| `docs/architecture/storage/l8-lifecycle-recovery-maintenance.md` | `V1Q-014` |
| `docs/architecture/storage/l9-storage-api-boundary.md` | `V1Q-010`, `V1Q-014`, `V1Q-024`, `V1Q-026` |
| `docs/spec/strata-storage-format-v1.md` | `V1Q-002`, `V1Q-036`, `V1Q-037` |
| `docs/architecture/engine/control-plane-layout-contract.md` | `V1Q-019` |
| `docs/architecture/engine/storage-space-id-registry.md` | `V1Q-019` |
| `docs/architecture/engine/branch-operation-and-capability-adapter-contract.md` | `V1Q-020` |
| `docs/architecture/engine/entity-ref-and-relationship-layer-contract.md` | `V1Q-021`, `V1Q-033` |
| `docs/architecture/engine/error-and-diagnostics-contract.md` | `V1Q-022` |
| `docs/architecture/v1-error-and-diagnostics-contract.md` | `V1Q-022` |
| `docs/architecture/engine/ipc-and-command-boundary-contract.md` | `V1Q-023` |
| `docs/architecture/engine/persistence-adapter-contract.md` | `V1Q-024` |
| `docs/architecture/engine/retrieval-and-derived-state-contract.md` | `V1Q-025`, `V1Q-032` |
| `docs/architecture/engine/temporal-context-and-timeline-resolver-contract.md` | `V1Q-026` |
| `docs/architecture/engine/testing-and-conformance-plan.md` | `V1Q-027` |
| `docs/architecture/engine/dataset-clone-artifact-contract.md` | `V1Q-028`, `V1Q-034` |
| `docs/architecture/runtime-resource-profile-architecture.md` | `V1Q-029` |
| `docs/architecture/inference-architecture.md` | `V1Q-030`, `V1Q-031` |
| `docs/architecture/intelligence-architecture.md` | `V1Q-004`, `V1Q-025`, `V1Q-032`, `V1Q-035` |
| `docs/architecture/stratahub-substrate-architecture.md` | `V1Q-034` |
| `docs/stratahub/docs/product/stratahub-user-pathways.md` | `V1Q-028`, `V1Q-034` |
| `docs/stratahub/docs/product/stratahub-v1-cli-commands.md` | `V1Q-022`, `V1Q-028`, `V1Q-034` |
| `docs/product/strata-v1-branching-direction.md` | `V1Q-020`, `V1Q-026` |
| `docs/product/strata-v1-graph-relationship-layer.md` | `V1Q-021`, `V1Q-033` |
| `docs/product/strata-v1-versioning-time-travel.md` | `V1Q-026` |
| `docs/product/stratahub-product-direction.md` | `V1Q-034` |
| `docs/product/strata-autosearch-product-direction.md` | `V1Q-035` |

## Milestone Inputs

Each milestone should start by filtering this register for its owner code:

1. `M0`: `V1Q-018`.
2. `M1`: `V1Q-001`, `V1Q-006`.
3. `M2`: `V1Q-003`, `V1Q-007`, `V1Q-017`.
4. `M3`: `V1Q-002`, `V1Q-007`, `V1Q-008`, `V1Q-010`, `V1Q-011`, `V1Q-036`,
   `V1Q-037`.
5. `M4`: `V1Q-009`, `V1Q-011`, `V1Q-012`, `V1Q-013`, `V1Q-014`,
   `V1Q-016`, `V1Q-026`.
6. `M5`: `V1Q-015`, `V1Q-018`, `V1Q-019`, `V1Q-021`, `V1Q-022`,
   `V1Q-024`, `V1Q-027`, `V1Q-029`.
7. `M6`: `V1Q-004`, `V1Q-013`, `V1Q-018`, `V1Q-019`, `V1Q-020`,
   `V1Q-021`, `V1Q-022`, `V1Q-023`, `V1Q-024`, `V1Q-025`, `V1Q-026`,
   `V1Q-028`, `V1Q-032`, `V1Q-033`, `V1Q-034`, `V1Q-035`.
8. `M7`: `V1Q-030`, `V1Q-031`.
9. `M8`: `V1Q-004`, `V1Q-025`, `V1Q-032`, `V1Q-035`.
10. `M9`: `V1Q-020`, `V1Q-022`, `V1Q-027`, `V1Q-028`, `V1Q-034`.
11. `M10`: `V1Q-005`, `V1Q-018`, `V1Q-020`, `V1Q-021`, `V1Q-022`,
    `V1Q-023`, `V1Q-024`, `V1Q-025`, `V1Q-026`, `V1Q-027`, `V1Q-028`,
    `V1Q-033`.
12. `M11`: `V1Q-014`, `V1Q-016`, `V1Q-022`, `V1Q-027`, `V1Q-028`,
    `V1Q-029`, `V1Q-033`.

## M0B Closure

M0B is closed when:

1. Every active V1 open-question section maps to at least one `V1Q-*` row.
2. Every `V1Q-*` row has an owner and closure point.
3. The roadmap points to this register instead of carrying unowned
   "remaining" items.
4. Placeholder documents remain allowed only when their owner milestone is
   listed in this register or in `docs/architecture/v1-document-inventory.md`.

At M0B capture time, no load-bearing V1 architecture question is unowned.
