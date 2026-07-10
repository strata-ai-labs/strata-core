# Engine-Next Public API And CLI Surface Cleanup Checklist

Status: V1 architecture draft

## Purpose

This document defines the public API and CLI cleanup required before
engine-next becomes the V1 product surface.

The goal is to keep Strata's public surface aligned with the product model:

```text
open -> store/relate/search/branch/version/clone -> inspect -> recover
```

The current executor and CLI are evidence. They are not the product truth.
Development-era commands should not survive merely because they compile.

The API and CLI layer is also Strata's integration boundary. The real leverage
of this layer is not only direct human use; it is what other systems can safely
build on top of it: MCP servers, LangGraph integrations, ORMs, notebook
connectors, Codex and Claude Code plugins, agent sandboxes, CI jobs, private
company tools, and future Strata AI workflows. Those integrations need a small,
stable, serializable product surface. They must not need storage access, raw
engine handles, or historical commands to do useful work.

## Related Documents

Read this with:

1. `docs/product/strata-v1-product-requirements.md`
2. `docs/product/strata-v1-feature-inventory.md`
3. `docs/product/strata-v1-user-pathways.md`
4. `docs/product/pathways/runtime-and-portability.md`
5. `docs/product/pathways/operations-and-interfaces.md`
6. `docs/architecture/engine-architecture.md`
7. `docs/architecture/engine/ipc-and-command-boundary-contract.md`
8. `docs/architecture/engine/dataset-clone-artifact-contract.md`
9. `docs/architecture/engine/branch-operation-and-capability-adapter-contract.md`
10. `docs/architecture/engine/temporal-context-and-timeline-resolver-contract.md`
11. `docs/architecture/engine/retrieval-and-derived-state-contract.md`

## Current Code Evidence

Current public or quasi-public surfaces include:

1. SDK handle in `crates/executor/src/compat.rs`.
   `Strata::open`, `Strata::open_with`, `Strata::cache`, `new_handle`,
   `session`, `database`, `is_ipc`, branch helpers, KV helpers, search helpers,
   `flush`, and `compact`.

2. Serializable command boundary in `crates/executor/src/command.rs`.
   This contains user-facing commands, IPC commands, internal compatibility
   commands, maintenance commands, branch-bundle commands, transaction commands,
   tags, notes, graph, vector, search, config, model, generation, and import/export
   variants.

3. CLI parser in `crates/cli/src/parse.rs`.
   Current top-level commands include `ping`, `info`, `health`, `metrics`,
   `flush`, `compact`, `describe`, `durability-counters`, `kv`, `json`, `event`,
   `vector`, `graph`, `branch`, `space`, `begin`, `commit`, `rollback`, `txn`,
   `search`, `config`, `recipe`, `configure-model`, `embed`, `models`,
   `generate`, `tokenize`, `detokenize`, `export`, and `import`.

4. CLI admin commands in `crates/cli/src/admin.rs`.
   `strata up` and `strata down` manage same-machine IPC.

5. Product open options in `crates/engine/src/database/open_options.rs`.
   `OpenOptions` currently exposes `access_mode`, `follower`, and
   `default_branch`.

6. Engine and executor re-exports in `crates/engine/src/lib.rs` and
   `crates/executor/src/lib.rs`.
   Some exports are product types; others expose implementation details,
   storage details, or workspace-internal engine construction.

## Requirement Language

1. **Keep** means the surface is part of V1 if implementation and tests satisfy
   the referenced contract.
2. **Keep with redesign** means the capability is V1-aligned but the current
   name, command shape, error model, or output shape should change.
3. **Optional** means allowed for V1 only when feature-gated, documented, and
   honest about missing runtime support.
4. **Admin/diagnostic** means not part of normal user workflows.
5. **Remove** means remove from default public V1 API and CLI.
6. **Internal** means engine/storage may keep machinery, but public users should
   not see or manage it.

## Product Surface Rule

A public API, CLI command, output field, or re-export earns V1 status only if it
serves one of the documented product pathways.

The default answer for historical surface area is removal, hiding, or redesign.
Compatibility with pre-V1 internal APIs is not a product requirement.

## Integration Boundary Rule

Treat the API, CLI, IPC, and serializable command boundary as one integration
contract.

V1 integrations should be able to depend on:

