# V1 IDL Overlay KV And Vector Implementation Plan

## Status

Draft.

## Goal

Implement the first V1 IDL overlay slice for KV and vector commands.

This slice generates the IDL artifacts only:

```text
executor-next DTOs
  -> executor-owned thin KV/vector IDL overlay
  -> generated resolved command-index.json
```

CLI generation is Slice 2. Slice 1b first folds the IDL tooling into
executor-next. SDK, MCP, OpenAPI, and docs generation are deferred.

## Related Documents

1. `docs/architecture/v1-idl-overlay-strategy.md`
2. `docs/architecture/v1-public-output-inventory.md`
3. `docs/architecture/v1-response-contract-completion-plan.md`
4. `docs/architecture/v1-error-and-diagnostics-contract.md`
5. `docs/architecture/implementation-plans/v1-response-contract-completion-implementation-plan.md`
6. `docs/architecture/implementation-plans/v1-response-contract-completion-test-plan.md`

## Non-Goals

1. Do not wire CLI help, command listing, or `strata explain` in this slice.
2. Do not generate TypeScript, Python, MCP, OpenAPI, or docs artifacts.
3. Do not hand-author request or response field schemas.
4. Do not change executor request or response wire shapes unless a fixture
   reveals a pre-V1 response-contract bug.
5. Do not add JSON, event, graph, branch, space, admin, Arrow, or inference IDL
   commands in this slice.
6. Do not expose engine-next or storage-next internals through the IDL.

## Source Of Truth

Executor-next public DTOs remain the field source of truth.

The IDL overlay may reference:

1. executor command variants;
2. executor output variants;
3. shared response concepts;
4. public error codes;
5. golden request/response fixtures;
6. command prose files.

The IDL overlay must not define DTO fields.

## Authored Layout

Create:

```text
crates/executor-next/idl/v1/
  manifest.yaml
  defaults.yaml
  families.yaml
  kinds.yaml
  errors.yaml
  commands/
    kv.yaml
    vector.yaml
  prose/
    commands/
      kv.*.md
      vector.*.md
    snippets/
      mutation-ack.md
      optional-read.md
      cursor-page.md
      search-result.md
      itemwise-batch.md
      status-value.md
      diagnostics.md
  generated/
    command-index.json
```

Keep authored files optimized for humans. Keep generated files optimized for
machines and agents.

## Command Block Shape

Each command entry should be compact:

```yaml
id: kv.put
kind: mutation.put
title: Put KV value
input: Command::KvPut
output: Output::WriteResult
result: KvWrite
prose: commands/kv.put.md
errors+:
  - invalid_argument.kv.key
fixtures:
  response: kv/write_applied.json
```

Rules:

1. `id` is stable and user-facing.
2. `kind` selects inherited access, commit, pagination, batch, and response
   model defaults.
3. `input` and `output` reference executor command/output variants or generated
   DTO inventory names.
4. `result` fills a shared response concept.
5. `prose` points to a Markdown prose file.
6. `errors+` appends command-specific public errors to inherited errors.
7. `fixtures` points to checked-in golden fixtures.
8. Command entries cannot define DTO fields.

## Prose Shape

Command prose lives in Markdown, not YAML.

Example:

```markdown
---
summary: Store or replace a value by key.
mcp_description: Use this when the user wants to write, overwrite, or upsert a KV value.
---

Writes a binary value to the selected KV space. If the key already exists,
Strata replaces it and records a new version.
```

The resolver should read prose into the generated index so downstream CLI/SDK
work can consume a single resolved artifact.

## Required Defaults

Add only the defaults needed for KV and vector.

Required layers:

1. global defaults;
2. `kv` and `vector` family defaults;
3. operation kind defaults;
4. command overrides.

Required kind examples:

```text
mutation.put
mutation.delete
mutation.bulk_delete
mutation.metadata_update
read.get
read.history
read.page
read.search
read.diagnostics
read.status
read.sample
batch.itemwise_mutation
batch.itemwise_read
batch.itemwise_status
```

Do not add inheritance beyond:

```text
global -> family -> kind -> command
```

Do not add arbitrary expressions, conditionals, scripts, recursive includes, or
YAML anchors as a composition mechanism.

## Generated Command Index

`crates/executor-next/idl/v1/generated/command-index.json` should be
deterministic and fully resolved.

Each command entry must include:

1. `id`
2. `family`
3. `op`
4. `kind`
5. `title`
6. `summary`
7. `description`
8. `docs`
9. `cli.path`
10. `feature`
11. `access`
12. `input`
13. `output`
14. `outputs`
15. `wire_status`
16. `response_model`
17. `commit`
18. `pagination`
19. `batch`
20. `errors`
21. `fixtures`
22. `source`

