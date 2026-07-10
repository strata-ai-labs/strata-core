# CLI-Next Test Plan

Status: Draft test plan
Implementation plan: [cli-next-implementation-plan.md](cli-next-implementation-plan.md)

## Goal

Prove that `cli-next` is a clean V1 CLI over `executor-next`, not a port of old engine/session behavior.

The test suite should verify:

- The crate is dependency-clean.
- The parser maps CLI input to `executor-next` command variants exactly.
- Cache and durable-local execution work through normal executor APIs.
- REPL, pipe, and one-shot modes share the same parser and execution path.
- First-time setup, database creation, local AI setup, and script usage match the product pathways.
- Old non-V1 surfaces are rejected deliberately.

## Test Layers

Use focused tests at several layers:

- Parser unit tests for global flags and command-group mapping.
- Path/open resolver unit tests for cache, durable, current-directory, and missing-path behavior.
- Renderer unit tests for human, JSON, and raw output modes.
- REPL and pipe tests using scripted input.
- Integration tests against `Executor::open_cache()`.
- Durable-local integration tests in temp directories.
- Source/dependency guard tests.
- Optional external binary smoke tests once the crate is packaged.

## Source And Dependency Guards

Add a guard test or xtask check that fails if `crates/cli-next` imports old runtime boundaries:

- `strata_executor`
- `strata_engine`
- `strata_storage`
- `strata_intelligence`
- old `Session`
- old `StrataConfig`
- old `IpcServer`

Add a parser-surface guard that fails if these old command names become accepted top-level commands:

- `flush`
- `compact`
- `begin`
- `commit`
- `rollback`
- `txn`
- `up`
- `down`
- `uninstall`
- `follower`

The guard should allow those words in documentation and comments only when they are explicitly listed as exclusions.

## Crate And Feature Tests

Required checks:

- `cargo check -p strata-cli-next`
- `cargo test -p strata-cli-next`
- `cargo clippy -p strata-cli-next --all-targets`
- Workspace check after the crate is added.

Feature checks:

- Default build includes executor, Arrow commands, and cloud inference command parsing.
- Local inference feature compiles only when native local inference prerequisites are enabled.
- No-default-features build should either compile a documented minimal shell or be explicitly unsupported in `Cargo.toml`.

## Global Parser Tests

Cover:

- `strata-next --help`
- `strata-next --version`
- `strata-next --output json ...`
- `strata-next --output human ...`
- `strata-next --output raw ...`
- `strata-next --db <path> ...`
- `strata-next --cache ...`
- `strata-next <path>`
- `strata-next init`
- `strata-next new <path>`
- `strata-next --profile small ...`
- `strata-next --memory-budget 64MiB ...`

Reject:

- `strata-next --cache <path>`
- `strata-next --cache --db <path>`
- one-shot commands with no database unless the command is local or explicitly supports cache
- unknown old CLI commands

## Command Mapping Tests

Each command-group parser should produce an exact `strata_executor_next::Command` value.

Required groups:

- Branch: list, get, create, fork, fork-at-version, fork-at-timestamp, delete.
- KV: put, get, delete, exists, list, scan, count, sample, history, batch-put, batch-get, batch-delete, batch-exists.
- JSON: set, get, delete, exists, list, count, sample, history, batch-set, batch-get, batch-delete, index create, index drop, index list.
- Vector: create, drop, list, stats, count, upsert, get, delete, query, list-keys, update-metadata, delete-by-filter, delete-all, batch-upsert, batch-get, batch-delete, exists, history.
- Event: append, batch-append, get, range, range-time, list, list-types, len, exists, verify-chain, get-by-type.
- Graph core: create, delete, list, info, add-node, get-node, remove-node, list-nodes, add-edge, get-edge, remove-edge, neighbors, bindings-for-entity, batch-write.
- Arrow: import, export.
- Inference: models list, models local, models pull, generate, embed, embed-batch, rank, tokenize, detokenize, unload, cache-status, capability.

For JSON-like input arguments, test both inline JSON and `@file` inputs.

For binary or byte-oriented arguments, test literal strings and file input without changing executor semantics.

## Output Rendering Tests

For representative executor outputs, verify:

- JSON mode emits valid JSON and preserves all fields.
- Human mode renders concise, stable text.
- Raw mode returns bytes/string payloads without decoration where applicable.
- Error rendering includes a stable code or category when the executor supplies one.
- Unsupported old commands produce a clear unsupported-command error.