1. Stable command names for product operations.
2. Stable JSON input and output shapes for automation.
3. Stable error codes and retry/action guidance.
4. Explicit read/write classification.
5. Explicit feature availability and unsupported-feature errors.
6. Bounded inspection through `describe`, `health`, `metrics`, and related
   diagnostics.
7. Stable branch, space, version, time, search, clone, and data-capability
   semantics.

V1 integrations should not depend on:

1. Direct storage access.
2. Raw engine `Database` handles.
3. Storage iterators, WAL records, manifests, tables, checkpoints, or compaction
   internals.
4. Follower mode.
5. Public transaction sessions.
6. Legacy branch bundles.
7. Tags and notes.
8. Debug strings as machine-readable data.
9. Hidden network behavior.
10. Commands that exist only for current tests or historical implementation
    convenience.

The command boundary should be designed like a protocol. If MCP, LangGraph,
ORMs, Codex plugins, Claude Code plugins, notebooks, or CI tools cannot use the
public surface without reaching around it, the public surface is incomplete.

## V1 Surface Summary

| Area | V1 decision | Current surface | Cleanup action |
| --- | --- | --- | --- |
| Durable open | Keep | `Strata::open`, `Strata::open_with`, CLI `--db` | Keep; remove follower from open options. |
| Cache open | Keep | `Strata::cache`, CLI `--cache` | Keep as explicitly ephemeral only. |
| Read-only open | Keep | `AccessMode::ReadOnly`, CLI `--read-only` | Keep; test command write classification. |
| IPC local sharing | Keep | `strata up`, `strata down`, IPC fallback | Keep same-machine only; remove follower guidance. |
| Follower mode | Remove | `OpenOptions::follower`, CLI `--follower`, follower refresh | Remove from public V1 surface. |
| KV | Keep | `Kv*`, `Strata::kv_*` subset | Keep; align outputs and temporal behavior. |
| JSON | Keep | `Json*` | Keep core document operations; review secondary-index surface. |
| Events | Keep | `Event*` | Keep append/query surface; document immutability. |
| Vector | Keep | `Vector*` | Keep; optional indexes must be capability-gated. |
| Graph | Keep with redesign | `Graph*` | Keep graph/relationship model; align naming with relationship contract. |
| Graph analytics | Optional | `GraphWcc`, `GraphPagerank`, etc. | Keep only if bounded and tested; otherwise feature-gate. |
| Spaces | Keep | `Space*`, `set_space` | Keep; define delete safety. |
| Branch workspaces | Keep with redesign | create/list/delete/fork/diff/merge/revert/cherry-pick | Keep capability; prefer product wording. |
| Tags and notes | Remove | `Tag*`, `Note*` | Remove; future dataset releases/provenance may replace. |
| Version/history/time | Keep | `Getv`, `as_of`, `TimeRange` | Keep; add branch-from-time surface. |
| Public transactions | Remove | `begin`, `commit`, `rollback`, `txn`, `Txn*`, `Session` txn state | Remove default public workflow; keep internal commit machinery. |
| Primitive import/export | Optional | `DbExport`, `ArrowImport`, CLI `export`/`import` | Keep only with explicit scope and tests. |
| Legacy branch bundles | Remove | `BranchExport`, `BranchImport`, `BranchBundleValidate` | Replace with `.strata` clone artifact. |
| Dataset clone | Keep | Not implemented as current command | Add `strata clone <source> <destination>`. |
| Health/inspection | Keep | `ping`, `info`, `describe`, `health`, `metrics`, counters | Keep bounded, privacy-aware outputs. |
| Flush/compact/checkpoint | Remove or admin | `flush`, `compact`, SDK methods | Remove from normal user surface; possible hidden diagnostics. |
| Retention commands | Remove or diagnostic | `RetentionApply`, `RetentionStats`, `RetentionPreview` | Do not expose as normal workflows. |
| Config | Keep with cleanup | `config`, `ConfigureSet`, `ConfigureGetKey` | Keep safe config; remove dead modes. |
| Recipes | Keep | `Recipe*` | Keep for retrieval. |
| Auto-embedding | Optional | `ConfigSetAutoEmbed`, `EmbedStatus`, `ReindexEmbeddings` | Keep as explicit, observable, repairable. |
| Model/generation utilities | Optional | `models`, `embed`, `generate`, tokenize/detokenize | Feature-gate; no hidden provider/network calls. |
| Raw engine handle | Remove/redesign | `Strata::database()` | Remove from normal SDK; expose only test/internal escape hatch if needed. |
| Low-level re-exports | Remove/redesign | storage and engine types re-exported by executor | Keep only product DTOs and errors. |

