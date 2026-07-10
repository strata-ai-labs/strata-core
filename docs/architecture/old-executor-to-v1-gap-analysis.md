# Old Executor To V1 Gap Analysis

## Purpose

This document records the feature gap between the last old executor surface and
the current V1 `executor-next` surface after the old crates were removed from
the workspace.

The goal is not to restore every old command. The goal is to avoid losing
intentional product capabilities by accident, and to separate:

1. capabilities already covered in V1;
2. old commands that were renamed or replaced;
3. old commands deliberately excluded from V1;
4. old capabilities that should be restored in a future implementation pass.

## Inventory Source

The old executor inventory is recovered from git history:

- old surface: `cb01f0dd:crates/executor/src/command.rs`
- current surface: `crates/executor-next/src/command.rs`

At the old inventory point, the executor exposed 153 command variants. The
current V1 executor exposes 103 command variants. That raw count overstates the
gap because several old commands were renamed, replaced, or intentionally
removed.

## Disposition Labels

| Label | Meaning |
| --- | --- |
| Covered | Current V1 has the same capability or a stricter equivalent. |
| Renamed | Current V1 has the same capability under a new command family/name. |
| Replaced | Current V1 has a different primitive that covers the same common use case. |
| Restore | Capability should be ported back in a focused V1 follow-up. |
| Defer | Capability is real, but should remain out of the current core milestone. |
| Remove | Capability should not be public in V1. |

## Executive Summary

The two obvious missing areas are real:

1. git-style branch semantics;
2. graph ontology.

There are additional old executor families that are not present in V1:

1. graph analytics and BFS traversal;
2. search/query and recipes;
3. automatic embedding control plane;
4. explicit transaction/session commands;
5. old public admin/maintenance controls;
6. tags and notes attached to branch versions.

Most of these are not executor-only gaps. The current `engine-next` either does
not expose the underlying API yet, or has guard tests that intentionally keep the
old surface out.

## Covered Or Renamed Surface

### KV

Disposition: Covered.

Old KV commands are covered, with additional V1 response normalization:

- `KvPut`
- `KvGet`
- `KvDelete`
- `KvList`
- `KvScan`
- `KvBatchPut`
- `KvBatchGet`
- `KvBatchDelete`
- `KvBatchExists`
- `KvExists`
- `KvGetv`
- `KvCount`
- `KvSample`

### JSON

Disposition: Covered.

Old JSON commands are covered:

- `JsonSet`
- `JsonGet`
- `JsonDelete`
- `JsonList`
- `JsonBatchSet`
- `JsonBatchGet`
- `JsonBatchDelete`
- `JsonExists`
- `JsonGetv`
- `JsonCount`
- `JsonCreateIndex`
- `JsonDropIndex`
- `JsonListIndexes`
- `JsonSample`

### Event

Disposition: Covered, with one V1 addition.

Old event commands are covered:

- `EventAppend`
- `EventBatchAppend`
- `EventGet`
- `EventExists`
- `EventLen`
- `EventRange`
- `EventRangeByTime`
- `EventList`
- `EventGetByType`
- `EventListTypes`

V1 also adds:

- `EventVerifyChain`

### Vector

Disposition: Covered, with V1 additions and one old minor omission.

Old vector commands covered:

- `VectorCreateCollection`
- `VectorDeleteCollection`
- `VectorListCollections`
- `VectorCollectionStats`
- `VectorUpsert`
- `VectorBatchUpsert`
- `VectorGet`
- `VectorBatchGet`
- `VectorExists`
- `VectorDelete`
- `VectorBatchDelete`
- `VectorQuery`
- `VectorCount`
- `VectorGetv`

V1 additions:

- `VectorIndexQuery`
- `VectorListKeys`
- `VectorUpdateMetadata`
- `VectorDeleteAll`
- `VectorDeleteByFilter`

Old command not present:

- `VectorSample`

`VectorSample` is a small read-surface gap, not a strategic blocker.

### Inference And Model Operations

Disposition: Renamed and partially expanded.

Old commands:

- `ModelsList`
- `ModelsLocal`
- `ModelsPull`
- `Generate`
- `Tokenize`
- `Detokenize`
- `Embed`
- `EmbedBatch`
- `GenerateUnload`

Current V1 commands:

- `InferenceModelsList`
- `InferenceModelsLocal`
- `InferenceModelsPull`
- `InferenceModelCapability`
- `InferenceGenerate`
- `InferenceTokenize`
- `InferenceDetokenize`
- `InferenceEmbed`
- `InferenceEmbedBatch`
- `InferenceRank`
- `InferenceUnload`
- `InferenceCacheStatus`

This covers direct inference. It does not cover the old automatic embedding
pipeline or search integration.

### Spaces

Disposition: Covered.

Current V1 covers:

- `SpaceList`
- `SpaceCreate`
- `SpaceExists`
- `SpaceDelete`

### Basic Admin And Status

Disposition: Covered for read/status, intentionally narrower for mutation.

Current V1 covers:

- `Ping`
- `Info`
- `Health`
- `Metrics`
- `Describe`
- `ConfigGet`
- `ConfigureGetKey`

The old public maintenance/config mutation controls are discussed separately.

### Arrow Import And Export

Disposition: Covered and changed.

Old command:

- `ArrowImport`

Current V1 commands:

- `ArrowImport`
- `ArrowExport`

Old `DbExport` is not directly restored. Current V1 export scope is Arrow import
and export rather than the old database bundle export surface.

## Major Missing Surface

### 1. Git-Style Branch Semantics

Disposition: Restore in a dedicated branch semantics milestone.

Current V1 branch support:

- `BranchList`
- `BranchGet`
- `BranchCreate`
- `BranchForkCurrent`
- `BranchForkAtVersion`
- `BranchForkAtTimestamp`
- `BranchDelete`

Old executor branch capabilities missing from V1:

- `BranchExists`
- `BranchDiff`
- `BranchMerge`
- `BranchDiffThreeWay`
- `BranchMergeBase`
- `BranchRevert`
- `BranchCherryPick`
- `BranchExport`
- `BranchImport`
- `BranchBundleValidate`

Related old branch annotation commands missing from V1:

- `TagCreate`
- `TagDelete`
- `TagList`
- `TagResolve`
- `NoteAdd`
- `NoteGet`
- `NoteDelete`

This is not just executor wiring. The old executor called branch APIs such as
diff, merge, merge-base, revert, cherry-pick, and bundle import/export on the old
engine. The current `engine-next` branch API does not expose equivalent product
operations yet.

Recommended restoration scope:

1. branch diff and merge-base first;
2. merge with explicit strategy and conflict response model;
3. revert and cherry-pick after diff/merge contracts are stable;
4. tags and notes as a branch metadata follow-up;
5. bundle export/import only after durable format and compatibility policy are
   settled.

Do not treat this as a CLI-only problem. The engine needs a conflict model,
lineage model, response model, and tests before executor commands are added.

### 2. Graph Ontology

Disposition: Restore in a dedicated graph ontology milestone.

Current V1 graph support:

- `GraphCreate`
- `GraphDelete`
- `GraphList`
- `GraphGetMeta`
- `GraphAddNode`
- `GraphGetNode`
- `GraphRemoveNode`
- `GraphListNodes`
- `GraphAddEdge`
- `GraphGetEdge`
- `GraphRemoveEdge`
- `GraphNeighbors`
- `GraphBindingsForEntity`
- `GraphBatchWrite`

Old graph ontology commands missing from V1:

- `GraphDefineObjectType`
- `GraphGetObjectType`
- `GraphListObjectTypes`
- `GraphDeleteObjectType`
- `GraphDefineLinkType`
- `GraphGetLinkType`
- `GraphListLinkTypes`
- `GraphDeleteLinkType`
- `GraphFreezeOntology`
- `GraphOntologyStatus`
- `GraphOntologySummary`
- `GraphListOntologyTypes`
- `GraphNodesByType`

Current guard tests explicitly exclude ontology terms from the graph core. This
matches the earlier decision to keep graph core operations separate from
ontology.

Recommended restoration scope:

1. ontology schema records in branch-local system/control space;
2. object type and link type CRUD;
3. validation at node/edge write boundaries;
4. freeze semantics and status/summary;
5. `GraphNodesByType` only after object type indexing/query cost is understood.

Ontology should not be mixed into the already completed graph core command
contract. It is its own layer.

### 3. Graph Analytics And Traversal

Disposition: Defer.

Old commands missing from V1:

- `GraphBfs`
- `GraphWcc`
- `GraphCdlp`
- `GraphPagerank`
- `GraphLcc`
- `GraphSssp`

The V1 graph primitive currently exposes storage-backed graph CRUD and local
neighbor traversal. It does not expose graph analytics. Guard tests currently
exclude these names from graph core.

Recommended disposition:

1. keep out of V1 core;
2. revisit after ontology and the search/query layer are settled;
3. decide whether analytics belongs in `engine-next`, a query layer, or an
   optional analysis crate.

### 4. Search, Retrieval, And Recipes

Disposition: Defer to the search/query milestone.

Old commands missing from V1:

- `Search`
- `RecipeGetDefault`
- `RecipeGet`
- `RecipeSet`
- `RecipeList`
- `RecipeSeed`
- `RecipeDelete`

The old search path combined:

1. recipe resolution;
2. query embedding;
3. expansion;
4. retrieval substrate;
5. fusion;
6. reranking;
7. optional RAG answer generation;
8. version/diff enrichment.

