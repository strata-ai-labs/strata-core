# V1 IDL Overlay KV And Vector Test Plan

## Status

Draft.

## Goal

Verify the KV/vector IDL generation slice.

This test plan proves that:

1. executor-next DTOs remain the field source of truth;
2. executor-next owns the IDL overlay and generator tooling;
3. KV and vector command metadata is complete and valid;
4. command prose is separate, present, and consistently resolved;
5. generated `command-index.json` is deterministic and fresh;
6. generated entries contain the facts needed by later CLI generation;
7. broken references fail fast with useful diagnostics.

CLI help, CLI command listing, and `strata explain` execution are Slice 2 and
are out of scope for this test plan.

## Related Documents

1. `docs/architecture/v1-idl-overlay-strategy.md`
2. `docs/architecture/implementation-plans/v1-idl-overlay-kv-vector-implementation-plan.md`
3. `docs/architecture/v1-public-output-inventory.md`
4. `docs/architecture/v1-error-and-diagnostics-contract.md`
5. `docs/architecture/implementation-plans/v1-response-contract-completion-test-plan.md`

## Test Scope

In scope:

1. KV command metadata.
2. Vector command metadata.
3. KV command prose.
4. Vector command prose.
5. Shared defaults and kind resolution needed by KV/vector.
6. Generated resolved command index.
7. KV/vector fixture references.
8. Error-code references.
9. Executor DTO reference inventory.
10. Explain-ready generated data.
11. Executor-owned IDL packaging after Slice 1b.

Out of scope:

1. CLI help generation.
2. CLI command-list generation.
3. `strata explain` CLI execution.
4. SDK generation.
5. MCP generation.
6. OpenAPI generation.
7. Long-form docs generation.
8. Full Rust DTO schema generation.

## Required KV Command Coverage

Every command below must have a resolved IDL entry:

1. `kv.put`
2. `kv.get`
3. `kv.delete`
4. `kv.list`
5. `kv.scan`
6. `kv.batch_put`
7. `kv.batch_get`
8. `kv.batch_delete`
9. `kv.batch_exists`
10. `kv.exists`
11. `kv.history`
12. `kv.count`
13. `kv.sample`

## Required Vector Command Coverage

Every command below must have a resolved IDL entry:

1. `vector.collection.create`
2. `vector.collection.delete`
3. `vector.collection.list`
4. `vector.collection.stats`
5. `vector.count`
6. `vector.upsert`
7. `vector.get`
8. `vector.history`
9. `vector.exists`
10. `vector.keys`
11. `vector.metadata.update`
12. `vector.delete`
13. `vector.delete_by_filter`
14. `vector.delete_all`
15. `vector.query`
16. `vector.index.query`
17. `vector.batch_upsert`
18. `vector.batch_get`
19. `vector.batch_delete`

## Required Resolved Fields

Every resolved command entry must include:

1. `id`;
2. `family`;
3. `op`;
4. `kind`;
5. `title`;
6. `summary`;
7. `description`;
8. `docs`;
9. `cli.path`;
10. `feature`;
11. `access`;
12. `input`;
13. `output`;
14. `outputs`;
15. `wire_status`;
16. `response_model`;
17. `commit`;
18. `pagination`;
19. `batch`;
20. `errors`;
21. `fixtures`;
22. `source`.

## Resolver Tests

### Source Loading

Tests:

1. loads manifest, defaults, families, kinds, commands, errors, prose, and
   snippets;
2. rejects missing required source files;
3. rejects duplicate command IDs;
4. rejects duplicate family IDs;
5. rejects duplicate kind IDs;
6. rejects duplicate error overlay IDs;
7. rejects duplicate prose IDs if prose IDs are introduced.

Exit criteria:

1. resolver cannot silently skip a source file;
2. duplicates fail before generation.

### Source Schema Validation

Tests:

1. rejects unknown top-level fields in command entries;
2. rejects field definitions inside command metadata;
3. rejects missing `id`;
4. rejects missing `kind`;
5. rejects missing `input`;
6. rejects missing `output`;
7. rejects missing `prose`;
8. rejects malformed command IDs;
9. rejects malformed merge operators;
10. rejects unsupported inheritance depth;
11. rejects YAML anchors if the parser exposes anchor information or a source
   guard can detect them.