## Required Public V1 Surface

### Open And Runtime

Keep:

1. `Strata::open(path)`.
2. `Strata::open_with(path, options)` after `OpenOptions` is cleaned.
3. `Strata::cache()`.
4. CLI `--db`.
5. CLI `--cache`.
6. CLI `--read-only`.
7. `AccessMode::ReadWrite`.
8. `AccessMode::ReadOnly`.
9. `Strata::close`.
10. `Strata::new_handle` if behavior is consistent across local and IPC.
11. `Strata::is_ipc` or a renamed handle-kind query.

Cleanup:

1. Remove `OpenOptions::follower`.
2. Remove CLI `--follower`.
3. Remove product docs and error hints that recommend follower mode.
4. Keep `default_branch` only if it remains a product-level open option.
   Otherwise move default-branch bootstrapping into config or engine policy.
5. Ensure cache means ephemeral. Do not reintroduce disk-backed cache.

### Local IPC

Keep:

1. `strata up`.
2. `strata down`.
3. Local IPC fallback when a primary process owns the database and an IPC socket
   exists.
4. Structured IPC request/response boundary.
5. Access-mode enforcement for IPC clients.

Cleanup:

1. Keep IPC same-machine only.
2. Do not introduce TCP server mode.
3. Do not make IPC required for normal embedded use.
4. `strata up` and `strata down` should reject unrelated global flags.
5. Locked-without-socket errors should tell the user to start IPC if they want
   shared same-machine access, not to use follower mode.
6. Replace the current locked-without-socket message that mentions follower
   mode with a V1 message that points only to `strata up` for same-machine
   sharing.

### Data Capabilities

Keep as required:

1. KV record commands.
2. JSON document commands.
3. Event append/query commands.
4. Vector collection and vector query commands.
5. Graph relationship, traversal, ontology, and basic graph management
   commands.
6. Spaces.

Cleanup:

1. Product docs should say data capability, not primitive, where possible.
2. CLI help should describe what the user can do, not implementation shape.
3. Command outputs should be stable, bounded, and structured.
4. Time-travel fields must use the temporal resolver contract.
5. Batch command atomicity must be documented per command.
6. JSON secondary index commands need a product decision before V1. They should
   not accidentally become a general query-engine promise.

### Branching And Time Travel

Keep:

1. Branch create.
2. Branch list/info/exists.
3. Branch delete with safety checks.
4. Branch from current state.
5. Branch from retained version.
6. Branch from retained timestamp.
7. Compare/diff.
8. Promote/merge with explicit conflict strategy.
9. Copy selected records or changes.
10. Restore/revert by writing compensating changes.
11. Record history through `getv`-style APIs.
12. `as_of` reads.
13. Time range/timeline inspection.

Cleanup:

1. Current `fork`, `merge`, `cherry-pick`, and `revert` can remain as aliases or
   advanced API names, but CLI/help should lead with Strata product language:
   create branch, compare, promote, copy, restore.
2. `merge-base` and `diff3` are useful implementation/power tools, but should
   not be top-level V1 mental-model requirements unless the branching contract
   explicitly keeps them.
3. Branch output must surface retained-history limits and conflict facts.
4. Tags and notes are removed from V1 branch requirements.
5. Branch bundles are removed from V1 branch requirements.

### Search, Retrieval, Recipes, And Intelligence

Keep:

1. `search`.
2. Named recipes.
3. Inline recipes where supported.
4. Search result stats and provenance.
5. Auto-embedding status and explicit reindex/repair where supported.
6. Optional model management.
7. Optional text generation, tokenization, detokenization, query expansion,
   reranking, and RAG utilities.

Cleanup:

1. Any model/provider/network call must be explicit.
2. Generation commands must not appear required for database correctness.
3. Missing model/runtime support must be a normal unsupported-feature error.
4. Auto-embedding must be observable and repairable.
5. Search recipes are the tuning boundary; avoid adding one-off command variants
   for each retrieval experiment.

### Clone And Data Movement

Keep/add:

1. `strata clone <source> <destination>`.
2. SDK clone API.
3. `.strata` artifact validation.
4. Optional primitive import/export where reliable.
5. Export formats such as JSON, JSONL, CSV, Parquet, or Arrow where supported.

Cleanup:

1. Remove `branch export`, `branch import`, and `branch validate` as normal V1
   product commands.
