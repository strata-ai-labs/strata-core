# CLI-Next Implementation Plan

Status: Draft implementation plan
Companion test plan: [cli-next-test-plan.md](cli-next-test-plan.md)

## Objective

Create a new `cli-next` crate that exposes the V1 Strata command-line experience on top of `executor-next`.

The CLI should be a thin, stateless command delegator over the executor layer. It should preserve the useful shell mechanics from the old CLI where they still fit, but it must not carry over old engine-specific administration surfaces, product transaction sessions, or storage maintenance controls.

The first production shape should support:

- First-time machine setup through `strata-next init`.
- New durable database creation through `strata-next new <path>`.
- Interactive REPL usage through `strata-next` or `strata-next <path>`.
- Scriptable one-shot commands through `strata-next --db <path> <primitive> ...`.
- Explicit in-memory usage through `strata-next --cache <primitive> ...`.
- The current V1 primitive surface implemented by `executor-next`: branch, KV, JSON, vector, event, graph core, Arrow import/export, and inference.

## Product Anchors

- [strata-v1-cli-sdk-experience.md](../../product/strata-v1-cli-sdk-experience.md)
- [strata-v1-user-pathways.md](../../product/strata-v1-user-pathways.md)
- [runtime-memory-budget-implementation-plan.md](runtime-memory-budget-implementation-plan.md)
- [runtime-memory-budget-test-plan.md](runtime-memory-budget-test-plan.md)
- [v1-error-and-diagnostics-contract.md](../v1-error-and-diagnostics-contract.md)
- [testing-and-conformance-plan.md](../engine-next/testing-and-conformance-plan.md)

## Current Evidence

The old CLI is useful as a source of shell behavior, not as an API boundary to preserve directly.

- `crates/cli/src/app.rs` shows the existing REPL, pipe, and one-shot execution loop, but it is coupled to old `strata_executor::{Command, Session, Strata}`.
- `crates/cli/src/open.rs` shows the old path/default-open shape, but it uses old `Strata::cache()` and `Strata::open_with`.
- `crates/cli/src/context.rs` tracks branch, space, and old product transaction state. The branch/space behavior is useful; the transaction prompt/state should not be ported.
- `crates/cli/src/repl.rs` and `crates/cli/src/render.rs` are good conceptual references for terminal flow and output rendering.
- `crates/cli/src/parse.rs` exposes many old commands that are outside the V1 boundary: manual storage maintenance, product transactions, old search/recipe/admin/config surfaces, and old graph ontology/analytics.
- `crates/cli/src/init.rs` contains useful first-run ideas: hardware detection, storage detection, profile selection, and optional model setup. It should be rewritten against the new runtime/profile contracts.
- `crates/cli/src/admin.rs` implements old IPC daemon management. That should not be ported in this slice.
- `crates/executor-next/src/command.rs` is the source of truth for the serialized command contract.
- `crates/executor-next/src/executor.rs` is the source of truth for opening cache and durable-local execution.

## Crate Shape

Add a new crate:

- Path: `crates/cli-next`
- Package name: `strata-cli-next`
- Initial binary name: `strata-next`

The initial binary name avoids collision with the old `strata` CLI while both crates coexist. The final cutover can rename the binary after old CLI retirement and the planned codebase sweep for `-next` naming.

Dependencies should be intentionally narrow:

- `strata-executor-next`
- `anyhow` or the workspace error convention used by adjacent crates
- `clap`
- `serde`
- `serde_json`
- `rustyline`
- `shlex`
- `tracing`
- `dirs` or existing workspace path helpers if already standardized

Do not depend on old executor, old engine, old storage, old inference/intelligence, or old CLI modules.

## Opening Model

Support these entry points:

- `strata-next init`
  - Machine-level setup only.
  - Creates or updates `~/.strata`.
  - Detects hardware and recommends a profile.
  - Configures local AI only when explicitly requested.
  - Does not create a database unless the user invokes `new`.

- `strata-next new <path>`
  - Creates a durable-local database at `<path>`.
  - Applies profile and memory-budget settings from explicit flags or the machine default profile.
  - Does not download models or perform network work.

- `strata-next <path>`
  - Opens an existing durable-local database and starts the REPL.
  - If `<path>` does not exist, print the `new <path>` guidance instead of silently creating it.