The generated index is the only artifact downstream agents should need for
command discovery and explanation.

## KV Command Scope

Include:

| Command ID | Executor command | Output | Target concept |
| --- | --- | --- | --- |
| `kv.put` | `Command::KvPut` | `Output::WriteResult` | `MutationAck<KvWrite>` |
| `kv.get` | `Command::KvGet` | `Output::KvValue` | `Maybe<Bytes>` |
| `kv.delete` | `Command::KvDelete` | `Output::DeleteResult` | `MutationAck<KvDelete>` |
| `kv.list` | `Command::KvList` | `Output::Keys` / `Output::KeysPage` | `Page<Bytes, Bytes>` |
| `kv.scan` | `Command::KvScan` | `Output::KvScanResult` | `Page<ScanItem, Bytes>` |
| `kv.batch_put` | `Command::KvBatchPut` | `Output::BatchResults` | `BatchResult<KvMutationItem>` |
| `kv.batch_get` | `Command::KvBatchGet` | `Output::BatchGetResults` | `BatchResult<Maybe<Bytes>>` |
| `kv.batch_delete` | `Command::KvBatchDelete` | `Output::BatchResults` | `BatchResult<KvMutationItem>` |
| `kv.batch_exists` | `Command::KvBatchExists` | `Output::BoolList` | `BatchResult<StatusValue<bool>>` |
| `kv.exists` | `Command::KvExists` | `Output::Bool` | `StatusValue<bool>` |
| `kv.history` | `Command::KvGetv` | `Output::VersionHistory` | `Maybe<Vec<HistoryItem>>` |
| `kv.count` | `Command::KvCount` | `Output::Uint` | `StatusValue<u64>` |
| `kv.sample` | `Command::KvSample` | `Output::SampleResult` | `SamplePage<SampleItem>` |

Notes:

1. `Bool`, `BoolList`, and `Uint` are transitional executor outputs. The IDL
   should map them to product concepts instead of advertising anonymous DTOs as
   user-facing models.
2. `Command::KvGetv` should be exposed as `kv.history` unless the CLI later
   adopts a different stable name.

## Vector Command Scope

Include:

| Command ID | Executor command | Output | Target concept |
| --- | --- | --- | --- |
| `vector.collection.create` | `Command::VectorCreateCollection` | collection/status output | `MutationAck<VectorCollectionCreate>` or `StatusResponse<VectorCollectionInfo>` |
| `vector.collection.delete` | `Command::VectorDeleteCollection` | collection/status output | `MutationAck<VectorCollectionDelete>` |
| `vector.collection.list` | `Command::VectorListCollections` | `Output::VectorCollectionList` | `Page<VectorCollectionInfo, String>` |
| `vector.collection.stats` | `Command::VectorCollectionStats` | collection/status output | `StatusResponse<VectorCollectionInfo>` |
| `vector.count` | `Command::VectorCount` | `Output::Uint` | `StatusValue<u64>` |
| `vector.upsert` | `Command::VectorUpsert` | `Output::VectorWriteResult` | `MutationAck<VectorWrite>` |
| `vector.get` | `Command::VectorGet` | `Output::VectorData` | `Maybe<VectorVersionedData>` |
| `vector.history` | `Command::VectorGetv` | `Output::VectorVersionHistory` | `Maybe<Vec<VectorHistoryItem>>` |
| `vector.exists` | `Command::VectorExists` | `Output::Bool` | `StatusValue<bool>` |
| `vector.keys` | `Command::VectorListKeys` | `Output::VectorKeyPage` | `Page<String, String>` |
| `vector.metadata.update` | `Command::VectorUpdateMetadata` | `Output::VectorMetadataUpdateResult` | `MutationAck<VectorMetadataUpdate>` |
| `vector.delete` | `Command::VectorDelete` | `Output::VectorDeleteResult` | `MutationAck<VectorDelete>` |
| `vector.delete_by_filter` | `Command::VectorDeleteByFilter` | `Output::VectorBulkDeleteResult` | `MutationAck<VectorBulkDelete>` |
| `vector.delete_all` | `Command::VectorDeleteAll` | `Output::VectorBulkDeleteResult` | `MutationAck<VectorBulkDelete>` |
| `vector.query` | `Command::VectorQuery` | `Output::VectorMatches` | `SearchResult<VectorMatch>` |
| `vector.index.query` | `Command::VectorIndexQuery` | `Output::VectorIndexQuery` | `SearchResult<VectorMatch> + IndexDiagnostics` |
| `vector.batch_upsert` | `Command::VectorBatchUpsert` | `Output::VectorBatchUpsertResults` | `BatchResult<VectorMutationItem>` |
| `vector.batch_get` | `Command::VectorBatchGet` | `Output::VectorBatchGetResults` | `BatchResult<Maybe<VectorVersionedData>>` |
| `vector.batch_delete` | `Command::VectorBatchDelete` | `Output::VectorBatchDeleteResults` | `BatchResult<VectorMutationItem>` |

