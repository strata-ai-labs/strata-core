# V1 IDL Overlay CLI Generation Test Plan

## Status

Implemented for generated CLI metadata checks, runtime metadata loading,
`strata commands`, `strata explain`, help rendering, source/dependency guards,
and golden output fixtures. Full command execution, REPL, SDK, MCP, OpenAPI,
and long-form documentation tests remain deferred.

Implementation plan:
`docs/architecture/implementation-plans/v1-idl-overlay-cli-generation-implementation-plan.md`

## Goal

Verify Slice 2 of the V1 IDL overlay: CLI command discovery, help, and explain
behavior generated from the executor-owned resolved IDL.

This test plan proves that:

1. CLI metadata is generated from `command-index.json`;
2. generated CLI artifacts are deterministic and fresh;
3. `strata explain` and command listing use generated metadata;
4. runtime CLI code does not read authored YAML or prose;
5. KV/vector help facts stay consistent with the IDL;
6. no full SDK, MCP, OpenAPI, docs, or executor command parser generation is
   introduced in this slice.

## Related Documents

1. `docs/architecture/v1-idl-overlay-strategy.md`
2. `docs/architecture/implementation-plans/v1-idl-overlay-cli-generation-implementation-plan.md`
3. `docs/architecture/implementation-plans/v1-idl-overlay-kv-vector-implementation-plan.md`
4. `docs/architecture/implementation-plans/v1-idl-overlay-kv-vector-test-plan.md`
5. `docs/architecture/implementation-plans/cli-next-test-plan.md`
6. `docs/product/strata-v1-cli-sdk-experience.md`

## Test Scope

In scope:

1. CLI artifact generation from the resolved command index.
2. CLI artifact freshness checks.
3. CLI metadata runtime loading.
4. Command lookup by ID and CLI path.
5. Command family listing.
6. `strata explain` human output.
7. `strata explain --format json` output.
8. Help rendering adapters.
9. Unknown command suggestions.
10. Dependency and source guards.
11. CI-agent regeneration workflow.

Out of scope:

1. Full CLI command execution.
2. REPL tests.
3. Durable/cache database open behavior.
4. Real executor command invocation.
5. SDK generation.
6. MCP generation.
7. OpenAPI generation.
8. Long-form docs generation.
9. Non-KV/non-vector command families.

## Required Command Coverage

Every KV and vector command in
`crates/executor-next/idl/v1/generated/command-index.json` must appear in the
generated CLI artifact.

Required KV IDs:

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

Required vector IDs:

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

## Generator Tests

### Source Boundary

Tests:

1. `generate-cli` reads `generated/command-index.json`.
2. `generate-cli` does not read authored `commands/*.yaml`.
3. `generate-cli` does not read authored `prose/**/*.md`.
4. `generate-cli` fails if `command-index.json` is missing.
5. `generate-cli` fails if `command-index.json` is malformed.
6. `generate-cli` fails if `command-index.json` has an unsupported generator
   version.
7. `generate-cli` reports the source path used for generation.

Exit criteria:

1. CLI generation has one machine input.
2. Runtime CLI and CLI generator do not duplicate the Slice 1 resolver.

### Artifact Shape

Tests:

1. generated CLI artifact has a generator version;
2. generated CLI artifact has a source checksum;
3. generated CLI artifact has a command count;
4. generated CLI artifact has family groups;
5. generated CLI artifact has command entries sorted by CLI path;
6. every command entry has `id`;
7. every command entry has `path`;
8. every command entry has `family`;
9. every command entry has `title`, `summary`, and `description`;
10. every command entry has `docs`;
11. every command entry has `access`, `commit`, `pagination`, and `batch`;
12. every command entry has `input`, `outputs`, and `response_model`;
13. every command entry has public error codes;
14. every command entry has `wire_status`;
15. no command entry contains request or response field schemas.

Exit criteria:

1. the CLI artifact is sufficient for explain/list/help;
2. the CLI artifact remains a metadata overlay, not a second schema.

### Determinism And Freshness

Tests:

1. two consecutive `generate-cli` runs produce byte-identical output;
2. `check-cli` or `strata-idl check` fails when the CLI artifact is stale;
3. changing a command summary changes the generated CLI artifact;
4. changing a command path changes lookup tables deterministically;
5. changing only unrelated files does not change the generated CLI artifact;
6. generated JSON is pretty-printed and stable;
7. generated artifact contains no wall-clock timestamp.

Exit criteria:

1. CI can safely regenerate artifacts;
2. generated output is reviewable in diffs.

### Duplicate And Missing Reference Guards

Tests:

1. duplicate command IDs fail generation;
2. duplicate CLI paths fail generation;
3. missing title fails generation;
4. missing summary fails generation;
5. missing description fails generation;
6. missing docs link fails generation;
7. missing input DTO name fails generation;
8. missing output DTO name fails generation;
9. missing response model fails generation;
10. missing public error code list fails generation.

Exit criteria:

1. broken command metadata does not reach CLI runtime;
2. users never see partial help for a command.

## Runtime Metadata Tests

### Loader

Tests:

1. loader can parse the generated CLI artifact;
2. loader exposes all KV/vector commands;
3. loader exposes family groups;
4. loader exposes sorted command listing;
5. loader exposes lookup by command ID;
6. loader exposes lookup by CLI path tokens;
7. loader rejects malformed embedded metadata in tests;
8. loader does not require `idl-tooling`.

Exit criteria:

1. runtime CLI metadata is independent of authoring dependencies;
2. command lookup is fast and deterministic.

### Command Lookup

Tests:

1. `kv.put` resolves by ID;
2. `kv put` resolves by CLI path;
3. `kv.batch_get` resolves by ID;
4. `kv batch-get` or the chosen generated path resolves by CLI path;
5. `vector.query` resolves by ID;
6. `vector query` resolves by CLI path;
7. `vector.collection.create` resolves by ID;
8. `vector collection create` resolves by CLI path;
9. unknown command ID returns structured not-found data;
10. unknown CLI path returns structured not-found data with suggestions.

Exit criteria:

1. users can explain commands using stable IDs or shell-like paths;
2. command lookup does not require command execution parsing.

### Family Listing

Tests:

1. list all commands returns KV and vector groups;
2. groups are sorted deterministically;
3. commands inside a group are sorted by CLI path;
4. `--family kv` returns only KV commands;
5. `--family vector` returns only vector commands;
6. unknown family returns a structured CLI error;
7. transitional wire-status commands are marked in JSON output;
8. hidden or deferred commands are not listed.

Exit criteria:

1. command listing is useful for humans and agents;
2. listing output can be consumed without command-specific logic.

## CLI Behavior Tests

### `strata commands`

Human output tests:

1. prints `kv` group;
2. prints `vector` group;
3. includes command path;
4. includes one-line summary;
5. does not include long descriptions by default;
6. does not require a database path;
7. does not create `~/.strata`;
8. exits with code 0.

JSON output tests:

1. emits valid JSON;
2. includes generator version;
3. includes family groups;
4. includes command entries;
5. includes command IDs and paths;
6. includes access, commit, pagination, and batch facts;
7. includes docs links;
8. exits with code 0.

Exit criteria:

1. humans can discover commands from the CLI;
2. agents can discover commands from JSON output.

### `strata explain`

Human output tests:

1. `strata explain kv.put` succeeds;
2. `strata explain kv put` succeeds;
3. `strata explain vector.query` succeeds;
4. `strata explain vector query` succeeds;
5. output includes title;
6. output includes summary;
7. output includes description;
8. output includes CLI path;
9. output includes docs link;
10. output includes read/write access;
11. output includes commit behavior;
12. output includes pagination behavior;
13. output includes batch behavior;
14. output includes response model;
15. output includes public error codes.

JSON output tests:

1. emits valid JSON;
2. includes the stable command ID;
3. includes CLI path tokens;
4. includes all required explain facts;
5. preserves public error codes as strings;
6. preserves fixture hints if included;
7. matches a golden fixture.

Exit criteria:

1. `strata explain` is complete enough for humans;
2. `strata explain --format json` is complete enough for agents.

### Unknown Command Errors

Tests:

1. `strata explain nope` exits nonzero;
2. unknown error includes stable code
   `invalid_argument.cli.command_unknown`;
3. unknown error includes entered selector;
4. unknown error includes nearest command ID suggestions;
5. unknown error includes nearest CLI path suggestions;
6. unknown error includes command discovery docs link;
7. unknown command does not open a database;
8. unknown command does not fall back to old CLI command handling.

Exit criteria:

1. mistakes are actionable;
2. unsupported commands cannot accidentally execute old behavior.

### `strata-idl explain` Guard

Tests:

1. `strata-idl` has `generate`, `check`, and CLI artifact generation commands;
2. `strata-idl explain` is not a user-facing explain path;
3. the docs and generated help point users to `strata explain`;
4. no generated CLI fixture contains `strata-idl explain`.

Exit criteria:

1. developer tooling and user CLI stay separate.

## Help Adapter Tests

Tests:

1. top-level help can render command groups from generated metadata;
2. KV family help can render KV command summaries;
3. vector family help can render vector command summaries;
4. command help can render `kv.put` facts;
5. command help can render `vector.query` facts;
6. help text uses the same summary as `strata explain`;
7. help text uses the same docs link as `strata explain`;
8. help text marks pagination and batch facts consistently;
9. help adapter does not require command execution parser generation.