## Open And Path Tests

Use temporary directories.

Cache:

- `--cache kv put a b` succeeds.
- Cache state is visible for subsequent commands in the same REPL process.
- Cache state does not persist across processes.

Durable:

- `new <path>` creates a durable database.
- `--db <path> kv put a b` writes data.
- Reopening `<path>` can read the data.
- Opening a missing `<path>` without `new` fails with creation guidance.
- Opening a non-database directory fails with clear guidance.

Current directory:

- Running inside a database root opens it.
- Running outside any database does not silently create one.

Read-only:

- If read-only open is supported, read commands succeed and writes fail.
- If not supported yet, the parser rejects the flag with an explicit not-supported error.

## REPL And Pipe Tests

REPL tests:

- `help` renders command groups.
- `quit` and `exit` close cleanly.
- `clear` is handled locally.
- `use <branch>` validates the branch before changing context.
- `use <branch>/<space>` updates both fields after validation.
- Prompt includes branch and space but not transaction state.
- Command failures do not terminate the REPL.

Pipe tests:

- Multiple commands from stdin execute in order.
- Blank lines and comments are ignored.
- A failing command records failure and the process exits nonzero after the pipe completes.
- Pipe mode and one-shot mode render the same output for the same command.

## Init And New Tests

Use a temp home directory for all init tests.

`init`:

- Creates `~/.strata`.
- Writes machine-level profile/configuration files only.
- Does not create a database.
- Does not download models unless an explicit model setup flag or prompt response requests it.
- Can run non-interactively with safe defaults.
- Re-running is idempotent and preserves explicit user choices.

`new`:

- Creates a durable-local database at the requested path.
- Applies explicit profile and memory-budget values.
- Does not perform hidden network work.
- Fails if the path already contains a non-Strata directory unless `--force` is explicitly supported.

## Local AI And Cloud Provider Tests

No test should require real API keys by default.

Default tests:

- Cloud inference command parsing is available by default.
- Missing credentials produce a clear runtime error only when a provider command is executed.
- `models local` and `inference cache-status` work without a database.
- `models pull` is explicit and never triggered by `init` or `new` unless requested.

Integration tests gated by environment:

- OpenAI request smoke test when `OPENAI_API_KEY` and an opt-in env var are set.
- Anthropic request smoke test when `ANTHROPIC_API_KEY` and an opt-in env var are set.
- Google request smoke test when `GOOGLE_API_KEY` and an opt-in env var are set.
- Local GGUF smoke test only on workstations with the local inference feature and model path configured.

## Primitive Integration Tests

Run the same minimal scenario in cache and durable-local modes:

- Create branch, switch context, write KV, read KV.
- JSON set/get/delete and batch-set.
- Event append/range/verify.
- Vector create/upsert/query/list-keys/update-metadata/delete.
- Graph create/add-node/add-edge/neighbors/remove.
- Arrow export/import round trip for at least one primitive.
- Inference non-network commands where available.

Each scenario should assert both command success and observable persisted state where durable-local applies.

## Memory Budget Tests

After the runtime memory-budget opening contract exists:

- `--memory-budget` parses human sizes.
- Invalid sizes fail before opening the executor.
- The parsed budget is passed into the executor/runtime opening options.
- Diagnostics show the effective memory budget.
- A small-budget durable database can still execute simple KV and branch operations.

## External Compatibility Suite

When `cli-next` is ready for packaging, add an external binary smoke suite:

- Install or build `strata-next`.
- Run first-time `init` in a temp home.
- Create a database with `new`.
- Run one command from each primitive group.
- Open the same database in REPL mode and execute a scripted sequence.
- Run `--cache` smoke commands.

Do not migrate old external CLI tests for excluded surfaces. Delete or rewrite tests for old manual maintenance, product transactions, follower mode, daemon control, old search/recipe, and old graph ontology/analytics.

## Acceptance Criteria

- Every V1 command group has parser mapping coverage.
- Cache and durable-local integration tests pass through the normal executor API.
- `init`, `new`, REPL, one-shot, pipe, local AI, and cache pathways are covered.
- Old CLI dependency guards pass.
- Old excluded command guards pass.
- JSON output is stable enough for Python, Node, MCP, and shell integrations.
- No benchmark-only or lower-layer bypass appears in the CLI.