Notes:

1. If vector collection create/delete/stats currently use transitional outputs,
   the IDL should record the target concept and mark whether the current wire
   shape needs a later response-contract cleanup.
2. Plain vector query is not cursor-paginated but is still a bounded result set.
   The IDL should model it as `read.search`, not `read.page`.
3. `vector.index.query` is diagnostic search. Its match list should share the
   same search result concept with attached index diagnostics.

## Resolver And Generator

Implement or plan a deterministic resolver that can:

1. load authored YAML files;
2. load Markdown prose frontmatter and body;
3. load snippets;
4. apply global, family, kind, and command layers;
5. support plain field override;
6. support `field+` list append;
7. support `field-` list removal only if needed;
8. expand simple placeholders such as `{family}`, `{op}`, and `{result}`;
9. reject unknown fields;
10. reject unresolved placeholders;
11. reject duplicate command IDs;
12. reject duplicate CLI paths;
13. reject duplicate MCP names when present;
14. reject missing prose paths;
15. reject missing fixture paths;
16. reject unknown error codes;
17. reject unknown DTO references;
18. emit stable sorted JSON.

The implementation should live inside `executor-next` as isolated tooling. It
can use a feature-gated module plus a dev binary, but it must not introduce a
separate workspace crate or put YAML/prose parsing on the normal executor
runtime path.

## DTO Reference Inventory

Do not attempt full Rust-to-schema generation in this slice.

Use a small generated or hand-maintained DTO inventory for the scoped KV and
vector command/output variants plus shared response DTOs.

The inventory can be replaced later by an executor DTO schema generator.

## Error References

Validate every referenced command error against the executor public error
registry.

This slice does not need a complete exhaustive per-command error matrix. It
does need:

1. common scoped branch/space/read-only errors;
2. KV-specific validation errors referenced by KV commands;
3. vector-specific validation errors referenced by vector commands;
4. deterministic deduplication and ordering.

## Fixtures

The resolver should validate fixture paths. Missing representative KV/vector
fixtures should be added as needed.

Required fixture categories:

1. mutation applied;
2. mutation missing/no-op;
3. optional read found;
4. optional read missing;
5. page first/continued/terminal where applicable;
6. search result;
7. diagnostics search result;
8. batch success;
9. batch partial failure;
10. status/count;
11. representative failure.

Request fixtures should be added alongside response fixtures if absent.

## Implementation Steps

### 1. Add IDL Directory Skeleton

Create the `crates/executor-next/idl/v1` source and generated directories.

Exit criteria:

1. authored and generated files are clearly separated;
2. generated files are marked as generated;
3. root manifest remains small.

### 2. Define Minimal Source Schemas

Define allowed authored shapes for manifest, defaults, families, kinds, command
entries, and error overlays.

Exit criteria:

1. unknown fields fail validation;
2. command entries cannot define DTO fields;
3. merge operators are documented.

### 3. Add KV And Vector Family Defaults

Add family defaults for `kv` and `vector`.

Exit criteria:

1. each family declares feature, docs root, common errors, and default naming;
2. family defaults do not encode command-specific behavior.

### 4. Add Operation Kinds

Add only the operation kinds needed by KV and vector.

Exit criteria:

1. command entries inherit access/commit/pagination/batch facts;
2. unusual vector search/diagnostics behavior has explicit kind support;
3. no general expression engine is introduced.

### 5. Add KV Command Metadata And Prose

Add `commands/kv.yaml` and KV prose files.

Exit criteria:

1. every scoped KV command has metadata;
2. every scoped KV command has prose;
3. each command references executor inputs, primary output, all current wire
   outputs, errors, and fixtures.
4. multi-output commands, such as `kv.list`, include golden response fixtures
   for every declared current wire output.

### 6. Add Vector Command Metadata And Prose

Add `commands/vector.yaml` and vector prose files.

Exit criteria:

1. every scoped vector command has metadata;
2. every scoped vector command has prose;
3. vector search and index diagnostics are represented distinctly.

### 7. Implement Resolver

Implement load, validate, merge, placeholder expansion, prose loading, fixture
checking, error checking, DTO checking, and deterministic JSON emission.

Exit criteria:

1. `generate` emits `command-index.json`;
2. `check` fails on stale generated output;
3. invalid references produce actionable diagnostics;
4. repeated generation is byte-stable.

### 8. Add Explain-Ready Data

Do not wire CLI explain yet, but ensure generated command entries include all
facts needed by `strata explain <command-id>` in Slice 2.

Exit criteria:

1. generated entries include source paths;
2. generated entries include prose;
3. generated entries include inherited defaults in resolved form;
4. no downstream consumer needs to walk authored YAML/prose to explain a
   command.

### 9. Add Guard Tests

Add conformance tests for command coverage, generated freshness, fixture
existence, DTO refs, error refs, duplicate paths, and prose existence.

Exit criteria:

1. public KV/vector command metadata cannot drift unnoticed;
2. generated index cannot become stale;
3. authored IDL cannot become a loose YAML programming language.

## Completion Criteria

Slice 1 is complete when:

1. KV and vector authored command metadata exists.
2. KV and vector command prose exists.
3. A deterministic resolved `command-index.json` is generated.
4. Resolver validation covers DTO refs, error refs, fixture refs, duplicate
   command IDs, duplicate CLI paths, unresolved placeholders, and missing prose.
5. Generated entries contain all metadata needed for future CLI help and
   `strata explain`.
6. No CLI, SDK, MCP, OpenAPI, or docs generation is introduced.

## Slice 1b: Executor-Owned IDL Packaging

Slice 1b fixes the packaging boundary before CLI generation starts. The goal is
to make executor-next own the IDL overlay because executor-next owns the public
command boundary.

### Scope

1. Move authored IDL sources from repo-root `idl/strata/v1` to
   `crates/executor-next/idl/v1`.
2. Move generated artifacts to
   `crates/executor-next/idl/v1/generated/command-index.json`.
3. Move resolver/generator code from `crates/idl-next` into executor-owned
   tooling.
4. Add an executor package dev binary:

   ```text
   cargo run -p strata-executor-next --features idl-tooling --bin strata-idl -- generate
   cargo run -p strata-executor-next --features idl-tooling --bin strata-idl -- check
   ```

5. Remove `crates/idl-next` from the workspace.
6. Keep `serde_yaml` and other authoring-only dependencies behind
   executor-owned tooling so normal executor runtime builds do not compile the
   resolver.
7. Preserve the generated command index semantics. Diffs should be limited to
   source-path relocation and package/tool naming.

### Non-Goals

1. Do not add CLI command execution or `strata explain`.
2. Do not add SDK, MCP, OpenAPI, or docs generation.
3. Do not change KV/vector command IDs, prose semantics, response models, error
   refs, or fixture meanings.
4. Do not expand IDL coverage beyond KV/vector.

### Implementation Steps

1. Create `crates/executor-next/idl/v1` and move authored/generated IDL files
   into it.
2. Move resolver code into an executor-owned `idl_tooling` module or equivalent
   isolated source tree.
3. Add the `strata-idl` dev binary under `crates/executor-next`.
4. Update default repo-root and IDL-path resolution to use the executor package
   layout.
5. Update generated `source.command` paths to point at
   `crates/executor-next/idl/v1/...`.
6. Update docs and CI commands from `strata-idl-next` to the executor-owned
   `strata-idl` binary.
7. Remove `crates/idl-next` from workspace members and delete the standalone
   crate.
8. Re-run generation and check in the new location.

### Exit Criteria

1. `cargo run -p strata-executor-next --features idl-tooling --bin strata-idl -- generate`
   succeeds.
2. `cargo run -p strata-executor-next --features idl-tooling --bin strata-idl -- check`
   succeeds.
3. KV/vector IDL tests run under `strata-executor-next`, not a standalone IDL
   crate.
4. Normal executor tests still pass without enabling `idl-tooling`.
5. `crates/idl-next` no longer exists and no workspace member references it.
6. Generated command-index content is unchanged except for expected source-path
   relocation.
7. No downstream consumer needs to traverse authored YAML/prose.

## Follow-Up Slice

After Slice 1b, Slice 2 should be:

```text
docs/architecture/implementation-plans/v1-idl-overlay-cli-generation-implementation-plan.md
```

That slice should wire CLI help, command listing, and `strata explain` to the
resolved `command-index.json`.