2. If a manual validation command remains, it should validate `.strata`
   artifacts, not legacy branch bundles.
3. Import/export must be explicitly scoped by branch, space, data capability,
   and format.
4. Clone must create a normal database and not become a remote runtime mode.

### Inspection And Diagnostics

Keep:

1. `ping`.
2. `info`.
3. `describe`.
4. `health`.
5. `metrics`.
6. `durability-counters`.
7. Structured JSON output.
8. Stable error categories.

Cleanup:

1. Inspection must be bounded and privacy-aware.
2. `describe` should not leak secrets or unbounded record contents.
3. Metrics should report unsupported backend facts honestly.
4. Durability counters should clearly distinguish cache mode from durable mode.
5. Health should explain automatic maintenance state without telling users to
   manually drive normal maintenance.

## Remove Or Redesign Checklist

### Follower Mode

Current surface:

1. `OpenOptions::follower`.
2. CLI `--follower`.
3. Follower-oriented open/spec APIs below product open.
4. Follower refresh and persisted follower state.
5. User-facing messages that recommend follower mode.

V1 decision: Remove from public product surface.

Replacement:

1. IPC for same-machine shared access.
2. Read-only open for inspection when no primary owner conflict exists.
3. Future sync/clone for dataset movement, not follower refresh.

Checklist:

1. Remove CLI `--follower`.
2. Remove `OpenOptions::follower`.
3. Remove or hide follower command examples.
4. Remove follower from `DescribeResult` or rename to a handle/open mode that
   matches V1.
5. Remove follower hints from lock-conflict errors.
6. Delete or quarantine follower tests when implementation is excised.
7. Keep any lower-level recovery lessons only if they become normal recovery
   tests.

### Public Transaction Commands

Current surface:

1. CLI `begin`.
2. CLI `commit`.
3. CLI `rollback`.
4. CLI `txn info`.
5. CLI `txn active`.
6. `Command::TxnBegin`.
7. `Command::TxnCommit`.
8. `Command::TxnRollback`.
9. `Command::TxnInfo`.
10. `Command::TxnIsActive`.
11. `Session` transaction state.

V1 decision: Remove from default public product surface.

Replacement:

1. Individual writes have clear commit boundaries.
2. Batch APIs declare atomicity and failure behavior.
3. Engine/storage retain internal commit machinery.
4. A future public multi-command transaction API requires a separate ACID,
   backend, isolation, and test plan.

Checklist:

1. Remove top-level CLI transaction commands.
2. Remove transaction commands from default command docs.
3. Remove transaction state from normal session UX.
4. Keep internal transaction/commit types below product APIs.
5. Verify IPC no longer depends on public transaction state once removed.
6. Rewrite tests that used public transactions only as a convenience.

### Legacy Branch Bundles

Current surface:

1. `Command::BranchExport`.
2. `Command::BranchImport`.
3. `Command::BranchBundleValidate`.
4. CLI `branch export`.
5. CLI `branch import`.
6. CLI `branch validate`.
7. Engine `bundle/` module and `.branchbundle.tar.zst` format.

V1 decision: Remove branch-bundle commands and compatibility claims. The V1
data-movement product path is `.strata` clone artifacts.

Replacement:

1. `strata clone <source> <destination>`.
2. `.strata` artifact validation.
3. Dataset/database export where designed.
4. Optional primitive import/export for interoperability.

Checklist:

1. Remove branch-bundle CLI commands from default help.
2. Remove branch-bundle command variants from V1 serializable boundary.
3. Preserve useful tests as artifact-validation/export/import tests if
   applicable.
4. Do not let `.branchbundle.tar.zst` become a V1 compatibility obligation.
5. Make clone failure and partial-write behavior follow the artifact contract.

### Tags And Notes

Current surface:

1. `Command::TagCreate`.
2. `Command::TagDelete`.
3. `Command::TagList`.
4. `Command::TagResolve`.
5. `Command::NoteAdd`.
6. `Command::NoteGet`.
7. `Command::NoteDelete`.
8. Engine branch tag/note types and re-exports.

V1 decision: Remove.

Replacement:

1. Dataset release metadata later.
2. Provenance metadata later.
3. Branch timeline and history inspection for V1.

Checklist:

1. Remove tag/note command variants.
2. Remove tag/note public output types.
3. Remove tag/note CLI docs if any are added.
4. Keep no hidden dependency on tag/note rows in branch operations.
5. If future release labels return, design them as dataset/provenance metadata,
   not Git-style branch notes by default.

