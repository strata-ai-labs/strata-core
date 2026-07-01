# V1 IDL Overlay CLI Generation Implementation Plan

## Status

Implemented for generated CLI metadata, runtime metadata loading, `strata
commands`, `strata explain`, help rendering, and CI freshness/behavior guards.
Full command execution parsing, REPL integration, SDK generation, MCP
generation, and long-form docs generation remain deferred.

Companion test plan:
`docs/architecture/implementation-plans/v1-idl-overlay-cli-generation-test-plan.md`

## Goal

Implement Slice 2 of the V1 IDL overlay: generate or wire CLI command
discovery, help, and explain behavior from the executor-owned resolved IDL.

The target flow is:

```text
executor-next DTOs
  -> executor-owned authored IDL and prose
  -> generated command-index.json
  -> generated CLI command metadata
  -> strata explain / command listing / help text
```

This slice proves that the CLI can be built from the same resolved command
artifact that future SDK, MCP, and docs generators will consume.

## Related Documents

1. `docs/architecture/v1-idl-overlay-strategy.md`
2. `docs/architecture/implementation-plans/v1-idl-overlay-kv-vector-implementation-plan.md`
3. `docs/architecture/implementation-plans/v1-idl-overlay-kv-vector-test-plan.md`
4. `docs/architecture/implementation-plans/cli-next-implementation-plan.md`
5. `docs/product/strata-v1-cli-sdk-experience.md`
6. `docs/architecture/v1-error-and-diagnostics-contract.md`

## Slice Boundary

This slice is intentionally narrower than the full `cli-next` product
milestone.

In scope:

1. CLI command metadata generated from
   `crates/executor-next/idl/v1/generated/command-index.json`.
2. `strata explain <command>` behavior for KV and vector commands.
3. Command listing and grouping for KV and vector commands.
4. Help text facts for KV and vector commands.
5. CI checks that generated CLI artifacts are fresh.
6. Guardrails that the shipped CLI does not read authored YAML or prose.

Out of scope:

1. Full one-shot command execution parsing.
2. REPL implementation.
3. Database open/path resolution.
4. SDK, MCP, OpenAPI, or long-form docs generation.
5. JSON, event, graph, branch, space, admin, Arrow, or inference command
   families.
6. Changing executor command or response wire shapes.

## Product Decision

The user-facing command is `strata explain`, not `strata-idl explain`.

`strata-idl` remains a developer tool for generating and checking IDL-derived
artifacts. Users and agents should discover commands through the Strata CLI:

```sh
strata explain kv.put
strata explain kv put
strata commands
strata commands --family vector
```

If old and new CLI binaries coexist during development, the implementation may
use a temporary binary name internally. User-facing help, generated metadata,
fixtures, and documentation should still describe the production command as
`strata`.

## Source Of Truth

The CLI generator must read the resolved command index, not authored YAML or
Markdown:

```text
crates/executor-next/idl/v1/generated/command-index.json
```

The generated index is already responsible for resolving:

1. command IDs;
2. CLI paths;
3. titles, summaries, descriptions, and docs links;
4. family and operation groups;
5. access mode;
6. commit behavior;
7. pagination behavior;
8. batch behavior;
9. input and output DTO names;
10. response models;
11. public error codes;
12. fixture paths.

The CLI layer must not reimplement YAML inheritance, prose loading, placeholder
resolution, or error-code merging.

## Generated Artifacts

Add a generated CLI artifact derived from `command-index.json`.

Recommended initial shape:

```text
crates/executor-next/idl/v1/generated/cli-command-index.json
```

This artifact should be optimized for CLI runtime and test consumers. It may be
checked into the repository so CI agents can update it deterministically.

The artifact should contain:

1. generator version;
2. source `command-index.json` checksum;
3. command count;
4. family groups;
5. command entries sorted by CLI path;
6. lookup table by command ID;
7. lookup table by CLI path;
8. help-rendering facts;
9. explain-rendering facts;
10. stable docs links;
11. stable fixture references;
12. generation timestamp omitted, or set only when deterministic.

Each CLI command entry should include:

1. `id`;
2. `path`;
3. `family`;
4. `op`;
5. `title`;
6. `summary`;
7. `description`;
8. `docs`;
9. `feature`;
10. `access`;
11. `commit`;
12. `pagination`;
13. `batch`;
14. `input`;
15. `outputs`;
16. `response_model`;
17. `errors`;
18. `wire_status`.

Do not duplicate request/response field schemas in the CLI artifact.

## Runtime Consumption

The shipped CLI should consume generated CLI metadata only.

Acceptable runtime options:

1. Embed `cli-command-index.json` with `include_str!`.
2. Generate a Rust module from `cli-command-index.json` and compile it into the
   CLI.
3. Keep both JSON and Rust forms if the JSON artifact is useful for external
   agents and the Rust module is useful for runtime speed.

The first implementation should prefer the simplest checked-in artifact that
keeps runtime dependencies small. The runtime CLI must not depend on
`serde_yaml`, frontmatter parsers, or the executor IDL generator feature.

## Developer And CI Agent Workflow

Human edit flow:

```text
edit executor DTOs when needed
edit crates/executor-next/idl/v1 authored YAML/prose
run strata-idl generate
run strata-idl generate-cli
run tests
commit authored and generated changes together
```

CI agent flow:

```text
detect changed IDL/prose/fixtures
run strata-idl generate
run strata-idl generate-cli
run strata-idl check
run CLI generation tests
open or update a PR with generated artifacts
```