Exit criteria:

1. authored IDL cannot become a loose YAML blob;
2. malformed config produces targeted diagnostics.

### Merge Resolution

Tests:

1. applies global defaults;
2. applies family defaults;
3. applies kind defaults;
4. applies command overrides;
5. command plain field overrides inherited value;
6. `errors+` appends inherited errors;
7. list values are deduplicated in deterministic order;
8. `errors-` removes inherited errors if supported;
9. missing family fails;
10. missing kind fails;
11. vector search kind resolves differently from cursor page kind.

Exit criteria:

1. resolved commands are deterministic;
2. inherited facts are visible in generated output.

### Placeholder Resolution

Tests:

1. expands `{family}`;
2. expands `{op}`;
3. expands `{result}`;
4. rejects unknown placeholders;
5. rejects unresolved placeholders in generated output;
6. rejects placeholder expansion that produces an invalid CLI path;
7. rejects placeholder expansion that produces an invalid docs URL;
8. reports the source file and field for placeholder failures.

Exit criteria:

1. generated artifacts contain no template syntax;
2. placeholder failures are actionable.

### Prose Loading

Tests:

1. loads command Markdown by path;
2. parses frontmatter;
3. requires `summary`;
4. supports optional `mcp_description`;
5. preserves Markdown body;
6. rejects missing prose file;
7. rejects empty summary;
8. rejects empty Markdown body unless explicitly allowed;
9. loads referenced snippets;
10. rejects missing snippets;
11. keeps command prose out of command YAML.

Exit criteria:

1. every public KV/vector command has canonical prose;
2. generated output does not invent prose.

### DTO Reference Validation

Tests:

1. accepts known KV command variants;
2. accepts known KV output variants;
3. accepts known vector command variants;
4. accepts known vector output variants;
5. accepts required shared response DTO references;
6. rejects unknown `input`;
7. rejects unknown `output`;
8. rejects unknown `response_model` wrapper;
9. rejects command metadata that references unscoped primitive DTOs unless the
   command is explicitly marked transitional.

Exit criteria:

1. IDL cannot drift away from executor-next DTOs;
2. field schemas are not redefined in the IDL.

### Error Reference Validation

Tests:

1. accepts known common scoped error codes;
2. accepts known KV-specific error codes;
3. accepts known vector-specific error codes;
4. rejects unknown error code;
5. deduplicates inherited and command-specific repeated errors;
6. preserves deterministic error ordering;
7. includes docs URL when registry provides it;
8. fails when a referenced error has no registry entry.

Exit criteria:

1. generated command metadata never advertises an unregistered public error;
2. later CLI/SDK/docs generation can trust resolved error refs.

### Fixture Reference Validation

Tests:

1. accepts existing request fixture paths;
2. accepts existing response fixture paths;
3. rejects missing request fixture;
4. rejects missing response fixture;
5. rejects fixture path outside the fixture root;
6. verifies fixture `type` tags match the declared executor input/output
   variants;
7. verifies multi-output commands cover every declared output tag with response
   fixtures;
8. verifies fixture names are stable and lower snake case;
9. allows multiple response fixtures for found/missing or applied/no-op cases.

Exit criteria:

1. every IDL command is backed by examples;
2. generated artifacts cannot point to nonexistent fixtures.

## Generated Artifact Tests

### Deterministic Output

Tests:

1. repeated generation produces byte-identical `command-index.json`;
2. command entries are sorted by command ID;
3. list fields have deterministic ordering;
4. JSON formatting is stable;
5. generated file contains a generated-file marker;
6. generated output includes generator version or schema version.

Exit criteria:

1. generated diffs are reviewable;
2. CI can enforce freshness reliably.

### Freshness Check

Tests:

1. `check` passes when generated output is current;
2. `check` fails when generated output is stale;
3. `check` reports which generated file is stale;
4. `check` does not rewrite files unless explicitly asked to generate;
5. manual edits to generated output fail freshness checks.

Exit criteria:

1. CI can detect stale generated IDL artifacts.

