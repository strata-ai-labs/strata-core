# Engine-Next Error And Diagnostics Contract

Status: V1 architecture draft

## Purpose

This document defines the engine-next error and diagnostics contract.

The cross-layer error vocabulary lives in
`docs/architecture/v1-error-and-diagnostics-contract.md`. This document applies
that vocabulary to engine-next:

1. How engine product failures become stable error statuses.
2. How storage-next, capability, branch, retrieval, clone, and IPC failures are
   mapped.
3. Which context the engine must attach before an error leaves the engine.
4. Which current error surfaces are transitional and must not be frozen.
5. Which conformance tests prove the engine error boundary is trustworthy.

Engine-next is the first layer that can interpret storage mechanics as product
meaning. It must make failures useful without leaking implementation history,
raw storage internals, credentials, or private debug strings.

## Related Documents

Read this with:

1. `docs/architecture/v1-error-and-diagnostics-contract.md`
2. `docs/architecture/engine-next-architecture.md`
3. `docs/architecture/engine-next/README.md`
4. `docs/architecture/engine-next/persistence-adapter-contract.md`
5. `docs/architecture/engine-next/primitive-implementation-contract.md`
6. `docs/architecture/engine-next/branch-operation-and-capability-adapter-contract.md`
7. `docs/architecture/engine-next/temporal-context-and-timeline-resolver-contract.md`
8. `docs/architecture/engine-next/retrieval-and-derived-state-contract.md`
9. `docs/architecture/engine-next/ipc-and-command-boundary-contract.md`
10. `docs/architecture/engine-next/dataset-clone-artifact-contract.md`
11. `docs/architecture/engine-next/product-pathway-conformance-plan.md`

## Requirement Language

1. Must means V1 behavior is incomplete without it.
2. Should means expected unless a later architecture decision records a clear
   deferral.
3. May means allowed but not required for V1.

## Current Code Evidence

The current codebase has the right broad ownership shape, but not the final V1
surface:

1. `crates/engine/src/error.rs` owns `StrataError`, a coarse `ErrorCode`, source
   chains for storage errors, structured details, and retry helpers.
2. `StrataError` currently exposes ten legacy wire codes:
   `NotFound`, `WrongType`, `InvalidKey`, `InvalidPath`, `HistoryTrimmed`,
   `ConstraintViolation`, `Conflict`, `SerializationError`, `StorageError`, and
   `InternalError`.
3. `crates/executor/src/error.rs` owns a serializable command error enum with
   many product-shaped variants.
4. `crates/executor/src/convert.rs` maps `StrataError` to executor errors, but
   loses target V1 fields such as class, detailed code, retry policy, commit
   outcome, and source-chain identity.
5. `crates/engine/src/database/product_open.rs` owns product open policy and
   lock-to-IPC classification, but its current locked-without-socket message is
   legacy and still mentions follower mode.
6. Storage recovery health types currently live in storage and are surfaced
   through engine paths for degraded recovery, retention refusal, and primitive
   degradation.

Engine-next should keep engine ownership of product errors and replace the
legacy coarse details with the V1 status contract.

## Relationship To The V1 Error Contract

The V1 error contract owns the common language:

1. Error class.
2. Error code format.
3. Retry policy.
4. Commit outcome.
5. Diagnostic status shape.
6. Source-chain and redaction rules.
7. Registry and testing expectations.

This engine contract owns engine application of that language:

1. Which engine operation emits which code family.
2. Which detail fields are attached by engine.
3. How storage errors are enriched at the persistence adapter.
4. How capability-local errors collapse into product diagnostics.
5. How command, IPC, CLI, SDK, agent, and integration boundaries preserve the
   engine status.

Engine-next must not define a second taxonomy. It may add engine-owned code
families to the shared registry, but those codes still use the V1 class, retry,
commit-outcome, and redaction model.

## Target Error Shape

Engine-next should have one product error parent for normal database APIs.

The implementation name is not frozen here. Conceptually it must carry:

```text
EngineError {
    status: ErrorStatus,
    source: Option<source error>,
}

ErrorStatus {
    class,
    code,
    retry_policy,
    commit_outcome,
    message,
    details,
    hints,
    trace_id,
}
```

Rules:

1. Engine APIs return the product error parent, not many unrelated public error
   enums.
2. Narrow helper errors may exist inside a module, but they collapse into the
   parent before crossing an engine boundary.