Freshness checks must fail if:

1. authored IDL changed without regenerated `command-index.json`;
2. `command-index.json` changed without regenerated CLI artifacts;
3. generated artifacts are not deterministic;
4. CLI artifacts contain references to missing command IDs or fixtures.

## CLI Commands

### `strata commands`

List available commands from generated metadata.

Required behavior:

1. groups commands by family;
2. sorts by CLI path;
3. supports `--family <family>`;
4. supports `--format json`;
5. marks transitional wire shapes when present;
6. does not require a database;
7. does not initialize `~/.strata`.

Initial family scope:

1. `kv`;
2. `vector`.

### `strata explain <command>`

Explain one command from generated metadata.

The command selector should accept:

1. stable command ID: `kv.put`;
2. CLI path tokens: `kv put`;
3. dotted vector IDs: `vector.collection.create`;
4. CLI path tokens for nested paths: `vector collection create`.

Required output facts:

1. title;
2. summary;
3. description;
4. CLI usage path;
5. docs link;
6. access mode;
7. commit behavior;
8. pagination behavior;
9. batch behavior;
10. input DTO;
11. output DTOs;
12. response model;
13. public error codes;
14. fixture hints when useful in JSON mode.

Human output should be concise and stable. JSON output should expose the full
generated command entry or a stable explain DTO.

### Help Text

Generated CLI metadata should support future `--help` rendering.

This slice should at least add a help renderer or metadata adapter that can
produce:

1. top-level command groups;
2. family help;
3. command summary;
4. command docs link;
5. command facts such as read/write, commit behavior, pagination, and batch
   mode.

If the full CLI parser does not exist yet, keep this as an internal rendering
API plus golden tests.

## Unknown Command Handling

Unknown command explanations should return a structured CLI error with:

1. stable code such as `invalid_argument.cli.command_unknown`;
2. entered command selector;
3. nearest command ID suggestions;
4. nearest CLI path suggestions;
5. docs link for command discovery.

Do not silently fall back to old CLI commands. Do not forward unknown explain
commands to executor command execution.

## Implementation Slices

### 2A. CLI Artifact Generator

1. Extend executor-owned IDL tooling with a `generate-cli` command.
2. Read only `generated/command-index.json`.
3. Produce deterministic `generated/cli-command-index.json`.
4. Include source checksum and generator version.
5. Reject duplicate CLI paths and duplicate command IDs even if the previous
   resolver already checked them.
6. Reject entries missing facts required by CLI help or explain.
7. Add a `check-cli` or include CLI freshness in the existing `check` command.

Exit criteria:

1. CLI artifact generation is deterministic.
2. CLI artifact generation does not read authored YAML or prose.
3. Stale CLI artifacts fail CI.

### 2B. CLI Metadata Runtime

1. Add a small runtime metadata loader for generated CLI artifacts.
2. Keep the loader dependency-light.
3. Provide lookups by command ID and CLI path.
4. Provide family grouping and sorted listing.
5. Provide unknown-command suggestion helpers.

Exit criteria:

1. Runtime lookup does not require IDL tooling features.
2. KV/vector command IDs and paths resolve consistently.
3. Missing generated artifacts fail at build time or in tests, not at user
   runtime.

### 2C. `strata commands` And `strata explain`

1. Wire command listing to the generated CLI metadata.
2. Wire explain output to the generated CLI metadata.
3. Support human output and JSON output.
4. Ensure these commands are local CLI actions that do not open a database.
5. Ensure `strata-idl explain` is not introduced.

Exit criteria:

1. `strata explain kv.put` works.
2. `strata explain kv put` works.
3. `strata commands --family vector` works.
4. These commands do not create or open a database.

### 2D. Help Adapter

1. Add an adapter that converts generated command facts into help sections.
2. Keep the adapter independent from command execution parsing.
3. Prove top-level, family, and command-level help can be rendered from the
   generated data.

Exit criteria:

1. CLI help metadata comes from the IDL artifact.
2. Help text stays consistent with `strata explain`.
3. No manually maintained duplicate command descriptions are needed for KV and
   vector.

### 2E. CI Integration

1. Add CI steps for CLI artifact freshness.
2. Add CLI explain/listing golden tests.
3. Add source guard tests that prevent runtime CLI code from reading authored
   YAML or prose.
4. Add dependency guard tests that prevent YAML/frontmatter dependencies from
   leaking into the shipped CLI runtime.

Exit criteria:

1. CI fails on stale CLI artifacts.
2. CI fails on accidental duplicate command help.
3. CI fails if the runtime CLI depends on IDL authoring dependencies.

## Acceptance Criteria

1. KV and vector command explain/list/help metadata is generated from the
   resolved executor-owned IDL.
2. The CLI runtime consumes generated artifacts only.
3. `strata explain` and command listing do not require a database.
4. CLI output can answer:
   - what command to use;
   - whether it reads or writes;
   - whether success commits;
   - whether the response is paginated;
   - whether the response is batched;
   - what response model to expect;
   - what public errors can occur.
5. Generated CLI artifacts are deterministic and checked in.
6. CI can be used by an agent to regenerate artifacts and catch stale output.
7. No SDK, MCP, OpenAPI, or full docs generation is introduced in this slice.

## Deferred Work

1. Full command execution parser generation.
2. REPL integration.
3. Database open/path resolution.
4. CLI output rendering for real executor responses.
5. Remaining command families.
6. TypeScript SDK generation.
7. Python SDK generation.
8. MCP tool metadata generation.
9. Long-form documentation generation.
