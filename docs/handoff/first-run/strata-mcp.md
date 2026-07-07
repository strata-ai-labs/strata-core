# strata-mcp — work order (D8 distribution half, D1)

Prereq reading: `00-shared-contracts.md` (§7 especially).

## Context: the repo's role changed

The MCP **server now lives in the main binary**: `strata <db> mcp serve`
(decided 2026-07-06; shipped in strata-core). It speaks stdio JSON-RPC with 20
curated tools plus `strata_guide`/`strata_command` meta-tools, version-locked
with the engine, same envelopes and error codes as the CLI.

This repo's Rust server (61 tools, `cargo install` distribution) is therefore
**superseded**. Its new job is packaging and registry presence: make Strata one
config line in every MCP client. Archive or clearly deprecate the old server
code — do not leave two servers alive without a cutover note (one canonical
path).

## 1. `@stratadb/mcp` — the npx shim

A thin npm package whose bin:

1. **Resolves the platform binary**: use an already-installed `strata` on PATH if
   its version matches the shim's pinned version; otherwise download the GitHub
   release asset for `{os}-{arch}` from `stratalab/strata-core` releases (the
   same assets install.sh uses), **sha256-verified** against `SHA256SUMS`, cached
   under `~/.strata/bin`. Respect `STRATA_HOME`.
2. **Execs the server**: `strata <db-args> mcp serve`, passing through the shim's
   own CLI args. Argument contract:
   - `--db <path>` or a positional path → forwarded as the database target
   - `--cache` → forwarded (ephemeral memory)
   - `--branch <b>` / `--space <s>` → forwarded (session scope)
   - no target → let the binary refuse; its stderr teaching error
     (`invalid_argument.cli.no_database`) is the correct UX
3. **stdio discipline**: the child's stdin/stdout are the protocol channel — the
   shim must not write anything to stdout itself (download progress goes to
   stderr, quiet by default).
4. **Version**: the shim's npm version pins the binary version it downloads
   (republished by the release train, D7). No `latest` resolution at runtime.

Target UX (must work verbatim once published):

```
claude mcp add strata -- npx -y @stratadb/mcp --db ~/strata/agent-memory
```

```json
{ "mcpServers": { "strata": { "command": "npx",
  "args": ["-y", "@stratadb/mcp", "--db", "~/strata/agent-memory"] } } }
```

Note `~` is not expanded by config files — the shim should expand a leading `~`
in the db path before forwarding.

**Follow-on (cheap, after npx ships)**: `uvx stratadb-mcp` — the same shim
pattern published to PyPI for the Python-native crowd.

## 2. Conformance test

Script a full client session against the shim (the strata-core reference is
`scripts/cli-tests/17_mcp.sh`): pipe newline-delimited JSON-RPC —
`initialize` → `notifications/initialized` → `tools/list` → `tools/call`
(`strata_kv_put`, `strata_kv_get`, `strata_guide`, `strata_command`) → an
unknown tool — and assert:

- `initialize` result: `serverInfo.name == "strata"`, instructions mention
  `strata_guide`;
- `tools/list`: 20 tools, every one with an object `inputSchema` and a
  description longer than a label;
- tool results carry `{type,data}` envelopes; failures are `isError` results
  whose text parses as the shared error envelope (match on `error.code`);
- unknown tool → JSON-RPC `-32602`; stdout contains nothing but JSON-RPC lines;
- writes persist: after the session, read the key back through the shim again.

Run it in CI on the platform matrix, and post-publish against the real registry
(`npx -y @stratadb/mcp@<version>`), not just pre-publish.

## 3. Registry listings

Once the npx path works, list the server everywhere agent runtimes look —
this is the agent-world equivalent of SEO:

- the official MCP registry (modelcontextprotocol servers listing)
- Smithery, mcp.so, and equivalent directories
- a Claude Desktop extension bundle if the format is available

Listing copy: Strata is an **embedded** multi-model database (KV, JSON, vectors,
events, graphs) with branches and time travel — persistent agent memory in one
config line, no server to run. Include the config snippet and the
`strata_guide`-first orientation note. Never "server-side" framing.

## 4. D1 sweep

The README references `strata-systems/strata-*` — every GitHub URL becomes
`stratalab/…` (engine: `stratalab/strata-core`).

## Acceptance

- The `claude mcp add` one-liner works on a clean machine (macOS + Linux) with
  nothing preinstalled but Node.
- Conformance session passes post-publish; sha mismatch on download is a hard
  refusal.
- Old server code is archived/deprecated with a pointer to `strata mcp serve`.
- Zero non-`stratalab` GitHub references.