- `strata-next`
  - If the current directory is a database root, open it.
  - If the current directory has the standard child database path from the product contract, open that.
  - Otherwise start in no-database help mode or print concise creation/opening guidance. Do not silently create a durable database.

- `strata-next --db <path> <command>`
  - Opens the durable-local database and executes one command.
  - Returns a nonzero exit code on command failure.

- `strata-next --cache <command>`
  - Opens an in-memory database and executes one command.
  - Rejects `--cache <path>` or any cache+path combination.
  - In REPL mode, cache state lives only for that process.

Read-only open can be added if `executor-next` exposes a durable read-only mode. Do not preserve old follower mode unless a new executor contract is created for it.

## Runtime Profile And Memory Budget

Wire CLI flags to the new runtime profile and memory-budget contracts once those are available:

- `--profile <small|balanced|large|custom>`
- `--memory-budget <bytes|human-size>`
- `--local-ai`
- `--no-local-ai`

The CLI should not independently enforce storage memory behavior. It should parse user intent, pass it into the executor/runtime opening layer, and render diagnostics returned by that layer.

## REPL Model

The REPL should stay small and predictable:

- Prompt includes the active database label, branch, and space.
- `help` prints command groups and examples.
- `use <branch>` changes the default branch after validation.
- `use <branch>/<space>` changes branch and space after validation.
- `clear` clears the terminal.
- `quit` and `exit` close the executor and exit.

Do not expose old public transaction commands in the REPL. Snapshot isolation and commit validation belong behind the executor and engine APIs, not in a user-facing CLI session state.

## Command Surface

Implement command parsing directly against `strata_executor_next::Command`. Each parsed CLI command should produce one executor command or a local CLI action.

### Branch

Required commands:

- `branch list`
- `branch get <name>`
- `branch create <name>`
- `branch fork <source> <target>`
- `branch fork-at-version <source> <target> <version>`
- `branch fork-at-timestamp <source> <target> <timestamp>`
- `branch delete <name>`

### KV

Required commands:

- `kv put <key> <value>`
- `kv get <key>`
- `kv delete <key>`
- `kv exists <key>`
- `kv list [prefix] [--cursor <cursor>] [--limit <n>]`
- `kv scan [prefix] [--cursor <cursor>] [--limit <n>]`
- `kv count [prefix]`
- `kv sample [--limit <n>]`
- `kv history <key>`
- `kv batch-put <json-or-file>`
- `kv batch-get <json-or-file>`
- `kv batch-delete <json-or-file>`
- `kv batch-exists <json-or-file>`

Values should support literal strings, `@file` input, and JSON string encoding without changing executor semantics.

### JSON

Required commands:

- `json set <collection> <key> <document-or-file>`
- `json get <collection> <key>`
- `json delete <collection> <key>`
- `json exists <collection> <key>`
- `json list <collection> [--prefix <prefix>] [--cursor <cursor>] [--limit <n>]`
- `json count <collection>`
- `json sample <collection> [--limit <n>]`
- `json history <collection> <key>`
- `json batch-set <collection> <json-or-file>`
- `json batch-get <collection> <json-or-file>`
- `json batch-delete <collection> <json-or-file>`
- `json index create ...`
- `json index drop ...`
- `json index list <collection>`

Do not add a native query DSL in this slice.

### Vector

Required commands:

- `vector create <collection> --dimensions <n> [--metric <metric>]`
- `vector drop <collection>`
- `vector list`
- `vector stats <collection>`
- `vector count <collection>`
- `vector upsert <collection> <key> <vector-or-file> [--metadata <json-or-file>]`
- `vector get <collection> <key>`
- `vector delete <collection> <key>`
- `vector query <collection> <vector-or-file> [--top-k <n>] [--filter <json-or-file>]`
- `vector list-keys <collection> [--prefix <prefix>] [--cursor <cursor>] [--limit <n>]`
- `vector update-metadata <collection> <key> <patch-or-file>`
- `vector delete-by-filter <collection> <filter-or-file>`
- `vector delete-all <collection>`
- `vector batch-upsert <collection> <json-or-file>`
- `vector batch-get <collection> <json-or-file>`
- `vector batch-delete <collection> <json-or-file>`
- `vector exists <collection> <key>`
- `vector history <collection> <key>`

### Event

Required commands:

- `event append <stream> <payload-or-file> [--type <type>]`
- `event batch-append <stream> <json-or-file>`
- `event get <stream> <id>`
- `event range <stream> [--from <id>] [--to <id>] [--limit <n>]`
- `event range-time <stream> [--from <timestamp>] [--to <timestamp>] [--limit <n>]`
- `event list [--prefix <prefix>] [--cursor <cursor>] [--limit <n>]`
- `event list-types <stream>`
- `event len <stream>`
- `event exists <stream> <id>`
- `event verify-chain <stream>`
- `event get-by-type <stream> <type> [--limit <n>]`

### Graph Core

Required commands:

- `graph create <name>`
- `graph delete <name>`
- `graph list`
- `graph info <name>`
- `graph add-node <graph> <id> [--labels <json-or-file>] [--props <json-or-file>]`
- `graph get-node <graph> <id>`
- `graph remove-node <graph> <id>`
- `graph list-nodes <graph> [--cursor <cursor>] [--limit <n>]`
- `graph add-edge <graph> <id> <from> <to> <type> [--props <json-or-file>]`
- `graph get-edge <graph> <id>`
- `graph remove-edge <graph> <id>`
- `graph neighbors <graph> <node> [--direction <in|out|both>] [--type <type>] [--limit <n>]`
- `graph bindings-for-entity <graph> <entity>`
- `graph batch-write <graph> <json-or-file>`

Do not expose old ontology, traversal DSL, analytics, or schema commands until separate V1 contracts exist.

### Arrow

Required commands:

- `arrow import <primitive> <target> <file> [options]`
- `arrow export <primitive> <target> <file> [options]`

Arrow is part of the product by default. The CLI should not hide it behind a user-facing optional command group.

### Inference

Required commands should mirror the executor inference contract:

- `models list`
- `models local`
- `models pull <model>`
- `inference generate ...`
- `inference embed ...`
- `inference embed-batch ...`
- `inference rank ...`
- `inference tokenize ...`
- `inference detokenize ...`
- `inference unload ...`
- `inference cache-status`
- `inference capability <model>`

Cloud providers are available by default when credentials are configured. Local inference is gated by explicit local setup because it depends on native model/runtime availability.

## Output Modes

Support:

- `--output human`
- `--output json`
- `--output raw`

JSON output must be stable and must serialize executor output without losing fields. Human output may be friendlier, but it should not invent semantics that are not present in the executor response.

## Explicit Exclusions

Do not port these old CLI surfaces in this slice:

- `flush`
- `compact`
- public `begin`, `commit`, `rollback`, or `txn`
- follower mode
- old IPC daemon `up` and `down`
- `uninstall`
- generic runtime config mutation commands
- old search and recipe commands
- old graph ontology, graph analytics, and graph traversal DSL commands
- old storage durability counters unless exposed through the new diagnostics contract

If a user tries an old command, return a clear unsupported-command message and point to the closest V1 command group when one exists.

## Implementation Order

1. Scaffold `crates/cli-next`, add it to the workspace, and create the `strata-next` binary.
2. Add the top-level app shell: global flags, output mode, local command dispatch, executor open/close, and error rendering.
3. Add durable/cache/path resolution with no silent database creation.
4. Add REPL and pipe mode using the new context model.
5. Implement branch and KV command parsing first because they prove the vertical spine.
6. Add JSON, event, vector, graph core, Arrow, and inference command groups in that order.
7. Add `init` and `new` after the core open/execute flow is stable.
8. Wire runtime profile and memory-budget flags to the new runtime opening contract.
9. Add source/dependency guards that prevent regressions into old executor, old engine, old storage, and old CLI APIs.
10. Prepare cutover notes for renaming the binary from `strata-next` to `strata` after the old CLI is retired.

## Acceptance Criteria

- `crates/cli-next` compiles independently and depends on `strata-executor-next`, not old executor or old engine crates.
- The `strata-next` binary can open cache and durable-local databases through normal executor APIs.
- The CLI covers the V1 command surface for branch, KV, JSON, vector, event, graph core, Arrow, and inference.
- REPL, pipe, and one-shot command paths all route through the same command parser and executor dispatch.
- No old public transaction, manual maintenance, follower, daemon, recipe/search, or ontology commands are exposed.
- `init` is machine setup, `new` is database creation, and neither performs hidden network/model work.
- JSON output is stable enough for SDK wrappers, MCP servers, and scripts to consume.
- The old CLI and new CLI can coexist until cutover.