### Manual Durability Maintenance

Current surface:

1. CLI `flush`.
2. CLI `compact`.
3. `Strata::flush`.
4. `Strata::compact`.
5. `Command::Flush`.
6. `Command::Compact`.
7. `Command::RetentionApply`.
8. `Command::RetentionStats`.
9. `Command::RetentionPreview`.

V1 decision: Remove from normal user workflows. Possibly keep as hidden
admin/diagnostic commands if there is a concrete support need.

Replacement:

1. Automatic lifecycle behavior.
2. Health and metrics.
3. Durability counters.
4. Recovery diagnostics.
5. Explicit repair flows where user action is actually required.

Checklist:

1. Remove `flush` and `compact` from normal CLI help.
2. Remove `Strata::flush` and `Strata::compact` from the normal SDK.
3. If retained, put under an explicit admin/debug namespace.
4. Do not require manual maintenance in docs or examples.
5. Retention commands must either become bounded diagnostics or disappear.
6. Checkpoint and snapshot controls must remain storage/engine lifecycle
   behavior unless a future support workflow requires exposure.

### Disk-Backed Cache

Current surface:

1. Historical product language and tests may still mention disk-backed cache.
2. Cache mode itself remains as `Strata::cache` and CLI `--cache`.

V1 decision: Cache is ephemeral. Disk-backed cache is not a mode.

Checklist:

1. Remove disk-backed cache language.
2. Ensure cache open does not create WAL, manifest, checkpoints, or durable
   files.
3. Ensure docs distinguish cache mode from `standard` and `always` durability.
4. Do not add hidden temporary-on-disk semantics under the cache name.

### Raw Engine And Storage Leakage

Current surface:

1. `Strata::database()` returns `Arc<Database>` and panics for IPC handles.
2. `strata_executor` re-exports many `strata_engine` and `strata_storage`
   implementation types.
3. `strata_engine` is workspace-internal but exposes broad modules and
   subsystem/runtime details.

V1 decision: Public users should consume the product SDK and command boundary,
not engine/storage internals.

Checklist:

1. Replace `Strata::database()` with a test-only, feature-gated escape hatch.
   It should not be available in the normal SDK surface.
2. Do not expose APIs that work for local handles but panic for IPC handles.
3. Trim executor re-exports to product DTOs, config, access mode, errors, and
   supported data-capability types.
4. Do not re-export storage iterators, recovery internals, WAL counters beyond
   product diagnostics, subsystem types, storage config internals, or raw
   engine runtime hooks unless explicitly documented.
5. Application docs should point to `strata_executor::Strata` or its successor,
   not `strata_engine::Database`.

### Model And Generation Surface

Current surface:

1. `ConfigureModel`.
2. `ConfigureSet` provider/model keys.
3. `Embed`, `EmbedBatch`.
4. `ModelsList`, `ModelsLocal`, `ModelsPull`.
5. `Generate`, `GenerateUnload`.
6. `Tokenize`, `Detokenize`.
7. CLI `configure-model`, `embed`, `models`, `generate`, `tokenize`,
   `detokenize`.

V1 decision: Optional, explicit, feature-gated where appropriate.

Checklist:

1. No hidden provider calls.
2. No hidden network use.
3. No secret leakage in config, describe, metrics, or errors.
4. Missing models produce unsupported-feature or not-configured errors.
5. Model pull is explicitly networked and policy-aware.
6. Generation utilities are not required for core database correctness.

### Query And JSON Index Surface

Current surface:

1. `JsonCreateIndex`.
2. `JsonDropIndex`.
3. `JsonListIndexes`.
4. Retrieval/search commands.

V1 decision: Do not accidentally promise a general query engine.

Checklist:

1. Keep retrieval/search as the primary V1 discovery surface.
2. Treat JSON secondary indexes as capability-specific support until a query
   product contract exists.
3. Do not describe Strata as SQL-like, Redis Query-like, or a general DSL unless
   that product is designed and tested.

## CLI Shape Checklist

Default CLI should include:

1. `strata init`.
2. `strata up`.
3. `strata down`.
4. `strata clone`.
5. `strata ping`.
6. `strata info`.
7. `strata describe`.
8. `strata health`.
9. `strata metrics`.

