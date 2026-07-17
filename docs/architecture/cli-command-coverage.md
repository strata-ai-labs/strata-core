# CLI-Next Command Coverage

Status: **SUPERSEDED (2026-07-17, TCP3.0)** — this hand-maintained
inventory has drifted (it predates the shipped `clone`/`remote`/`config
set|unset|path|show`/`agents`/`mcp` verbs and still lists inference as
raw-only). Per the Phase 3 plan (`v1-test-coverage-phase3-plan.md` §1),
point-in-time inventories are not trackers: CLI verb coverage becomes a
mechanical clap-tree enumeration guard in TCP3.10, and this file is
deleted in that slice. Do not update it.

Source documents:

- [old-executor-to-v1-gap-analysis.md](old-executor-to-v1-gap-analysis.md)
- [cli-next-user-experience-parity-implementation-plan.md](implementation-plans/cli-next-user-experience-parity-implementation-plan.md)

## Disposition

| Status | Meaning |
| --- | --- |
| Supported | First-class `strata` command exists and executes through `executor-next`. |
| Renamed | Capability exists with a V1 command name or grouped shape. |
| Deferred | The old UX is recognized as useful, but the V1 engine/executor surface is not ready. |
| Removed | Intentionally not exposed in V1. |
| Raw Only | Available through `strata command run` but not ergonomic yet. |

## Core Commands

| Old CLI area | V1 CLI area | Executor surface | Status | Notes |
| --- | --- | --- | --- | --- |
| `ping` | `ping` | `Command::Ping` | Supported | One-shot, REPL, and pipe. |
| `info` | `info` | `Command::Info` | Supported | Branch-aware. |
| `health` | `health` | `Command::Health` | Supported | Branch-aware. |
| `metrics` | `metrics` | `Command::Metrics` | Supported | Branch-aware. |
| `describe` | `describe` | `Command::Describe` | Supported | Branch-aware. |
| `config get` | `config get` | `Command::ConfigGet` | Supported | Read-only. |
| `config get-key` | `config get-key` | `Command::ConfigureGetKey` | Supported | Read-only. |
| `flush` | hidden deferred error | none in V1 executor | Removed | Public storage maintenance is intentionally not part of V1. |
| `compact` | hidden deferred error | none in V1 executor | Removed | Public storage maintenance is intentionally not part of V1. |
| `durability-counters` | none | no V1 executor command | Removed | Use `health`, `metrics`, and `describe`. |

## Open And Shell UX

| Old CLI area | V1 CLI area | Status | Notes |
| --- | --- | --- | --- |
| one-shot commands | `strata --db <path> <command>` / `strata <path> <command>` | Supported | Uses the same executor parser as REPL. |
| cache mode | `strata --cache` | Supported | Cache state lives for the process. |
| REPL | `strata` / `strata <path>` | Supported | Prompt carries branch and space. |
| pipe mode | `cat commands.strata \| strata <path>` | Supported | Blank and comment lines are skipped. |
| `help` meta command | `help` | Supported | Prints top-level CLI help. |
| `use <branch> [space]` | `use <branch> [space]` | Supported | Validates branch and optional space. |
| `quit` / `exit` | `quit` / `exit` | Supported | Local REPL command. |
| `clear` | `clear` | Supported | Local REPL command. |

## Branch And Space

| Old CLI area | V1 CLI area | Executor surface | Status | Notes |
| --- | --- | --- | --- | --- |
| branch list/get/create/fork/delete | `branch ...` | branch command family | Supported | `branch del` aliases `branch delete`. |
| branch fork at version/timestamp | `branch fork --version/--timestamp` | `BranchForkAtVersion`, `BranchForkAtTimestamp` | Supported | V1 grouped syntax. |
| branch diff/merge/tag/note | hidden deferred errors | none in V1 executor | Deferred | Needs git-style branch semantics pass. |
| space list/create/exists/delete | `space ...` | space command family | Supported | `space del` aliases `space delete`. |

## Primitive Commands