Exit criteria:

1. help and explain cannot drift for KV/vector commands;
2. full CLI parser generation can build on the same adapter later.

## Golden Snapshot Tests

Add fixtures for:

1. `strata commands` human output;
2. `strata commands --format json`;
3. `strata commands --family kv` human output;
4. `strata commands --family vector` human output;
5. `strata explain kv.put` human output;
6. `strata explain kv.put --format json`;
7. `strata explain vector.query` human output;
8. `strata explain vector.query --format json`;
9. unknown command error JSON;
10. generated `cli-command-index.json`.

Rules:

1. human fixtures should be stable and concise;
2. JSON fixtures should be pretty-printed;
3. fixtures should not include wall-clock timestamps;
4. fixture names should map to CLI model names;
5. public shape changes must update fixtures in the same commit.

Exit criteria:

1. CI catches accidental CLI output drift;
2. generated CLI output is reviewable.

## Dependency And Source Guard Tests

### Runtime Dependency Guards

Tests:

1. shipped CLI runtime does not depend on `serde_yaml`;
2. shipped CLI runtime does not depend on a frontmatter parser;
3. shipped CLI runtime does not enable executor `idl-tooling`;
4. shipped CLI runtime does not depend on old `strata_executor`;
5. shipped CLI runtime does not depend on old `strata_engine`;
6. shipped CLI runtime does not depend on old `strata_storage`;
7. shipped CLI runtime does not import executor IDL tooling modules.

Exit criteria:

1. authoring dependencies stay out of the product binary;
2. the CLI remains generated-data-driven.

### Source Guards

Tests:

1. runtime CLI source does not read `commands/*.yaml`;
2. runtime CLI source does not read `prose/**/*.md`;
3. runtime CLI source does not parse YAML;
4. runtime CLI source does not parse Markdown frontmatter;
5. runtime CLI source does not contain duplicated KV/vector long descriptions
   outside generated artifacts;
6. generated files contain a header that says they are generated;
7. hand-editing generated files without changing source is caught by freshness
   tests.

Exit criteria:

1. command language has one source of truth;
2. generated CLI artifacts are not maintained manually.

## CI Tests

Required CI additions:

1. run `strata-idl check`;
2. run CLI artifact freshness check;
3. run CLI generation tests;
4. run CLI explain/listing golden tests;
5. run dependency guards;
6. run source guards.

Suggested commands after implementation:

```sh
cargo run -p strata-executor-next --features idl-tooling --bin strata-idl -- check
cargo run -p strata-executor-next --features idl-tooling --bin strata-idl -- generate-cli --check
cargo test -p strata-executor-next --features idl-tooling --test idl_kv_vector_overlay
cargo test -p strata-cli-next --test idl_cli_generation
cargo test -p strata-cli-next --test idl_cli_explain
cargo clippy -p strata-cli-next --all-targets -- -D warnings
```

If `strata-cli-next` is not created in this slice, equivalent tests should live
under the executor-owned IDL tooling tests until the CLI crate exists.

Exit criteria:

1. CI fails when CLI artifacts are stale;
2. CI fails when CLI output drifts accidentally;
3. CI fails when runtime code starts reading authored IDL sources.

## Edge Cases

Tests:

1. command ID and CLI path both resolve to the same command;
2. dotted command IDs with nested paths resolve correctly;
3. hyphenated CLI paths resolve correctly;
4. underscores in command IDs do not leak into shell paths unless explicitly
   chosen;
5. duplicate aliases are rejected;
6. command summaries with punctuation render cleanly;
7. long descriptions wrap or remain readable in human output;
8. docs URLs are stable and non-empty;
9. transitional wire shapes are visible but not scary in human output;
10. empty command families are not rendered;
11. unknown family gives suggestions;
12. missing generated artifact fails deterministically in tests;
13. generated artifact with unknown command kind fails loader validation;
14. generated artifact with unknown pagination mode fails loader validation;
15. generated artifact with unknown batch mode fails loader validation.

## Acceptance Criteria

1. CLI metadata for all KV/vector commands is generated from
   `command-index.json`.
2. `strata commands` and `strata explain` can be implemented without reading
   authored YAML or prose.
3. Human and JSON output have golden fixtures.
4. Unknown command errors are structured and suggest valid commands.
5. Runtime CLI code has no authoring dependencies.
6. CI can regenerate and check CLI artifacts deterministically.
7. The slice does not introduce SDK, MCP, OpenAPI, docs, REPL, or full command
   execution generation.