The current V1 engine explicitly does not define the old `Recipe` and `Search`
row families. This should stay out of the primitive control plane and be handled
as a separate search/query layer pass.

Recommended restoration scope:

1. define the search/query product surface first;
2. define recipe storage separately from primitive core rows;
3. wire inference providers through `inference-next`;
4. reuse vector indexing where appropriate;
5. add shadow-vector behavior only after the base search layer is stable.

### 5. Automatic Embedding Control Plane

Disposition: Defer until search/query and local AI setup are designed.

Old commands missing from V1:

- `ConfigSetAutoEmbed`
- `AutoEmbedStatus`
- `EmbedStatus`
- `ReindexEmbeddings`
- `ConfigureModel`
- `ConfigureSet`

Direct inference is present in V1 under `Inference*` commands. What is missing
is the automatic embedding pipeline that connects writes to embedding jobs and
search indexes.

Recommended disposition:

1. keep direct inference commands as the current V1 surface;
2. add local/cloud model configuration through the first-time setup/admin config
   plan, not through ad hoc runtime key mutation;
3. restore auto-embedding only as part of the search/query layer.

### 6. Explicit Transaction Sessions

Disposition: Defer unless SDK/CLI workflows require long-lived transactions.

Old commands missing from V1:

- `TxnBegin`
- `TxnCommit`
- `TxnRollback`
- `TxnInfo`
- `TxnIsActive`

Current V1 relies on command-level atomicity and explicit batch commands. That
is simpler for remote execution, generated surfaces, and public response
contracts.

Recommended disposition:

1. keep out of the V1 executor command surface for now;
2. revisit if SDK users need multi-command atomic sessions;
3. do not restore without a session identity, timeout, rollback, and durable
   recovery contract.

### 7. Public Admin And Maintenance Controls

Disposition: Mostly remove from public V1.

Old commands missing from V1:

- `Flush`
- `Compact`
- `DurabilityCounters`
- `RetentionPreview`
- `RetentionApply`
- `RetentionStats`
- `TimeRange`

The current V1 executor has guard tests that reject public `Flush`, `Compact`,
durability counters, retention commands, and broad runtime config mutation. This
matches the earlier product decision that Strata should not expose low-level
maintenance controls as normal user operations.

Recommended disposition:

1. keep `Flush` and `Compact` internal/test-only;
2. expose compact health/metrics/status instead of storage internals;
3. design retention as a product-level lifecycle policy before restoring public
   commands;
4. decide separately whether `TimeRange` belongs in admin status, event APIs, or
   query diagnostics.

## Minor Or Ambiguous Gaps

### BranchExists

Disposition: Optional restore.

`BranchExists` can be derived from `BranchGet` or `BranchList`, but a direct
exists command may be useful for generated SDKs and CLI status checks.

### VectorSample

Disposition: Optional restore.

KV and JSON have sample commands in V1. Vector does not. Restore only if users
need deterministic inspection of vector collections without search.

### DbExport

Disposition: Defer.

V1 has `ArrowExport`, but not an old-style database export. A full database or
branch bundle format overlaps with branch bundle import/export and durable
format compatibility, so it should not be restored casually.

### GraphBulkInsert

Disposition: Replaced.

V1 has `GraphBatchWrite`, which should cover the important bulk-write use case.
If old bulk insert had chunking semantics that matter for very large imports,
that should be covered by an import or batch-size policy rather than reviving the
old command shape.

### GraphListNodesPaginated

Disposition: Covered by normalized pagination if `GraphListNodes` exposes page
fields consistently.

The old separate command should not be restored unless the current graph list
shape cannot express pagination cleanly.

## Restore Priority

Recommended near-term order:

1. write implementation and test plans for git-style branch semantics;
2. write implementation and test plans for graph ontology;
3. decide whether `BranchExists` and `VectorSample` should be small cleanup
   tickets;
4. keep graph analytics, search/recipes, auto-embedding, explicit transactions,
   and public maintenance commands deferred.

## Verification Required Before Restoring A Feature

Every restored old feature should pass this checklist:

1. The engine-next API exists and owns the behavior.
2. The executor command is a thin command boundary, not a reimplementation.
3. The response shape follows the V1 response/error contract.
4. Branch, space, timestamp, and durability behavior are specified.
5. Cache and durable modes are tested where applicable.
6. Golden response fixtures are added for public success and failure shapes.
7. Guard tests are updated so intentionally deferred old commands remain
   excluded.

## Current Recommendation

The highest-value missing product capability is git-style branch semantics. It
is central to Strata's branch-first identity and it affects how users reason
about changes.

The second highest-value missing capability is graph ontology. It is real
product surface, but it should remain separate from graph core operations.

Search/recipes and automatic embedding should be handled later as the
search/query layer, not as part of branch or graph ontology restoration.