3. Public error types must be `#[non_exhaustive]`.
4. Public statuses must be serializable.
5. Source errors remain available for logs and diagnostics.
6. Tests assert on `class`, `code`, `retry_policy`, `commit_outcome`, and
   stable detail fields, not display text.

## Vocabulary Discipline

Engine-next should avoid growing a new vocabulary of one-off error types.

Allowed durable concepts:

1. `Error` or `EngineError` as the parent error.
2. `ErrorStatus` as the serializable boundary shape.
3. `ErrorClass`, `ErrorCode`, `RetryPolicy`, and `CommitOutcome` from the V1
   contract.
4. `ErrorContext` only if implementation needs a repeated internal builder for
   detail fields.

Avoid:

1. One enum per capability method.
2. Error variants that differ only in wording.
3. Local `FooProblem`, `FooFault`, `FooIssue`, `FooFailure` types for the same
   concept.
4. Catch-all `Internal` for ordinary invalid input, unsupported feature,
   missing dependency, conflict, or unavailable service.
5. Storing telemetry in error variants when callers do not consume it.

The error system should make the code easier to read, not create another
private language.

## Binding Decisions

1. **Engine owns product diagnostics.**
   Storage reports mechanical facts. Engine turns those facts into user-facing
   database meaning.

2. **The persistence adapter is the normal storage-error boundary.**
   Storage errors should cross into product code through the persistence
   adapter with branch, space, storage-space, operation, durability mode, and
   commit-phase context.

3. **Command boundaries preserve status, not enum shape.**
   SDK, CLI, IPC, MCP, notebook, LangGraph, agent, and plugin integrations
   should be able to branch on the same class/code regardless of transport.

4. **Expected product failures are never internal.**
   Missing branch, invalid JSON path, vector dimension mismatch, graph dangling
   reference, stale index, unsupported backend, missing model, read-only write,
   and lock conflict are ordinary product errors.

5. **Commit ambiguity is explicit.**
   Write failures that may have committed must expose `maybe_committed`. They
   must not be collapsed into `io`, `unavailable`, `conflict`, or `internal`.

6. **Read-only rejection is a failed precondition.**
   Capability code, command code, and the persistence adapter may all reject a
   write before mutation, but they must produce the same logical code and no
   storage mutation.

7. **Derived-state failures are not authored-data corruption.**
   Stale, missing, rebuilding, or degraded search, embedding, graph projection,
   and recipe state should fail as derived-state diagnostics unless source rows
   are actually corrupt.

8. **Recovery health is diagnostic state, not prose.**
   Recovery health, degradation class, and recovery faults should be available
   through structured health/diagnostic outputs and mapped errors.

9. **Cache, standard, and always durability modes share codes.**
   The same operation should return the same product code across durability
   modes when the same product failure occurs. Durability-mode facts appear in
   details.

10. **No hidden network or provider effects.**
    Missing model, disabled network, and unavailable provider errors must be
    normal statuses with redacted details and explicit retry/action guidance.

11. **Messages are not contracts.**
    Display strings can improve. Codes, classes, retry policies, commit
    outcomes, and stable detail keys are the contracts.

12. **Legacy public transaction errors are removed with the public transaction
    surface.**
    Internal commit machinery can still emit conflict, aborted, ambiguous, or
    durability errors. Users should not see begin/commit/rollback session-state
    errors as normal V1 product pathways.

## Engine Error Domains

### Open And Runtime

Open errors attach:

1. Requested data location, redacted where needed.
2. Requested storage backend and durability mode.
3. Access mode.
4. Runtime resource profile, when relevant.
5. IPC fallback classification, when relevant.
6. Recovery health or failure facts, when open reached recovery.

Target codes:

| Situation | Code | Commit outcome |
| --- | --- | --- |
| Invalid path or open option | `invalid_argument.open_options` | `not_applicable` |
| Unsupported backend or mode | `unsupported.storage_mode` | `not_applicable` |
| Missing backend capability | `unsupported.backend_capability` | `not_applicable` |
| Local writer lock held | `unavailable.writer_lock` | `not_applicable` |
| Writer lock held and IPC socket absent | `failed_precondition.ipc_not_running` | `not_applicable` |
| IPC socket present but unavailable | `unavailable.ipc_socket` | `not_applicable` |
| Permission denied | `access_denied.backend` | `not_applicable` |
| Corrupt manifest, WAL, table, snapshot, or timeline | `corruption.*` | `not_applicable` |
| Unsupported durable format | `unsupported.format_version` | `not_applicable` |
| Incomplete clone materialization | `failed_precondition.clone_incomplete` | `not_applicable` |