`strata init` creates a new durable local database at the requested path and
initializes required V1 control-plane metadata. It does not start IPC, sync to a
hub, or create a cache database unless explicitly extended by a later product
contract.
10. `strata durability-counters`.
11. `strata kv ...`.
12. `strata json ...`.
13. `strata event ...`.
14. `strata vector ...`.
15. `strata graph ...`.
16. `strata branch ...`.
17. `strata space ...`.
18. `strata search ...`.
19. `strata recipe ...`.
20. `strata config ...`.
21. Optional `strata models ...`.
22. Optional `strata generate ...`.
23. Optional `strata embed ...`.
24. Optional `strata export ...`.
25. Optional `strata import ...`.

Default CLI should not include:

1. `--follower`.
2. `begin`.
3. `commit`.
4. `rollback`.
5. `txn`.
6. `branch export`.
7. `branch import`.
8. `branch validate` for legacy branch bundles.
9. Normal-user `flush`.
10. Normal-user `compact`.
11. Normal-user retention apply.
12. Public tag/note commands.

Admin/debug namespace may include:

1. Hidden lifecycle controls if support requires them.
2. Artifact validation if useful for CI or support.
3. Repair/rebuild commands with explicit safety wording.
4. Fault or test hooks only in test builds.

## SDK Shape Checklist

Default SDK should expose:

1. Open/cache/read-only options.
2. Close/drop behavior.
3. Handle-kind inspection.
4. KV API.
5. JSON API.
6. Event API.
7. Vector API.
8. Graph/relationship API.
9. Branch API.
10. Space API.
11. Search/recipe API.
12. Clone API.
13. Import/export API where supported.
14. Info/describe/health/metrics/counters API.
15. Config API with redaction.
16. Optional intelligence API.

Default SDK should not expose:

1. Follower mode.
2. Public begin/commit/rollback sessions.
3. Branch-bundle APIs.
4. Tags/notes.
5. Normal-user flush/compact/checkpoint/retention controls.
6. Raw storage handles.
7. Raw engine `Database` handles as the ordinary path.
8. APIs that panic depending on local vs IPC backend.

## Serializable Command Boundary Checklist

The command boundary serves CLI, IPC, SDK automation, and agents. It should be
stable and product-shaped.

Keep:

1. Commands needed by V1 pathways.
2. Read/write classification for every command.
3. Structured inputs and outputs.
4. Feature availability errors.
5. Redaction-safe config and diagnostics.

Remove or hide:

1. Follower-only commands or fields.
2. Transaction-session commands.
3. Branch-bundle commands.
4. Tag/note commands.
5. Manual durability maintenance commands from default schema.
6. Test-only or fault-injection commands.

Rules:

1. Every command must have a pathway or admin/debug justification.
2. Every command must define access-mode behavior.
3. Every command must define IPC behavior.
4. Every write command must define commit boundary and failure behavior.
5. Every optional command must define missing-feature behavior.
6. Every command output intended for JSON automation must avoid unstable debug
   strings as data.
7. Every stable command must have a golden JSON request and response example.
8. Every stable error returned through the command boundary must include a
   machine-readable code or class.
9. Command schemas must be versioned or otherwise guarded so integrations can
   detect incompatible changes.
10. Integration adapters must be able to call the same product operations as CLI
    and SDK without direct engine or storage access.

Maintenance authority:

1. `strata up` owns maintenance authority because it owns the writable local
   database handle.
2. Ordinary IPC clients do not receive maintenance authority by default.
3. Read-only handles never receive maintenance authority.
4. Admin/debug commands that require maintenance authority must fail before
   mutation when the caller lacks it.

## Re-Export Cleanup Checklist

The executor crate should not become a dumping ground for engine and storage
types.

Keep re-exports only when they are product contracts:

1. `Strata` or its V1 successor.
2. Product error type.
3. `AccessMode`.
4. Cleaned `OpenOptions`.
5. V1 config type or config builder.
6. Data capability DTOs.
7. Branch, version, time, search, graph, vector, event, and clone DTOs that
   users need.
8. Diagnostics DTOs.

Review or remove re-exports for:

1. `StorageIterator`.
2. Raw `StorageConfig` internals.
3. Raw `DurabilityMode` if durability becomes a config string/builder detail.
4. Raw WAL counters unless wrapped as product diagnostics.
5. Engine subsystem traits.
6. Graph/vector backend internals.
7. Recovery internals not intended for product diagnostics.
8. Branch implementation records that are not user-facing DTOs.