| Old CLI area | V1 CLI area | Executor surface | Status | Notes |
| --- | --- | --- | --- | --- |
| KV point operations | `kv put/get/delete/exists/history` | KV command family | Supported | `kv del` aliases `kv delete`; values support literal, `@file`, and `--file`. |
| KV list/scan/count/sample | `kv list/scan/count/sample` | KV command family | Supported | Pagination exposed where executor supports it. |
| KV batch operations | raw command path | KV batch command family | Raw Only | Ergonomic batch syntax remains a follow-up. |
| JSON point operations | `json set/get/delete/exists/history` | JSON command family | Supported | `json del` aliases `json delete`; JSON values support literal, `@file`, and `--file`. |
| JSON index operations | `json index ...` | JSON index command family | Supported | Create/drop/list. |
| JSON batch operations | raw command path | JSON batch command family | Raw Only | Ergonomic batch syntax remains a follow-up. |
| Vector collection operations | `vector collection ...` | vector collection command family | Supported | `vector collection del` aliases delete. |
| Vector point/query operations | `vector upsert/get/delete/query/...` | vector command family | Supported | Vectors and metadata support file-backed inputs. |
| Vector index search | `vector query --diagnostics` | `VectorIndexQuery` | Supported | Exposes index diagnostics through executor output. |
| Vector batch operations | raw command path | vector batch command family | Raw Only | Ergonomic batch syntax remains a follow-up. |
| Event append/read/list/range | `event ...` | event command family | Supported | Payloads support literal, `@file`, and `--file`. |
| Event batch append | raw command path | `EventBatchAppend` | Raw Only | Ergonomic batch syntax remains a follow-up. |
| Graph core operations | `graph ...` | graph core command family | Supported | Node/edge properties support file-backed inputs. |
| Graph ontology | hidden deferred error | none in V1 executor | Deferred | Needs ontology pass. |
| Graph analytics/traversal | hidden deferred error | none in V1 executor | Deferred | Needs graph analytics pass. |

## Secondary Surfaces

| Old CLI area | V1 CLI area | Executor surface | Status | Notes |
| --- | --- | --- | --- | --- |
| Arrow import/export | `arrow import/export` | Arrow command family | Supported | Uses current executor Arrow support. |
| Inference/model commands | raw command path for now | inference command family | Raw Only | First-class CLI syntax should be added once UX is settled. |
| Search | hidden deferred error | none in V1 executor | Deferred | Search/query layer is a separate milestone. |
| Recipes | hidden deferred error | none in V1 executor | Deferred | Recipe layer is a separate milestone. |
| Transactions | hidden deferred error | none in V1 executor | Deferred | V1 engine handles commits internally; public txn UX is not restored. |
| Daemon lifecycle `up/down/uninstall` | hidden deferred error | none in V1 executor | Deferred | Needs a V1 daemon/server contract before exposing. |

## Raw Command Escape Hatch

`strata command run --command-json <command-json>` and
`strata command run --file <path>` remain available for executor commands that
do not have ergonomic CLI syntax yet. This is an escape hatch, not the target
UX for stable public commands.

## CLI Scenario Corpus

The command matrix lives at [scripts/cli_next_command_matrix.sh](../../scripts/cli_next_command_matrix.sh).
It is a broad smoke test that verifies command families, aliases, output modes,
file-backed inputs, raw command execution, and deferred old commands.

The workflow corpus lives at [scripts/cli_next_corpus.sh](../../scripts/cli_next_corpus.sh)
with scenario files under [scripts/cli-corpus/](../../scripts/cli-corpus/).
Each scenario owns a fresh durable database and verifies multi-command behavior
across process boundaries. The current corpus covers:

1. branch, space, time-travel, and durable branch-delete recovery;
2. KV and JSON pagination, optional reads, indexes, history, and raw batches;
3. vector filters, diagnostics, metadata patches, delete-by-filter, branches,
   and spaces;
4. event sequence/time reads plus graph pagination, neighbor filters, and branch
   divergence;
5. raw command execution, Arrow import/export, pipe mode, and structured error
   rendering;
6. init, pipe `use` scope, cache-mode process locality, and raw/human rendering.

Run the full corpus with:

```bash
scripts/cli_next_corpus.sh
```

Run a focused scenario by name:

```bash
scripts/cli_next_corpus.sh 03_vector_index_filters_branches
```
