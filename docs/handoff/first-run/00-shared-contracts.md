# Shared contracts: what the Strata binary ships today

Read this before any repo-specific document. Everything below is implemented and
e2e-tested in `stratalab/strata-core` (branch `v1`, 2026-07-06) — these are the
contracts the website, shim, SDKs, and evals build against. Where a fact is a
target rather than shipped, it is marked **(pending)**.

## 1. Identity and versioning

- Binary: `strata`. GitHub org: **`stratalab`**; engine repo: `stratalab/strata-core`.
- Package names (fixed): PyPI `stratadb` · npm `@stratadb/core`, `@stratadb/mcp` ·
  crates.io `stratadb` (lib) / `strata-cli` (bin) at the M9B rename · Docker `stratadb/strata`.
- `strata --version` prints `strata <version>` to **stdout** and exits **0**
  (was stderr/exit-2; fixed 2026-07-06 — install verification depends on this).
- Version today: the binary reports `1.0.0` (cli crate version); the workspace is
  `0.6.1`; SDKs are 0.14.x/0.15.x. **(pending)** D7 unifies everything to one
  number per release tag, starting `1.0.0` at V1 promotion. Do not hardcode
  versions in artifacts — read them from the release manifest or the binary.

## 2. Database targeting (the D2 contract)

Resolution order for every one-shot or piped invocation:

1. Positional path or `--db <path>` — explicit, wins.
2. `STRATA_DB=<path>` environment variable — session-level fallback.
3. Otherwise **refuse** — exit 2, stderr:

```
error: [invalid_argument.cli.no_database]: no database specified
  hint: pass a path (strata ./mydb kv put …), set STRATA_DB, or use --cache for ephemeral
```

Strata never opens the current directory implicitly. `--cache` is the explicit
in-memory escape (nothing persisted, per process). A bare **interactive** `strata`
opens a cache-mode REPL with a banner stating nothing persists.

Any artifact that teaches a first command must teach one that passes this contract
(no bare `strata kv put …` examples — that was coherence finding F3).

## 3. Output contract

- `--json`: one compact envelope per command — `{"type": "<snake_case>", "data": …}`.
- `--raw`: script-friendly bare values. Default: human-readable.
- **Bytes are base64 on the JSON wire** (KV keys/values, continuation cursors).
  Human output decodes valid UTF-8; cursors stay base64 and are passed back
  verbatim (`--cursor`).
- Raw serialized commands (the programmatic path all channels share):
  `strata <db> command run --command-json '{"type":"kv_get","key":"a2V5"}'`.
  The command deserializer is strict (`deny_unknown_fields`) and its errors name
  the offending field and the valid set.

## 4. Error contract (the D4 contract)

Every failure carries a stable code `<class>.<area>.<detail>`. `--json` failures
emit this envelope on **stderr** (a structured-log line may precede it; the
envelope is the last line):

```json
{"error":{
  "class":"not_found",
  "code":"not_found.engine.branch",
  "retry_policy":"never",
  "retryable":false,
  "commit_outcome":"not_applicable",
  "message":"branch `ghost` does not exist",
  "suggested_fix":"Check that the requested branch, space, collection, graph, document, key, or model exists.",
  "docs_url":"https://stratadb.org/e/not_found.engine.branch",
  "reference_id":"err_local_38dd3c5a_000001"
}}
```

(`trace_id`, `details[]`, `hints[]` appear when present.) Human output renders
`code: message (reference)` plus `hint:` and `ref:` lines.

**`docs_url` is always `https://stratadb.org/e/<code>`** — the code is the final
path segment. There are 176 registered codes today; the machine-readable registry
is `strata agents errors --json`. Consumers match on `code` and `class`, never on
message text.

## 5. The agents surface (the D3 contract)

All run without a database:

| Command | Returns |
|---|---|
| `strata agents guide` | Complete offline markdown usage guide, generated from the binary's own metadata (version-locked; equivalent of `llms-full.txt`) |
| `strata agents commands --json` | `{"type":"agents_commands","data":{…}}` — the generated command catalog (32 commands carry full metadata today; grows with the IDL workstream) |
| `strata agents errors --json` | `{"type":"agents_errors","data":{"count":176,"errors":[{code,class,retry_policy,commit_outcome,message,hint,ref}]}}` |
| `strata agents init [--apply]` | Writes `.strata/AGENTS.md` into the repo; `--apply` idempotently appends a ~10-line pointer block to `AGENTS.md`/`CLAUDE.md` |

**"After install, run `strata agents guide`" is the canonical pointer that ends
web search** — every epilogue, README, and llms.txt should carry it.

## 6. Setup and diagnostics (the D5 contract)

- `strata init --json` → `{"type":"init","data":{"home":…,"created":bool,"next_steps":[…]}}`.
  `next_steps` is the canonical machine-readable list of taught commands
  (currently: `strata ./my-db kv put greeting hello`, `strata ./my-db kv get greeting`,
  `strata --cache`, `strata agents guide`). Mirror it; don't invent variants.
- `strata doctor [--json] [path]` → `{"type":"doctor","data":{"binary":…,
  "platform":"linux-x86_64","home":…,"path_ok":bool,"database":null|{…},
  "issues":[{"code":…,"hint":…}]}}`. Exits **non-zero when issues exist**; never
  creates databases. This is the install-verification step: end install scripts
  with `strata doctor` rather than only `--version`.

## 7. MCP (the D8 binary contract)

`strata <db> mcp serve` speaks MCP over **stdio**: newline-delimited JSON-RPC 2.0
on stdin/stdout, logs on stderr. Facts a client/shim/registry listing can rely on:

- `initialize` echoes the client's `protocolVersion`; `serverInfo.name` is
  `"strata"`; `instructions` teach models to call `strata_guide` first.
- Methods: `initialize`, `ping`, `tools/list`, `tools/call`; notifications are
  silently accepted. Unknown methods → `-32601`; unknown tools → `-32602`.
- **20 curated tools**: `strata_guide`, `strata_command` (escape hatch — any
  cataloged command as raw wire JSON), and core verbs for kv/json/vector/event/
  graph/branch. Tool results carry the same `{type,data}` envelopes; failures are
  `isError` results carrying the §4 error envelope.
- Targeting follows §2 (explicit path / `STRATA_DB` / `--cache`; refusal otherwise).
- Reference client config: `{"command":"strata","args":["<db-path>","mcp","serve"]}`.

## 8. Verification philosophy (P2/P7)

Every taught command must be transcript-tested against the shipped binary.
strata-core's e2e harness (`scripts/cli-tests/` — 17 suites, ~450 assertions,
including a full scripted MCP client session in `17_mcp.sh`) is the reference
pattern: bash + python3, per-suite sandbox (`STRATA_HOME` isolated, `STRATA_DB`
cleared), assertions on codes and classes rather than prose. Mirror the pattern
for each repo's golden path.

## 9. Open decisions (do not preempt)

- **Telemetry**: recommendation is none, ever, in any artifact — treat as the
  default until the owner decides otherwise. install.sh must not phone home.
- **Windows native binary**: pending; wheels/npm prebuilds cover Windows meanwhile.
- **`strata ai`**: does not exist in the V1 binary — remove it from any epilogue
  or doc that advertises it (coherence finding F5).