## CLAUDE.md Cutover Checklist

The V1 API cleanup must be reflected in `CLAUDE.md` before implementation
cutover:

1. Remove public transaction-session surface from the D4 product list.
2. Reconcile `SystemBranchCapability` with the control-plane contract; keep it
   private unless a product API requires it.
3. Replace product-default subsystem composition rules with internal capability
   registry/runtime composition rules.
4. Update the Engine Public API Surface section against the V1 product API and
   command-boundary contracts.
5. Remove follower-mode authority language from the current rules.

## Output Cleanup Checklist

Outputs must be stable enough for CLI JSON mode, IPC, SDK automation, and agent
workflows.

Checklist:

1. Remove follower fields from user-facing outputs unless replaced by a V1
   handle/open-mode field.
2. Remove transaction outputs when public transaction commands are removed.
3. Remove tag/note outputs.
4. Remove branch-bundle outputs.
5. Keep health, metrics, describe, and durability-counter outputs bounded.
6. Distinguish current reads, version reads, and timestamp reads clearly.
7. Do not expose storage file names, WAL segment IDs, compaction levels, or
   checkpoint internals except under diagnostics where they are meaningful.
8. Redact secrets and sensitive paths according to the diagnostics contract.

## Test And Guard Checklist

Before V1 surface freeze:

1. CLI help has no follower mode.
2. CLI help has no public transaction commands.
3. CLI help has no legacy branch-bundle commands.
4. CLI help has no public tag/note commands.
5. CLI help has no normal-user flush/compact/retention commands.
6. `Command::is_write()` covers every write command.
7. Read-only local and IPC handles reject the same writes.
8. Every command variant has a product category: required, optional,
   admin/diagnostic, or removed.
9. Executor re-export guard blocks accidental storage/engine leakage above the
   product API.
10. `Strata::database()` or any raw-handle escape hatch is absent from normal
    docs and unavailable in IPC-incompatible form.
11. Clone artifact commands reject partial/import failure cases.
12. Missing optional intelligence features produce stable unsupported-feature
    errors.
13. JSON automation output has golden tests for core commands.
14. CLI and SDK behavior agree for shared operations.
15. Serializable command DTOs have schema or golden-fixture compatibility tests.
16. Error-code fixtures prove integrations can branch on machine-readable
    failure classes without parsing strings.
17. Read/write classification tests cover every stable command and are shared by
    local, IPC, and CLI paths.
18. Integration-boundary guards reject production code above engine that imports
    storage directly without a documented exception.
19. Example adapters can perform open, describe, search, branch, clone, and
    mutation workflows using only the public command/API surface.

## Documentation Checklist

Before V1:

1. Product docs list only the final V1 default CLI commands.
2. SDK docs lead with open/cache/read-only, data capabilities, branches, search,
   clone, and diagnostics.
3. IPC docs describe same-machine sharing only.
4. No docs recommend follower mode.
5. No docs require manual flush, compact, checkpoint, or retention for ordinary
   use.
6. No docs present branch bundles as the V1 artifact.
7. No docs describe tags/notes as V1 branch requirements.
8. Optional intelligence docs clearly state runtime/provider requirements.
9. Import/export docs distinguish primitive data movement from `.strata` clone.
10. CLI examples avoid raw command JSON unless demonstrating automation.
11. Integration docs describe the public command/API boundary as the supported
    path for MCP servers, agent plugins, notebooks, ORMs, and automation.
12. Integration docs explicitly forbid direct storage access for normal product
    integrations.
13. JSON examples include both success and failure shapes for common workflows.

## V1 Minimum

The V1 minimum cleanup is:

1. Remove follower mode from public CLI/API.
2. Remove public transaction workflow.
3. Remove legacy branch-bundle commands from the public product path.
4. Remove public tags and notes.
5. Remove normal-user manual maintenance commands.
6. Keep IPC as the only same-machine shared-access story.
7. Add clone as the cold-start data movement story.
8. Keep data capability, branch, time-travel, search, config, and diagnostics
   surfaces product-shaped and tested.
9. Trim re-exports so application users do not depend on engine/storage
   internals.
10. Ensure every remaining command maps to a documented V1 pathway or an
    explicit optional/admin category.
11. Treat the public API, CLI, IPC, and command DTOs as the foundation for MCP,
    LangGraph, ORMs, Codex/Claude Code plugins, notebooks, and other
    integrations.