The current locked-without-socket message must be replaced before V1 because it
mentions follower mode. The V1 hint should tell the user to start the local IPC
owner with `strata up` if they want same-machine sharing.

### Access Mode And Write Authority

Write rejection must happen before mutation.

Target codes:

| Situation | Code | Retry policy |
| --- | --- | --- |
| Write through read-only handle | `failed_precondition.read_only` | `after_state_change` |
| Database closing | `unavailable.database_closing` | `after_state_change` |
| Database closed | `failed_precondition.database_closed` | `after_state_change` |
| Cache mode asked for durable guarantee | `failed_precondition.non_durable_cache` | `after_state_change` |
| Durability backend unavailable before commit | `unavailable.backend` | `same_request` |
| Always-mode durability outcome unknown | `ambiguous_commit.manifest_publish` | `unknown` |

Details should include `access_mode`, `command`, `durability_mode`, and
`commit_outcome`.

Maintenance-authority failures use `failed_precondition.maintenance_authority`.
For V1, `strata up` owns maintenance authority for its local database. Ordinary
IPC clients do not receive maintenance authority by default, and read-only
clients never receive it.

### Data Capability Operations

Data capabilities map local validation and decode errors into product status.

Common target codes:

| Situation | Code |
| --- | --- |
| Missing key or record when command treats absence as failure | `not_found.key` |
| Missing document | `not_found.document` |
| Missing event stream | `not_found.stream` |
| Missing vector collection | `not_found.collection` |
| Missing graph node, relationship, or graph object | `not_found.graph` |
| Wrong capability/type for operation | `failed_precondition.wrong_type` |
| Invalid key | `invalid_argument.key` |
| Invalid JSON path | `invalid_argument.path` |
| Vector dimension mismatch | `invalid_argument.vector_dimension` |
| Event append violates stream rules | `failed_precondition.event_sequence` |
| Graph relationship references missing/deleted entity | `failed_precondition.dangling_reference` |
| Capability value bytes are malformed | `corruption.capability_value` |
| Capability limit exceeded | `resource_exhausted.capability_limit` |

SDKs may choose `Option`-returning APIs for ordinary missing lookups. The
command, CLI, and IPC surfaces still need a status when absence is returned as
an error.

### Branch, Merge, And Time Travel

Branch and temporal failures are product errors. They should not be generic
invalid input strings.

Target codes:

| Situation | Code |
| --- | --- |
| Missing branch | `not_found.branch` |
| Branch already exists | `already_exists.branch` |
| Invalid branch name | `invalid_argument.branch_name` |
| Deleted branch | `failed_precondition.branch_deleted` |
| Archived branch | `failed_precondition.branch_archived` |
| Branch generation mismatch | `conflict.branch_generation` |
| Version conflict | `conflict.version` |
| Merge conflict | `conflict.branch_merge` |
| Cherry-pick conflict | `conflict.branch_cherry_pick` |
| Revert conflict | `conflict.branch_revert` |
| Requested version unavailable | `history_unavailable.version` |
| Requested timestamp unavailable | `history_unavailable.timestamp` |
| Timestamp is after latest known commit | `failed_precondition.timestamp_after_latest` |
| Timeline corrupt or inconsistent | `corruption.timeline` |

Details should include branch identity, branch name when user supplied it,
generation where relevant, requested version/timestamp, retained bounds, and
operation.

### Commit, Durability, And Storage Mapping

The persistence adapter maps storage outcomes into engine commit diagnostics.

Target rules:

1. Validation-before-commit uses `not_started`.
2. Rejected write with no mutation uses `definitely_not_committed`.
3. Storage uncertainty uses `maybe_committed`.
4. Successful commit followed by hook/index/recipe failure uses
   `committed_post_commit_failed`.
5. Durable-but-not-visible outcomes are surfaced explicitly and instruct the
   caller to reopen or inspect health rather than retry blindly.

Target codes:

| Situation | Code | Commit outcome |
| --- | --- | --- |
| Row conflict | `conflict.version` or `conflict.write` | `definitely_not_committed` |
| WAL append failed before acceptance | `io.backend_write` | `definitely_not_committed` |
| WAL publish unknown | `ambiguous_commit.wal_publish` | `maybe_committed` |
| Manifest publish unknown | `ambiguous_commit.manifest_publish` | `maybe_committed` |
| Durable sync failed after commit is known visible | `io.backend_sync` | `committed_post_commit_failed` |
| Durable sync timed out after possible publish | `ambiguous_commit.manifest_publish` | `maybe_committed` |
| Writer halted | `unavailable.writer_halted` | `not_started` |
| Write stall exceeded budget | `resource_exhausted.write_stall` | `not_started` or `definitely_not_committed` |
| Post-commit derived update failed | `failed_precondition.derived_update_failed` | `committed_post_commit_failed` |