### Resolved Command Shape

Tests:

1. every resolved command has all required fields;
2. no resolved command has null for required facts;
3. `family` and `op` match command ID;
4. `cli.path` is non-empty even though CLI wiring is deferred;
5. `docs` is non-empty;
6. `summary` is non-empty;
7. `description` is non-empty;
8. `source.command` points to authored YAML;
9. `source.prose` points to Markdown prose;
10. `input` and `output` are known references;
11. generated entry is explain-ready without authored source traversal.

Exit criteria:

1. downstream generators can consume one resolved artifact without walking
   authored sources.

## KV Concept Coverage Tests

### Mutation Acknowledgement

Commands:

1. `kv.put`
2. `kv.delete`
3. `kv.batch_put`
4. `kv.batch_delete`

Tests:

1. resolved `response_model` is mutation or batch mutation concept;
2. write commands resolve `access=write`;
3. write commands resolve non-`none` commit behavior;
4. fixtures include applied and no-op/missing examples where the output shape
   supports them.

### Optional Reads

Commands:

1. `kv.get`
2. `kv.history`
3. `kv.batch_get`

Tests:

1. resolved model distinguishes found from missing;
2. `kv.get` maps to `Maybe<Bytes>`;
3. `kv.history` maps to `Maybe<Vec<HistoryItem>>`;
4. batch get maps item results to a maybe-like concept;
5. fixtures include found and missing examples.

### Pages

Commands:

1. `kv.list`
2. `kv.scan`
3. `kv.sample`

Tests:

1. resolved page commands identify cursor pagination or sample-page behavior;
2. cursor is documented as opaque;
3. page commands resolve `access=read`;
4. fixtures include terminal page behavior.

### Batches

Commands:

1. `kv.batch_put`
2. `kv.batch_get`
3. `kv.batch_delete`
4. `kv.batch_exists`

Tests:

1. resolved `batch` facts identify itemwise mode;
2. item result concept is present;
3. fixtures cover success and partial failure where supported;
4. batch command prose explains positional item results.

## Vector Concept Coverage Tests

### Collection Lifecycle

Commands:

1. `vector.collection.create`
2. `vector.collection.delete`
3. `vector.collection.list`
4. `vector.collection.stats`

Tests:

1. lifecycle commands resolve to mutation/status/page concepts as appropriate;
2. list resolves cursor page facts;
3. stats resolves read/status facts;
4. transitional response-shape gaps are explicitly marked if present.

### Vector Entry Mutation

Commands:

1. `vector.upsert`
2. `vector.metadata.update`
3. `vector.delete`
4. `vector.delete_by_filter`
5. `vector.delete_all`
6. `vector.batch_upsert`
7. `vector.batch_delete`

Tests:

1. write commands resolve `access=write`;
2. mutation commands resolve commit behavior;
3. bulk delete commands resolve bulk mutation concepts;
4. batch mutation commands resolve itemwise batch facts;
5. fixtures include applied and missing/no-op where applicable.

### Vector Reads

Commands:

1. `vector.get`
2. `vector.history`
3. `vector.exists`
4. `vector.keys`
5. `vector.count`
6. `vector.batch_get`

Tests:

1. optional reads map to maybe concepts;
2. keys maps to cursor page;
3. count/exists avoid anonymous `Bool`/`Uint` product concepts;
4. batch get maps to itemwise maybe concept.

### Search And Diagnostics

Commands:

1. `vector.query`
2. `vector.index.query`

Tests:

1. plain query maps to `SearchResult<VectorMatch>`;
2. index query maps to search result with diagnostics;
3. query does not falsely advertise cursor pagination;
4. vector filter errors are referenced and registered;
5. diagnostics prose is distinct from plain query prose.

## Negative Tests

Add fixture-based negative tests for:

1. duplicate `kv.put` entry;
2. duplicate `vector.query` entry;
3. command referencing nonexistent prose;
4. command referencing nonexistent output DTO;
5. command referencing unknown error code;
6. command with duplicated CLI path;
7. command with unresolved `{result}`;
8. command YAML defining a fake field schema;
9. generated output manually edited;
10. response fixture path escaping fixture root;
11. prose file missing `summary`;
12. vector search command incorrectly using cursor pagination kind.

Each failure should assert a specific, actionable diagnostic.

## Source Guard Tests

Add source guards that enforce:

1. generated artifacts are not edited by hand;
2. authored command YAML does not contain DTO field definitions;
3. generated `command-index.json` is sufficient for downstream command
   discovery;
4. executor command/output DTO changes affecting KV/vector require updating the
   IDL command coverage test;
5. no CLI/SDK/MCP/OpenAPI/docs generation code is added in this slice.

## Slice 1b Packaging Tests

After Slice 1 is generated, Slice 1b moves ownership into `executor-next`.

Tests:

1. workspace members do not include `crates/idl-next`;
2. `crates/idl-next` does not exist;
3. authored IDL files live under `crates/executor-next/idl/v1`;
4. generated command index lives under
   `crates/executor-next/idl/v1/generated/command-index.json`;
5. `strata-executor-next` exposes a `strata-idl` dev binary;
6. `cargo run -p strata-executor-next --features idl-tooling --bin strata-idl -- generate`
   succeeds;
7. `cargo run -p strata-executor-next --features idl-tooling --bin strata-idl -- check`
   succeeds;
8. normal executor build/test targets pass without the `idl-tooling` feature;
9. YAML/prose parser dependencies are feature-gated or otherwise isolated from
   normal executor runtime builds;
10. generated `source.command` paths point at
    `crates/executor-next/idl/v1/...`;
11. command-index semantic content is unchanged except for expected source-path
    relocation;
12. KV/vector IDL tests live under `strata-executor-next`, not under a
    standalone IDL crate.

Exit criteria:

1. executor-next is the only package that owns public-boundary IDL tooling;
2. no separate IDL crate remains;
3. downstream CLI/SDK/MCP/docs work can consume the executor-owned generated
   artifact;
4. normal executor runtime code does not load authored YAML/prose.

## Fixture Requirements

Add missing KV/vector fixtures under:

```text
crates/executor-next/tests/fixtures/requests/v1/
crates/executor-next/tests/fixtures/responses/v1/
```

Minimum fixture categories:

1. KV get found/missing;
2. KV write/delete applied and missing/no-op;
3. KV list/scan/sample pages;
4. KV batch success and partial failure;
5. vector collection list/stats;
6. vector get found/missing;
7. vector upsert/delete/update metadata;
8. vector keys/count/exists;
9. vector query;
10. vector index query diagnostics;
11. vector batch success and partial failure;
12. representative KV/vector failure.

If a fixture already exists under a stable name, the IDL should reference that
fixture instead of duplicating it.

## CI Targets

Add or extend CI to run:

1. executor-owned IDL source validation;
2. generated artifact freshness check through
   `cargo run -p strata-executor-next --features idl-tooling --bin strata-idl -- check`;
3. KV/vector command coverage check;
4. fixture existence check;
5. DTO reference check;
6. error reference check;
7. executor-next golden response tests.

The first slice does not run CLI, SDK, MCP, OpenAPI, or docs generation.

## Performance And Agent-Cost Checks

Tests should assert that the generated command index is sufficient for
downstream consumers.

Suggested checks:

1. command index lookup by ID is direct or easily indexed;
2. generated artifact contains prose, docs links, fixtures, errors, and source
   paths;
3. no downstream consumer needs to traverse authored YAML and Markdown to
   answer command-discovery questions.

## Completion Criteria

The KV/vector IDL slice is test-complete when:

1. every scoped KV command has valid resolved metadata;
2. every scoped vector command has valid resolved metadata;
3. every scoped command has canonical prose;
4. every scoped command has fixtures;
5. generated output is deterministic and fresh;
6. generated entries are explain-ready;
7. bad references fail fast with actionable diagnostics;
8. no CLI/SDK/MCP/OpenAPI/docs generation is introduced.

## Follow-Up Test Plan

After this plan passes, create:

```text
docs/architecture/implementation-plans/v1-idl-overlay-cli-generation-test-plan.md
```

That plan should verify CLI help, command listing, and `strata explain` against
the resolved `command-index.json`.