The exact mapping depends on the storage-next L9 outcome. The implementation
must keep phase and commit outcome together so a future maintainer cannot map a
write-path storage error with a blanket conversion.

### Retrieval And Derived State

Search and retrieval errors must distinguish source data from derived state.

Target codes:

| Situation | Code |
| --- | --- |
| Recipe missing | `not_found.recipe` |
| Recipe invalid | `invalid_argument.recipe` |
| Unsupported recipe stage | `unsupported.search_stage` |
| Search index missing | `failed_precondition.index_not_ready` |
| Search index stale | `failed_precondition.index_stale` |
| Derived rebuild in progress | `unavailable.derived_rebuild` |
| Autoembedding not configured | `failed_precondition.embedding_not_available` |
| Model missing | `failed_precondition.model_not_configured` |
| Model/provider unavailable | `unavailable.model` or `unavailable.model_provider` |
| Network disabled by policy | `failed_precondition.network_disabled` |
| Provider response malformed | `serialization.provider_response` |
| Search result limit exceeded | `resource_exhausted.result_limit` |

Details should include recipe name/version, branch, space, temporal context,
stage name, index identifier, watermark, model/provider names, and whether
results are exact, partial, stale-refused, or unavailable.

### Graph Relationship Layer

Graph relationship errors should preserve the cross-capability entity context.

Target codes:

| Situation | Code |
| --- | --- |
| Relationship endpoint does not exist | `failed_precondition.dangling_reference` |
| Relationship endpoint was deleted before the selected time | `failed_precondition.dangling_reference` |
| Relationship ontology type missing | `not_found.graph` |
| Ontology frozen for mutation | `failed_precondition.ontology_frozen` |
| Traversal depth or result limit exceeded | `resource_exhausted.graph_traversal` |
| Unsupported graph algorithm | `unsupported.graph_algorithm` |
| Relationship reverse map corrupt | `corruption.derived_state` |

Details should include entity references, relationship type, direction, branch,
space, time context, ontology state, and result limits.

### Clone, Import, Export, And Provenance

Clone artifact failures are product errors over local materialization.

Target codes:

| Situation | Code |
| --- | --- |
| Source URL or path invalid | `invalid_argument.clone_source` |
| Destination exists | `already_exists.clone_destination` |
| Artifact unsupported | `unsupported.artifact_version` |
| Artifact checksum mismatch | `corruption.clone_artifact` |
| Artifact incomplete | `corruption.clone_artifact` |
| Feature omitted from artifact | `failed_precondition.artifact_feature_omitted` |
| Import schema mismatch | `serialization.import_payload` |
| Export format unsupported | `unsupported.export_format` |
| Partial destination cleaned up | status detail on the primary error |
| Partial destination quarantined | status detail on the primary error |

Details should include artifact ID, dataset identity, source, destination,
feature list, branch/version bounds, and cleanup/quarantine disposition. Hub
credentials, signed URLs, and tokens must be redacted.

### IPC And Command Boundary

IPC has two layers:

1. Command/database errors returned as command results.
2. Transport/protocol errors before or outside command execution.

Both layers must map to V1 status.

Target codes:

| Situation | Code |
| --- | --- |
| Socket absent or stale | `unavailable.ipc_socket` |
| IPC owner not running | `failed_precondition.ipc_not_running` |
| Protocol version mismatch | `unsupported.command_version` |
| Malformed frame | `serialization.ipc_protocol` |
| Oversized frame | `resource_exhausted.ipc_frame` |
| Response ID mismatch | `serialization.ipc_protocol` |
| Client disconnect during read command | `unavailable.ipc_socket` |
| Client disconnect during write command after possible commit | `ambiguous_commit.ipc_disconnect` |
| Server shutting down | `unavailable.database_closing` |

Local and IPC execution of the same command should return the same database
status unless the failure is truly transport-specific.

### Health, Describe, And Diagnostics

Diagnostics must be structured, bounded, and safe for read-only handles.

Engine health should report:

1. Database open mode and durability mode.
2. Backend capability facts.
3. Runtime resource profile and effective budgets.
4. Storage recovery health and degradation class.
5. Commit timeline bounds.
6. Branch count and branch lifecycle summary.
7. Derived-state health and rebuild status.
8. Search/index/embedding watermarks.
9. Graph relationship and ontology health.
10. Clone/provenance facts.
11. IPC availability when relevant.
12. Maintenance debt and writer health.

Health outputs should use ordinary status fields for degraded or unavailable
subsystems. A degraded search index is not an engine error unless the requested
operation requires that index and refuses stale results.

## Mapping Boundaries

### Capability To Engine Parent Error

Capability code should produce engine statuses with:

1. Capability name.
2. Branch and space.
3. Entity reference or capability-local key.
4. Operation.
5. Temporal context.
6. Source/control/derived classification where relevant.
7. Validation or decode context.

Capability helper errors may be narrow, but conversion must not lose these
facts.

### Persistence Adapter To Engine Parent Error

The persistence adapter enriches storage failures with:

1. Storage operation.
2. Engine operation origin.
3. Branch and space.
4. Storage-space symbolic assignment and numeric ID if safe.
5. Storage mode and backend kind.
6. Durability mode.
7. Commit phase.
8. Commit outcome.
9. Object role, such as WAL, manifest, table, snapshot, timeline, lock, or
   artifact.
10. Recovery health facts, when applicable.

Write-path storage errors must not use blanket conversion. Read-path blanket
conversion is acceptable only if it preserves class, code, retry policy, source
chain, and useful context.

### Engine To Executor, CLI, IPC, SDK, And Integrations

Every public boundary must preserve:

1. Error class.
2. Error code.
3. Retry policy.
4. Commit outcome.
5. Redacted message.
6. Stable details.
7. Hints.
8. Optional trace ID.

The executor's current serializable `Error` enum is a transitional shape. V1
should expose a status-compatible error response so integrations do not need to
track Rust enum variants or parse messages.

### Engine To Intelligence And Inference

Engine-next should not depend upward on intelligence-next or inference-next.
Engine-owned retrieval statuses should describe database and derived-state
availability. Intelligence-next and inference-next own provider execution
failures and map them into the same V1 status contract above engine.

When a public command boundary presents intelligence or inference failures next
to engine failures, the user-facing status should still distinguish:

1. Missing model configuration.
2. Disabled network.
3. Provider unavailable.
4. Recipe unsupported.
5. Derived state unavailable.

The provider-specific cause can live in the source chain or diagnostic details.
It should not look like storage corruption.

## Stable Detail Keys

The exact registry of stable detail keys belongs in the V1 error registry, but
engine-next should standardize these keys early:

| Key | Meaning |
| --- | --- |
| `operation` | Stable operation name. |
| `command` | Public command name, when error crosses command boundary. |
| `branch` | User-facing branch name, when supplied. |
| `branch_id` | Stable branch identity. |
| `branch_generation` | Branch lifecycle generation, when relevant. |
| `space` | Product space. |
| `entity_ref` | Product entity reference. |
| `storage_space` | Symbolic engine storage-space assignment. |
| `storage_space_id` | Numeric storage-space ID, when safe and useful. |
| `capability` | KV, JSON, event, vector, graph, search, recipe, or control. |
| `time_context` | latest, version, timestamp, or history selector. |
| `requested_version` | Requested commit version. |
| `requested_timestamp` | Requested timestamp. |
| `oldest_available_version` | Retained lower bound. |
| `latest_version` | Latest visible or recovered version. |
| `durability_mode` | cache, standard, or always. |
| `storage_mode` | cache/memory, local filesystem, or future backend mode. |
| `backend` | Backend kind with secrets removed. |
| `object_role` | WAL, manifest, table, snapshot, timeline, lock, artifact. |
| `commit_phase` | Validation, WAL, publish, visibility, post-commit, close. |
| `artifact_id` | Clone/export artifact identity. |
| `model` | Model name, if safe. |
| `provider` | Provider name, if safe. |
| `index` | Search, vector, graph, or derived-state index identifier. |
| `watermark` | Derived-state or timeline watermark. |
| `limit` | Limit name. |
| `requested` | Requested amount. |
| `maximum` | Configured maximum. |

Fields may be omitted when unknown or unsafe. They must not carry secrets.

## Redaction

Engine-next must redact:

1. Provider API keys.
2. StrataHub or private hub credentials.
3. Signed URLs.
4. Bearer tokens.
5. Access key IDs and secrets.
6. Full environment variable values.
7. Secret config values.
8. User data values unless the command normally returns that value.

Allowed:

1. Branch names and IDs.
2. User-supplied keys and paths when the command already exposes them.
3. Redacted backend or hub location.
4. Provider and model names.
5. Artifact IDs.
6. Object roles.
7. Numeric limits and versions.

Redaction applies to display strings, structured details, logs emitted at normal
levels, and IPC/CLI/SDK JSON responses.

## Current-To-Target Cutover

The current implementation should be treated as staging evidence, not V1 shape.

Cutover requirements:

1. Replace or wrap the current ten-code `ErrorCode` with the V1 class/code
   status model.
2. Keep source chaining from `StrataError`, but stop requiring integrations to
   match `StrataError` variants.
3. Align executor `Error` with status-compatible command errors.
4. Remove public transaction-session errors with the public transaction command
   cleanup.
5. Replace follower-mode hints in open and lock errors.
6. Replace unmapped executor conversions that return generic `Internal` for
   expected future variants.
7. Map `Corruption` to corruption-class status, not generic `Io`.
8. Preserve storage publish ambiguity through executor, CLI, IPC, and SDK.
9. Add status mapping for product open errors, including locked-without-socket.
10. Ensure capability-local errors carry capability, branch, space, entity, and
    temporal context before crossing public boundaries.

Compatibility shims may exist during migration, but V1 public fixtures should
use the status model.

## Conformance Requirements

Engine-next error conformance tests should include:

1. Registry tests proving every emitted engine code is registered.
2. Mapping tests from capability errors to stable statuses.
3. Mapping tests from storage-next L9 errors to stable engine statuses.
4. Write-path tests for `not_started`, `definitely_not_committed`,
   `maybe_committed`, and `committed_post_commit_failed`.
5. Read-only local and IPC write rejection tests.
6. Open failure tests for invalid path, writer lock, IPC not running,
   permission denied, unsupported backend, corrupt storage, and incomplete clone.
7. Branch tests for not-found, exists, archived, deleted, generation mismatch,
   merge conflict, cherry-pick conflict, revert conflict, and history
   unavailable.
8. Time-travel tests for version, timestamp, pruned history, timestamp after
   latest, and corrupt timeline.
9. Capability tests for KV, JSON, event, vector, graph, and relationship-layer
   validation failures.
10. Retrieval tests for stale index, missing index, rebuilding derived state,
    missing model, disabled network, provider unavailable, and invalid recipe.
11. Clone tests for corrupt artifact, unsupported artifact version, checksum
    mismatch, destination exists, and partial materialization cleanup.
12. IPC tests for socket unavailable, stale socket, protocol mismatch,
    oversized frame, server shutdown, and ambiguous disconnect during write.
13. Redaction tests for provider secrets, hub credentials, signed URLs, and
    backend credentials.
14. Golden JSON fixtures for representative command, CLI, IPC, and integration
    errors.
15. Guard tests that reject message-string assertions where class/code fields
    are available.

## Acceptance Criteria

This contract is satisfied when:

1. Engine-next has one normal product error parent.
2. Every public engine failure can produce a V1 `ErrorStatus`.
3. Storage errors cross through the persistence adapter with context.
4. Capability errors map into registered product codes.
5. Public command, IPC, CLI, SDK, and integration surfaces preserve status.
6. Read-only, durability, and commit-ambiguity errors are consistent.
7. Derived-state degradation is distinct from source-data corruption.
8. Open, recovery, branch, temporal, retrieval, clone, and IPC errors have
   stable conformance tests.
9. Sensitive values are redacted by default.
10. No normal product pathway depends on parsing display strings.

## Open Questions

Resolve before V1 implementation freezes:

1. What is the final Rust name of the public status-compatible error type?
2. Does engine-next keep `StrataError` as a compatibility wrapper, or replace it
   with the new parent type during cutover?
3. Which detail keys are stable public contract versus best-effort diagnostics?
4. Idempotency keys for write commands.
   Closed for V1: write commands do not add idempotency keys. Ambiguous commit
   retry policy remains `unknown`; clients should reopen or inspect state.
5. How much source-chain summary should trusted local IPC clients receive?
6. Which health facts are normal product output and which require diagnostic or
   debug mode?
7. Does the CLI render code/class by default, only in JSON mode, or both?
8. How are third-party integration adapters expected to preserve trace IDs?
