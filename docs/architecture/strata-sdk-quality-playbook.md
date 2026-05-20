# Strata SDK Quality Playbook

Status: V1 reference; synthesized from Stainless documentation and the Stripe SDK lineage

## Purpose

This document captures everything Strata's SDK work should be measured against,
extracted from the Stainless platform documentation (the same lineage as
Stripe's SDK team) and from the broader Stripe SDK design philosophy. Stainless
has wound down its hosted product post-Anthropic acquisition, but the docs
remain a useful blueprint and the principles are durable regardless of who
generates the code.

The audience for this doc is the engineer or AI agent generating Strata's
SDKs. The standard is: **the SDK should feel hand-crafted, not auto-generated,
in every language Strata supports.**

## Top-Level Philosophy

Three principles drive everything below:

1. **Idiomatic per language, not auto-generated-looking.** Python should feel
   Pythonic. Rust should feel Rust-y. JavaScript should feel JS-native. Each
   SDK is designed for its language's conventions — naming, async patterns,
   typing, error handling. An auto-generated SDK that reads identically across
   languages has already failed.

2. **Consistent semantics across languages.** Despite per-language
   idiomaticity, the conceptual model is identical everywhere. `Branch.create()`
   does the same thing in every SDK. A developer who knows the Python SDK can
   pick up Rust in 20 minutes.

3. **The AI agent is the primary user.** Vibecoders' AI agents (Claude Code,
   Cursor, Codex) will write code against these SDKs. The SDK must be
   predictable enough that the AI gets it right on the first try without
   reading source code or running into surprises.

## Languages to Support (Prioritized)

### V1 must-have
- **Python** — vibecoder default for AI work; richest ecosystem for ML/LLM
- **Rust** — native to the codebase; high-perf use cases; Tauri/embedded
- **TypeScript / JavaScript** — Node + browser; cache backend lives here
- **Go** — backend services; small footprint; AI agent products

### V1.x stretch
- **Swift** — macOS / iOS app integrations; Strata Foundry path
- **Java / Kotlin** — Android; enterprise inroads later

### V1.5+
- **C#** — .NET ecosystem
- **Ruby** — Rails / scripting
- **PHP** — broader long tail
- **Terraform provider** — when StrataHub lands
- **CLI** — also language-shaped; treat as first-class

Each language gets its own SDK, not a thin wrapper around one canonical
implementation. Use a Stainless-style spec-driven approach: maintain a single
spec, generate per-language idiomatically. Stainless went out of business as
a hosted product but the spec-driven approach is the right pattern regardless
of who implements the generator.

## Strata IDL: The Source of Truth

OpenAPI is the wrong shape for Strata because Strata is primarily an
embedded library, secondarily an MCP server, and only eventually a cloud
HTTP API (when Hub lands). OpenAPI assumes HTTP endpoints; forcing Strata's
operations into REST shape distorts the design. The right answer is a
**Strata-native IDL** that drives generation of MCP servers, SDKs, OpenAPI
(as a downstream output for tools that consume it), documentation, CLI
commands, and test fixtures from one source.

The Stainless principles — resource organization, method/model patterns,
client settings, README examples, language-specific overrides — all apply,
but the source artifact is a Strata IDL rather than an OpenAPI spec.

### IDL structure

```yaml
edition: "2026.01"

organization:
  name: Strata
  license: Apache-2.0
  docs: https://strata.dev/docs

types:
  Branch:
    fields:
      id: BranchId
      name: string
      parent: optional<BranchId>
      fork_version: optional<CommitVersion>
      created_at: Timestamp

  Commit:
    fields:
      version: CommitVersion
      timestamp: Timestamp
      branch: BranchId
      rows_written: u64
      rows_deleted: u64

  CommitRow:
    fields:
      key: bytes
      value: optional<bytes>
      tombstone: bool
      expiry: optional<Timestamp>

resources:
  database:
    description: Top-level Strata database handle
    methods:
      open:
        inputs: { path: string }
        outputs: Database

  branches:
    description: Branch management operations
    parent_resource: database
    methods:
      create:
        inputs: { name: string, from: optional<BranchId> }
        outputs: Branch
        errors: [branch.already_exists, branch.invalid_name]
        description_for_llm: |
          Creates a new branch. If `from` is provided, the new branch
          starts as a copy-on-write fork of that branch...
      fork:
        inputs:
          from: BranchId
          at_version: optional<CommitVersion>
          name: string
        outputs: Branch
        errors: [branch.fork.version_unavailable, branch.fork.source_unavailable]
      list:
        outputs: stream<Branch>
        pagination: cursor
      delete:
        inputs: { id: BranchId }
        errors: [branch.not_found, branch.delete.has_dependents]

  tables: { ... }
  ai: { ... }

errors:
  branch.fork.version_unavailable:
    code: "branch.fork.version_unavailable"
    message_template: "Version {version} is no longer retained on branch {branch}"
    suggested_fix: |
      Fork from an available version. Use
      `client.branches.list_available_versions(branch_id)` to see options.
    docs_url: "/docs/branching/fork-at-history#retention"

streams:
  ai.generate:
    input: GenerationRequest
    output_stream: GenerationToken
    description: ...

# MCP resources — URI-addressable read-only context the agent can fetch
mcp_resources:
  database_branches:
    uri_template: "strata://database/{db_id}/branches"
    description_for_llm: |
      JSON list of current branches with id, name, parent, fork_version,
      created_at. Use to discover branch topology before forking.
    output_type: List<Branch>
  architecture_overview:
    uri_template: "strata://docs/architecture/overview"
    description_for_llm: "Strata L1-L9 architecture overview"
    output_type: markdown
  error_registry:
    uri_template: "strata://docs/error-registry"
    description_for_llm: "Every error code with cause + fix + docs link"
    output_type: markdown

# MCP prompts — canonical patterns the agent reads and adapts
mcp_prompts:
  setup_rag:
    description_for_llm: "Canonical RAG setup pattern with Strata"
    parameters:
      docs_path: { type: string }
      embedding_model: { type: string, optional: true }
    template: |
      To set up RAG with Strata on {{docs_path}}:
      1. ...
  safe_schema_migration:
    description_for_llm: "Branch-test-merge pattern for safe schema changes"
    parameters:
      change_description: { type: string }
    template: |
      To safely migrate the schema:
      1. Create an experimental branch...

# MCP sampling — server-initiated calls back to the client's LLM
mcp_sampling:
  schema_inference:
    trigger: "knowledge graph creation requires schema decision"
    prompt_template: |
      The user is building a knowledge graph from {{document_summary}}.
      Propose a schema as JSON. Return: { entities: [...], edges: [...] }.
  diagnostic_reasoning:
    trigger: "self-DBA detects anomaly requiring narrative explanation"
    prompt_template: |
      Operational state: {{state}}
      Recent changes: {{changes}}
      Hypothesize 3 causes ranked by likelihood, each with a test command.

client_settings:
  retries:
    max: 2
    initial_delay_seconds: 0.5
    max_delay_seconds: 8
    jitter_pct: 25
  timeouts:
    total_seconds: 60
    read_seconds: 30
  idempotency:
    header: "Strata-Idempotency-Key"
    format: "strata-retry-{uuid}"

pagination:
  cursor:
    request_field: cursor
    response_field: next_cursor

readme:
  headline:
    operation: ai.ask
    args: { prompt: "Build me a knowledge graph from these PDFs" }
  default:
    operation: tables.list
  pagination:
    operation: branches.list

targets:
  python:
    package_name: strata
    publish: { pypi: strata }
  rust:
    package_name: strata-sdk
    publish: { crates_io: strata-sdk }
  typescript:
    package_name: "@strata/sdk"
    publish: { npm: "@strata/sdk" }
  go:
    module: github.com/strata/strata-go
  swift:
    package: StrataSDK
  ...
```

### Why this IDL shape rather than OpenAPI

1. **First-class types and resources, not endpoints.** Embedded library
   APIs don't have endpoints. Resources + methods + types is the right
   primary abstraction.
2. **First-class errors with code + message + fix + docs link.** OpenAPI's
   error story is weak; the Strata IDL makes errors structured artifacts
   with everything needed for Stripe-quality error UX.
3. **First-class streams.** Generation tokens, materialization progress,
   event subscriptions all involve streams. OpenAPI handles this awkwardly.
4. **All four MCP primitives are first-class, not just tools.** MCP is
   broader than HTTP API wrapping — it has tools, resources, prompts, and
   sampling as separate primitive types. The IDL models all four because
   Strata's MCP server uses all four (see the "MCP Server: All Four
   Primitives" section). OpenAPI has no concept of resources-as-context,
   prompts-as-templates, or sampling — it only models tools, awkwardly.
5. **MCP-specific concerns explicit.** `description_for_llm`, context-window
   considerations, client capability adaptation all live in the IDL.
6. **Single source for multiple outputs.** MCP server + SDKs + OpenAPI +
   docs + CLI all generate from one file. No drift between MCP and SDK
   surfaces.

### Benefits

1. **One source of truth.** Changes to the API surface propagate to every
   target automatically.
2. **Per-language overrides.** `skip` / `only` properties exclude operations
   from specific languages when they don't translate cleanly.
3. **AI-agent-updatable.** Continuous AI agents can propose IDL changes and
   regenerate everything downstream to test new patterns.
4. **Honest about the architecture.** Strata isn't an HTTP API; the IDL
   doesn't pretend it is.

## The IDL → Assets Pipeline

The IDL is only as valuable as the pipeline that keeps everything in sync
with it. Without automated regeneration, the IDL becomes another doc that
drifts; with it, the IDL is the actual driving force behind every
developer-facing artifact.

### Pipeline architecture

```
Strata IDL (canonical spec, in repo)
    │
    ▼  (on push to IDL files in a PR)
GitHub Actions: validate IDL
    │  - Schema validation
    │  - Backward-compatibility check (no breaking changes without major bump)
    │  - Source-vocabulary guard (no ad-hoc magic strings outside IDL)
    │
    ▼
Run generators in parallel (one job per target):
    ├─ mcp-server-gen      → crates/mcp-server/
    ├─ sdk-gen-python      → sdks/python/
    ├─ sdk-gen-rust        → sdks/rust/
    ├─ sdk-gen-typescript  → sdks/typescript/
    ├─ sdk-gen-go          → sdks/go/
    ├─ sdk-gen-swift       → sdks/swift/
    ├─ openapi-gen         → docs/api/openapi.yaml (for downstream tools)
    ├─ docs-gen            → docs/sdk-reference/
    ├─ cli-gen             → crates/strata-cli/
    └─ test-fixtures-gen   → tests/golden/
    │
    ▼
Output validators (per target):
    │  - Compile checks (does the generated code build?)
    │  - Lint checks (clippy, ruff, eslint, gofmt, swiftlint)
    │  - Golden vector comparison (do byte-stable outputs match?)
    │  - Documentation drift check
    │
    ▼
Continuous AI agents review (parallel):
    │  - vibecoder-simulator: builds 5-10 small apps against the new SDK,
    │    flags any failures or rough edges
    │  - documentation drift detector: verifies docs match generated code
    │  - error message reviewer: every new error must have code + fix + docs
    │  - type consolidation auditor: any new redundant types?
    │  - API stability sentinel: breaking changes flagged distinctly
    │
    ▼
Aggregated PR comment:
    │  - "Generators produced X file changes"
    │  - "vibecoder-simulator: 8/10 apps built successfully; 2 new issues filed"
    │  - "1 potential breaking change detected — bump major version?"
    │  - "Type consolidation auditor: 0 new redundant types"
    │  - "Documentation drift: clean"
    │
    ▼
Human review + merge
    │
    ▼
On merge to main:
    ├─ Auto-publish per-language: pip / cargo / npm / go modules / Swift PM
    ├─ Update docs site
    ├─ Tag release with IDL version + per-SDK versions
    └─ Trigger full vibecoder-simulator sweep against new release
```

### Repository layout

The Strata pattern: **mono-repo for development, automated push to
language-specific repos for distribution.**

```
strata-core/                          # primary development repo
├── idl/strata.yml                    # the canonical IDL
├── crates/
│   ├── storage-next/                 # core
│   ├── engine-next/
│   ├── intelligence-next/
│   ├── inference-next/
│   ├── agent-next/
│   ├── mcp-server/                   # generated from IDL
│   ├── strata-cli/                   # generated from IDL
│   └── idl-generators/               # the generator binaries
│       ├── sdk-gen-python/
│       ├── sdk-gen-rust/
│       ├── sdk-gen-typescript/
│       ├── sdk-gen-go/
│       ├── sdk-gen-swift/
│       ├── openapi-gen/
│       └── docs-gen/
├── sdks/                             # generated SDK code (mirrored to per-language repos)
│   ├── python/                       → github.com/strata/strata-python
│   ├── rust/                         → github.com/strata/strata-rust-sdk
│   ├── typescript/                   → github.com/strata/strata-typescript
│   ├── go/                           → github.com/strata/strata-go
│   └── swift/                        → github.com/strata/strata-swift
└── docs/
    ├── architecture/
    ├── sdk-reference/                # generated from IDL
    └── api/openapi.yaml              # generated from IDL
```

Language ecosystems expect language-specific repos (PyPI, crates.io, npm
all look for repos to point at), so subtree-push automation mirrors
`sdks/<lang>/` to the appropriate per-language repo on every release.

### Generator binaries

Each generator is its own Rust binary that consumes the IDL and emits
target-specific files:

```rust
// crates/idl-generators/sdk-gen-python/src/main.rs
fn main() -> Result<()> {
    let idl = read_idl("idl/strata.yml")?;
    let python_sdk = generate_python_sdk(&idl)?;
    write_output(&python_sdk, "sdks/python/")?;
    Ok(())
}
```

Writing generators in Rust keeps the toolchain consistent and lets
generators share IDL parsing utilities. Continuous AI agents can propose
generator improvements as PRs to the generator binaries themselves.

### Version coordination

When the IDL bumps, all SDKs bump in lockstep. Use semantic versioning
tied to the IDL: IDL `edition: "2026.01.15"` produces:

- `strata` Python 1.15.0 (PyPI)
- `strata-sdk` Rust 1.15.0 (crates.io)
- `@strata/sdk` TypeScript 1.15.0 (npm)
- `github.com/strata/strata-go` Go v1.15.0
- `StrataSDK` Swift 1.15.0 (Swift PM)

The IDL date is the source of truth; per-SDK patch versions handle
SDK-internal fixes (idiomaticity tweaks that don't touch the spec).

### Reversibility built in

If a generated change is wrong, rolling back the IDL PR rolls back
everything downstream. The pipeline regenerates from the prior IDL state
automatically. There's no "now I have to manually fix all 5 SDKs to match"
— the pipeline handles it.

### Multi-client installer: `npx add-strata`

A single-command installer that configures the Strata MCP server across
every supported AI coding client is itself a generated artifact, not a
hand-maintained script. The pattern is direct: one CLI command, executed
in any project directory, drops the appropriate config file (or modifies
the existing one) for every detected agent client.

Reference implementation: Neon's `npx add-mcp <url>` ships across Claude
Code, Claude Desktop, Codex, Cursor, Gemini CLI, Goose, OpenCode, VS
Code, and Zed in one invocation. Each client has its own config-file
location and format:

| Client | Config location | Format |
|---|---|---|
| Claude Code | `.claude/mcp.json` | JSON |
| Claude Desktop | `~/Library/Application Support/Claude/claude_desktop_config.json` | JSON |
| Codex (OpenAI) | project-level `AGENTS.md` + CLI config | JSON + Markdown |
| Cursor | `.cursor/mcp.json` or global | JSON |
| Windsurf | `~/.codeium/windsurf/mcp_config.json` | JSON |
| Cline | `.vscode/cline_mcp_settings.json` | JSON |
| Continue | `~/.continue/config.json` | JSON |
| Gemini CLI | `~/.gemini/mcp.json` | JSON |
| VS Code (MCP) | `.vscode/mcp.json` | JSON |
| Zed | `.zed/settings.json` | JSON |
| OpenCode | `.opencode.json` | JSON |
| Goose | `~/.config/goose/config.yaml` | YAML |
| Kiro | `.kiro/config.json` | JSON |
| Replit Agent | platform-managed | platform-specific |
| ChatGPT (when MCP supported) | platform-managed | platform-specific |
| Raycast | extension-managed | extension-specific |

The installer detects which client configs already exist in the project
(or system), adds the Strata MCP server entry to each, and refuses to
overwrite existing entries without explicit confirmation. The IDL drives
this — each client's required config shape lives in the IDL's `targets`
section, and the installer is generated from that just like the SDKs are.

Target coverage at V1: at minimum the first 8 clients in the table above.
The Context7 reference establishes that supporting ~33 named clients is
achievable and worth the engineering investment over time.

Installer design constraints:
- **One command.** `npx add-strata` (or `pip install strata && strata install-mcp`, `cargo install strata-sdk && strata install-mcp`, etc., depending on the user's primary language) configures every detected client.
- **No prompts in the default path.** Detect available clients, install, report what changed. Confirmation only when overwriting.
- **Reversible.** `strata uninstall-mcp` removes every entry the installer added; no leftover state.
- **Idempotent.** Running the installer twice has no additional effect; running after a Strata version bump updates the server reference.
- **Generated from the IDL.** New target client = update the IDL `targets` section + generator runs in CI + installer picks up the new config-format support automatically.

## Continuous AI in the Pipeline

The pipeline becomes genuinely AI-augmented when Continuous AI agents
participate as CI reviewers. Each agent from the harness gets a CI hook:

| Agent | When it runs | What it does |
|---|---|---|
| **vibecoder-simulator** | Every IDL-change PR | Builds 5-10 small apps with the new SDK; flags failures or rough edges; on merge, triggers full sweep across 50+ apps × deployment targets |
| **documentation drift detector** | Every doc-generation output | Verifies generated docs match the IDL; flags any drift |
| **type consolidation auditor** | Every SDK generation output | Flags new redundant types or sibling structs that should be unified |
| **error message reviewer** | Every new error in the IDL | Verifies code + message_template + suggested_fix + docs_url are all present and useful |
| **API stability sentinel** | Every PR | Detects breaking changes; flags major-version bump requirements |
| **MCP schema validator** | Every MCP server generation | Verifies tool schemas are self-contained (no $refs), context-window appropriate, client-capability adapted |

Each agent posts findings to the PR. Humans see a synthesized comment
rather than chasing down each agent's output:

```
Pipeline result: ✅ generators ran, ✅ compilers passed
Continuous AI review:
  ✅ vibecoder-simulator: 8/8 apps built successfully on sample run
  ⚠️ API stability sentinel: 1 breaking change detected — bump major?
       - branches.list() return type changed from List<Branch> to PaginatedList<Branch>
  ✅ documentation drift detector: clean
  ✅ type consolidation auditor: no new redundant types
  ✅ error message reviewer: 2 new errors have complete metadata
  ✅ MCP schema validator: all 47 tools self-contained
```

This is the practical execution of the Continuous AI thesis applied to SDK
quality maintenance. The pipeline + agents + IDL together create a
self-maintaining SDK surface that gets better with every IDL improvement.

## Client-Level Features (Mandatory in V1)

### Authentication

Strata is embedded; auth is local. But the SDK still needs an authentication
abstraction for:

- **Database opening** — path + permissions + writer-lock contention handling
- **Branch operations** — when StrataHub lands, branch-level auth becomes real
- **Inference** — for cases where Strata AI calls remote models (post-V1)

Pattern: clean separation between embedded mode (no auth) and future
authenticated mode (Hub-coordinated). Match the Stainless multi-scheme pattern:

- Local mode (default, no auth)
- API key mode (when Hub lands)
- OAuth 2.0 client credentials (when Hub lands)
- Custom auth prefix (for enterprise extensions)

### Retries with Exponential Backoff

Default behavior matching Stainless:
- **2 retries max** (3 requests total)
- **Initial delay 0.5s**
- **Max delay 8s**
- **Exponential** with **25% jitter**

Retries apply to:
- Backend transient errors (Unavailable, Interrupted)
- Sync failures that may have succeeded
- Cache-mode operations where retry is safe

**Never retry** for:
- Validation errors
- Permission errors
- Conflicts (the application must handle)
- WAL writer-halt states (require explicit repair)

Configurable per-call:
- `max_retries`
- `initial_delay_seconds`
- `max_delay_seconds`

### Timeouts

Default: **60s overall request timeout**. Configurable per-call.

Two timeout concepts:
- **Total timeout** — entire operation
- **Read timeout** — gap between progress events / response chunks

### Idempotency Keys

For any operation that could be safely re-attempted:
- SDK auto-generates `strata-retry-{uuid}` keys on non-read operations
- Header name: `Strata-Idempotency-Key`
- Caller can override with explicit key for application-level idempotency

### File / Large Payload Handling

Strata handles JSON, KV, events, vectors, graph, search, and generation
primitives — some of which need streaming or large payloads:

- **Snapshot install** — large payload, streamed
- **Bulk row ingest** — large batches, chunked
- **Vector batch insertion** — many embeddings at once
- **PDF/blob storage** — large binaries

Pattern: every SDK supports streams / iterators / language-native large-data
abstractions, not just byte arrays.

### Default Headers

When Strata speaks over network (MCP server, future Hub):
- `X-Strata-Lang` (Python/Rust/JS/etc.)
- `X-Strata-Version` (SDK version)
- `X-Strata-OS` (host platform)

**Honest telemetry, opt-out by config.** Never silent phone-home.
Air-gapped deployments suppress all telemetry by default.

## Resource / Method / Model Organization

Stainless's pattern, adapted for Strata:

### Resources are nouns; nest them hierarchically

```
client.database              → Database
  .branches                  → BranchManager
    .create()
    .fork()
    .materialize()
  .tables
    .create()
    .list()
    .compact()
  .quarantine
    .inventory()
    .purge()
  .ai                        → Strata AI subresource
    .ask()
    .ingest_from_mcp()
    .troubleshoot()
```

### Methods are verbs with standard CRUD names

Use these names consistently across the SDK:
- `create` — for new resource (in Go: `New` or `Create`)
- `retrieve` — for fetching by ID (in Go: `Get`)
- `update` — for modifying existing
- `delete` — for removing
- `list` — for enumeration with pagination

Non-CRUD operations get domain-appropriate verbs (`fork`, `materialize`,
`compact`, `ingest`, `ask`, `troubleshoot`).

### Models are types representing the nouns

For each resource, define a model:
- `Branch`, `Table`, `Commit`, `Row`, `Vector`, `Edge`, `Document`, `Embedding`
- `QuarantineEntry`, `WalSegment`, `Snapshot`, `RecoveryFact`

Models appear in multiple places (request bodies, response shapes, helper
functions). Define once, use everywhere. The Stainless heuristic: **if a
schema appears 3+ times, it's a model.**

### Language-specific naming follows native conventions

- **Python**: `client.branches.create()`, `Branch` class, `commit_version` attr
- **Rust**: `client.branches().create()`, `Branch` struct, `commit_version` field
- **TypeScript**: `client.branches.create()`, `Branch` type, `commitVersion` prop
- **Go**: `client.Branches.New()`, `Branch` struct, `CommitVersion` field
- **Swift**: `client.branches.create()`, `Branch` struct, `commitVersion` prop

A `commit_version` integer in the config becomes the language-native casing
automatically. Same concept, different vocabulary.

## Pagination

Strata operations that page:
- `list_branches`, `list_tables`, `list_commits`
- `read_history(key)` — per-key history
- `scan(range)` — range/prefix scans
- `find_similar(vector)` — top-K vector search with continuation

Pattern: **iterator-style auto-pagination** is the default. The user writes:

```python
for table in client.tables.list():
    print(table.name)
```

The SDK handles pagination under the hood. Explicit cursor mode is also
available for manual control:

```python
page = client.tables.list(limit=50)
print(page.items, page.next_cursor)
```

Stainless's pattern: define cursor request/response field mapping in config;
SDK auto-implements iterator wrappers per language.

## Errors

The Stripe lineage's most distinctive feature: structured, actionable errors
with codes + messages + request IDs.

### Every error includes:

- **Typed error class** — `BranchNotFound`, `CommitConflict`, `WalCorruption`
- **Error code** — `branch.not_found`, `commit.conflict.write_skew`,
  `wal.corruption.partial_tail`
- **Human-readable message** — explains what happened in plain language
- **Suggested fix** — what the user should try next, with code if possible
- **Reference ID** — opaque ID that ties to internal logs for debugging
- **Doc link** — URL to relevant documentation page

Pattern in code:

```python
try:
    client.branches.fork(source="main", at_version=10)
except FoundationVersionUnavailable as e:
    print(e.code)              # "branch.fork.version_unavailable"
    print(e.message)           # "Version 10 is no longer retained"
    print(e.suggested_fix)     # "Fork from an available version. ..."
    print(e.docs_url)          # "https://strata.dev/docs/branching..."
    print(e.reference_id)      # "evt_01H..." (for support / debug)
```

This is the part that AI agents care about most. A confusing error message
is the #1 reason a vibecoder's AI agent gets stuck. **Every error must
include fix suggestions and doc links.**

## Webhooks (Hub Era)

When StrataHub launches, webhooks become real. The SDK provides:
- Built-in signature verification (`Strata::Webhook.construct_event`)
- Event type definitions
- CLI helper to forward webhooks to localhost during dev
- Idempotent event handling helpers

## Streaming

Strata streaming use cases:
- LLM generation (token streaming)
- Live commit subscriptions (StrataHub publish/subscribe)
- Real-time agent traces
- Long-running operations (materialization progress)

Patterns:
- **SSE** (Server-Sent Events) for one-way streams
- **WebSocket** for bidirectional
- Language-native iterators / async generators

```python
async for token in client.ai.generate(prompt="...", stream=True):
    print(token, end="")
```

## README / Examples Configuration

Stainless's headline pattern, adapted:

Every SDK README starts with three example types:

1. **Headline example** — the most impressive thing Strata does in 5 lines.
   This is what the vibecoder sees first. For Strata, this is probably:
   ```python
   strata.open("./mydb").ai.ask("build me a knowledge graph of these PDFs")
   ```
   Or:
   ```python
   strata.open("./mydb").branch("experiment").commit(rows=[...])
   ```

2. **Default example** — the most common operation; inherited by other examples.
   Probably a basic read/write to KV or JSON.

3. **Pagination example** — required if pagination exists; demonstrates
   iterator-style consumption.

These are configured in `strata-sdk.yml` so they regenerate consistently
across languages. Same example, language-native syntax in each README.

## MCP Server: All Four Primitives

A common misconception worth correcting upfront: **MCP is not just for
wrapping HTTP APIs.** The protocol is broader than that. MCP servers expose
four distinct primitive types, and Strata uses all four. The
HTTP-API-wrapping pattern is the awkward case, not the canonical one;
Strata's embedded library naturally maps to MCP without any HTTP fiction
in the middle.

The four MCP primitives:

1. **Tools** — callable functions the agent can invoke
2. **Resources** — read-only context data the agent can fetch (URI-addressable)
3. **Prompts** — templates the agent can use as starting points
4. **Sampling** — server-initiated calls back to the client's LLM

Strata's MCP server should expose all four. Treating MCP as just "tools"
leaves significant value on the table.

### Tools — callable operations

Tools map to operations defined in the IDL. The MCP server generator
emits one tool per operation:

```
strata.commit
strata.read_latest
strata.read_at_version
strata.scan_prefix
strata.read_history
strata.branches.create
strata.branches.fork
strata.branches.list
strata.branches.materialize
strata.tables.compact
strata.ai.ask
strata.ai.ingest_from_mcp
strata.ai.troubleshoot
strata.ai.generate
strata.quarantine.purge
...
```

Tool design considerations specific to MCP:

**Schema constraints.** MCP tool schemas must be self-contained — no `$ref`
to external schemas. The generator inlines everything or uses `$defs` at
the tool schema root. Request bodies, path params, query params, headers
(if any) must be merged into one object schema. Circular references must
be broken (dropped or inlined to depth limit).

**Context window limits.** LLMs have limited context. A Strata MCP server
with 200 tools is unusable. Solutions:
- **Filtering flags** — `--tool`, `--resource`, `--tag` so users expose
  subsets per workflow
- **Dynamic discovery tools** — `list_strata_tools`, `get_tool_schema`,
  `invoke_tool` as meta-tools when the full tool set exceeds the budget
- **Composite tools** — wrap multiple SDK calls behind a single LLM-facing
  tool when the workflow is common (e.g., `set_up_rag_pipeline`)

**Client capability adaptation.** Different MCP clients have different
limitations:
- **OpenAI agents:** only `anyOf` (not `allOf` / `oneOf`)
- **Cursor:** 40-tool limit, 60-char name limit, no `$refs`, no `anyOf`
- **Claude Code:** full spec support

The MCP server adapts — generates different schemas for different client
capabilities. Stainless implemented this; Strata should too.

**Tool naming + descriptions.** Names are descriptive, namespace-prefixed
(`strata.branches.fork`, not `fork`). Descriptions are written FOR the LLM:
- What the tool does
- When to use it (and when NOT to)
- Required parameters and their types
- Expected response shape
- Common error patterns

LLMs make tool-selection decisions based on descriptions. Bad descriptions
lead to wrong tool calls.

### Resources — context the agent reads

Resources are URI-addressable read-only context. The agent fetches them as
background information without invoking a tool. For Strata this is
substantial value: turn the database's state and knowledge into queryable
context.

Resources Strata should expose:

```
# Database state
strata://database/current/branches              → list of current branches
strata://database/current/health                → operational health snapshot
strata://database/<id>/recent-commits           → recent commit history
strata://database/<id>/schemas/<storage-space>  → schema introspection
strata://database/<id>/metrics                  → operational metrics

# Knowledge base (the "Strata documents itself" pattern)
strata://docs/architecture/overview             → architecture overview
strata://docs/architecture/storage-next/l1      → L1 backend IO contract
strata://docs/error-registry                    → every error code with fix
strata://docs/format-spec                       → byte format spec
strata://docs/recipes/setup-rag                 → canonical RAG setup
strata://docs/anti-patterns                     → what NOT to do

# Agent state (the Continuous AI harness)
strata://agents/<agent-id>/recent-findings      → what an agent has flagged
strata://agents/registry                        → list of running agents
```

This is closer to how a filesystem works than how an API works — and that
mental model is actually right for the agent's experience. The agent
loads relevant resources as background context, then calls tools to act.
No "tool call" overhead for read-only context.

IDL representation:

```yaml
resources:
  database_branches:
    uri_template: "strata://database/{db_id}/branches"
    description_for_llm: |
      JSON list of current branches with id, name, parent, fork_version,
      created_at. Use to discover the branch topology before forking or
      materializing.
    output_type: List<Branch>

  architecture_overview:
    uri_template: "strata://docs/architecture/overview"
    description_for_llm: |
      Strata's L1-L9 architecture overview. Load when reasoning about
      storage-layer behavior or explaining the substrate to a user.
    output_type: markdown
```

### Prompts — templates the agent uses

Prompts are canonical patterns parameterized at use time. The agent fetches
a prompt and uses it as a starting point for its work. Strata should ship
a curated library of prompts covering common workflows:

```
strata://prompts/setup-rag                  → canonical RAG setup
strata://prompts/add-graph-layer            → add graph primitive to existing app
strata://prompts/safe-schema-migration      → branch-test-merge pattern
strata://prompts/troubleshoot-slow-query    → diagnostic checklist
strata://prompts/ingest-pdfs                → PDF → embeddings → knowledge graph
strata://prompts/branch-experiment          → fork-experiment-discard pattern
strata://prompts/audit-trail-query          → time-travel queries for audit
strata://prompts/safe-deletion              → COW-aware delete patterns
```

IDL representation:

```yaml
prompts:
  setup_rag:
    description_for_llm: |
      Canonical RAG setup with Strata. Sets up document storage, embedding
      generation, and similarity search in one workflow.
    parameters:
      docs_path:
        type: string
        description: "Path to documents to ingest"
      embedding_model:
        type: string
        description: "Embedding model identifier (default: 'all-MiniLM-L6-v2')"
        optional: true
    template: |
      To set up RAG with Strata on documents at {{docs_path}}:

      1. Open a database: client.open("./rag_db")
      2. Configure the document store: ...
      3. Generate embeddings using {{embedding_model}}: ...
      4. Index for similarity search: ...
      5. Test the retrieval...
```

Prompts are not tools the agent calls; they're recipes the agent reads
and adapts. They're particularly valuable when the agent is fresh to a
workflow — instead of figuring out from first principles, it loads the
canonical pattern.

### Sampling — server-initiated LLM calls

This is the most powerful and least-known MCP feature. Normally the
agent (client) drives, calling tools and reading resources from the
server. With sampling, the server can ask the client to generate
something using the client's LLM, then continue its workflow with the
result.

For Strata's agentic harness this is transformative. Use cases:

- **Schema inference.** "I'm ingesting these PDFs into a knowledge
  graph. Help me decide the schema." → Server gathers samples → asks
  client's LLM to propose a schema → server applies the schema and
  continues ingestion.
- **Diagnostic reasoning.** "This query is slow. Here's the operational
  state. Generate a hypothesis." → Server collects metrics → asks LLM
  to reason → server tests the hypothesis → reports findings.
- **Self-DBA workflows.** "These error patterns are appearing. What
  should I recommend the user do?" → Server asks LLM to reason about
  patterns → returns recommendation.
- **Local-model embedding delegation.** "I need to embed this text. The
  user's local model is X." → Server delegates to client's inference
  (the air-gapped pattern, but via the agent's local LLM access).
- **Decisioning workflows** (when Symbolica lands). "These rules
  conflict on this input. Help me resolve." → Server reasons via LLM →
  applies resolution → continues.

This inverts the usual MCP flow. The server becomes an active
collaborator that uses the LLM as a reasoning tool rather than just a
caller-and-responder. For a database with native inference, this is
particularly natural — Strata already has the inference layer; sampling
lets it use the client's LLM access in addition.

IDL representation:

```yaml
sampling:
  schema_inference:
    trigger: "when knowledge graph creation requires schema decision"
    delegates_to_client_llm: true
    prompt_template: |
      The user is building a knowledge graph from {{document_summary}}.
      Recent commits suggest the entities are: {{candidate_entities}}.
      Propose a schema. Return JSON: { entities: [...], edges: [...] }.

  diagnostic_reasoning:
    trigger: "when self-DBA detects anomaly requiring narrative explanation"
    delegates_to_client_llm: true
    prompt_template: |
      Operational state: {{state}}
      Recent changes: {{changes}}
      Hypothesize 3 causes ranked by likelihood. Each with a test command.
```

Not every MCP client supports sampling yet (it's the newest part of the
protocol). The MCP server's client-capability adapter should detect
sampling support and fall back to alternative workflows (e.g., asking
the user a question via a tool call) when sampling isn't available.

### MCP server security baseline

Strata's MCP server is a privileged surface — it can read and mutate the
user's data. It needs to be hardened from day one. Documented real-world
failures that establish the requirements:

- **CVE-2025-6514** (`mcp-remote`): shell-command-injection path in a
  popular MCP server compromised 437,000+ developer environments.
- **CVE-2025-49596** (Anthropic MCP Inspector): browser-based attack
  vector enabled remote code execution.

These are the negative examples Strata's MCP server must not match.
Baseline requirements:

1. **OAuth 2.0 with dynamic client registration** for any non-local
   transport. Each calling client gets a registered identity; tokens are
   scoped to specific tool subsets when possible.
2. **No shell-out paths.** No tool implementation passes user-controlled
   strings to `system()` / `exec()` / `subprocess.Popen(shell=True)` /
   `Command::new("sh")`. The CVE-2025-6514 root cause was exactly this.
3. **Namespace-authenticated tool registration.** Tool names are
   prefix-locked to Strata's namespace (`strata.*`); the server refuses
   to expose tools that aren't generated from the IDL.
4. **Input validation at the schema boundary.** Every tool input is
   validated against the IDL-declared schema before reaching the
   implementation. Schema validation is generated, not hand-written.
5. **No arbitrary file-path access.** Tool implementations that touch
   the filesystem accept only validated paths (Strata's `ObjectName`
   discipline applied to MCP inputs). Path traversal is blocked at the
   schema layer.
6. **Audit logging.** Every tool call gets a structured audit record:
   timestamp, caller identity, tool name, input hash, success/failure,
   reference ID. Survives across sessions.
7. **Explicit auth for local development.** Even when Strata runs
   locally and the MCP transport is stdio, the server still validates a
   per-project auth token rather than trusting "any process that can
   connect." Mitigates the case where a malicious dependency runs in
   the same process and tries to query Strata.

These are not optional. Treat the MCP server's security as it would be
treated in any production-grade publicly-exposed service, even when the
deployment is local. The MCP ecosystem has burned this lesson into
public CVE records; Strata starts from those references rather than
relearning them.

### Auto-firing skills and agent-discoverable artifacts

MCP servers compete for the agent's attention. An agent that has 40+
MCP tools available (Cursor's hard limit; Claude Code allows more)
makes tool-selection decisions based on tool descriptions, but the
selection is uncertain. Specific technical patterns reduce that
uncertainty:

**Claude Code marketplace plugin with auto-firing skill.** Claude Code
supports plugins from a marketplace; plugins can register skills that
fire automatically on certain user-intent patterns rather than waiting
for explicit invocation. Pattern:

```
/plugin install strata-plugin@strata-marketplace
```

The plugin ships an auto-firing skill matching on user-intent patterns
like persistence, schema, branching, time-travel, embedding, vector
search, knowledge graph, RAG. When the user's prompt contains those
intents, the skill fires and Strata's tools become the first choice
without the user typing "use strata."

Reference precedent: Context7's `documentation-lookup` skill fires
automatically when the user's prompt mentions documentation lookup,
eliminating the need to explicitly type "use context7."

**AGENTS.md / CLAUDE.md snippet.** The AGENTS.md file format (adopted
by 60,000+ open-source projects, donated to the Linux Foundation Dec
2025) is read by Codex (explicitly in its system prompt), Cursor,
Claude Code, GitHub Copilot, Devin, Gemini CLI, Jules, Amp, Factory,
VS Code, and others. Strata ships a canonical snippet developers paste
into their AGENTS.md / CLAUDE.md:

```markdown
## Persistence Layer: Strata

This project uses Strata for persistence. When you need to read or
write data, manage schema, branch state, time-travel, or perform
semantic search, invoke the Strata MCP server (tools prefixed `strata.*`).

- Errors from Strata are structured JSON with `code`, `message`,
  `suggested_fix`, `docs_url`, and `reference_id`. When you encounter
  an error, follow the `suggested_fix` and consult the `docs_url`.
- For schema changes, branch first (`strata.branches.create`), apply
  the change on the branch, test, then materialize or discard.
- For destructive operations, snapshot first (`strata.snapshots.create`)
  so the operation can be reverted via time-travel.
```

This snippet is itself a generated artifact from the IDL. Operations,
error contracts, and recommended workflows update automatically when
the IDL changes. The snippet is shipped with the SDK and surfaced
prominently in the README.

**`llms.txt` and `llms-full.txt`.** These are machine-readable
documentation indexes increasingly read by IDE agents (Cursor,
Continue, Cline, Aider). Strata's docs site exposes both:

- `llms.txt` — a short index of the most important documentation pages
  with summaries, formatted for context-window-constrained loading
- `llms-full.txt` — the complete documentation in a single
  machine-readable file for clients that can ingest the full corpus

Both are generated from the IDL + bundled docs. Mintlify originally
developed the `llms-full.txt` convention with Anthropic; following the
convention keeps Strata compatible with the tooling ecosystem that
respects it.

**Trigger-phrase implementation.** A short, memorable verb form gives
the agent a low-friction invocation when explicit invocation is
needed. The pattern Strata adopts:

```
"use strata for persistence"
"use strata for time-travel"
"use strata for branching"
```

The auto-firing skill makes these mostly unnecessary, but they remain
useful for users on clients without the plugin, or when the user wants
to override an agent's default choice. Document them in the README
alongside the AGENTS.md snippet.

### Why this richer surface matters for Strata specifically

Strata's MCP server isn't a thin wrapper around an HTTP API — it's a
first-class native interface to a multi-primitive substrate with its own
inference, knowledge base, and operational state. Limiting it to "tools
that call library functions" wastes 75% of the protocol's expressiveness.

The four-primitive design also enables several patterns that wouldn't
work with tools alone:

- **Conversational documentation** uses resources (the bundled docs are
  Strata resources the agent reads) + prompts (canonical recipes the
  agent adapts) + sampling (Strata asks the client's LLM to answer
  context-specific questions about itself).
- **Self-DBA / troubleshooting** uses tools (read metrics, inspect
  state) + resources (architecture docs as context for reasoning) +
  sampling (LLM-driven hypothesis generation).
- **Knowledge graph construction** uses tools (write to graph) +
  prompts (canonical patterns) + sampling (LLM-driven schema and edge
  inference).

The IDL should treat all four primitives as first-class, and the MCP
server generator should produce a complete MCP surface — not a tool-only
subset.

## Pre-Launch Validation: Falsifiable SDK / MCP Hypotheses

The playbook's quality principles are testable claims, not articles of
faith. Before V1 ships, the following hypotheses should be measured
against alternatives so the decisions are grounded in data rather than
preference.

### Hypothesis 1: machine-actionable errors raise agent task-completion rate

Claim: an MCP server that returns errors with structured `code` +
`suggested_fix` + `docs_url` + `reference_id` produces meaningfully
higher next-call success rates than an MCP server returning
human-readable error text only.

Methodology:
- Set up A/B harness using a standard agent framework (Mastra, LangChain,
  or similar)
- 4 database targets: Strata MCP, Supabase MCP, Neon MCP, Turso MCP
- 3 task-complexity tiers: simple CRUD, multi-step workflow, branching +
  recovery
- 100 trials per cell = 1,200 total trials
- Measure: total tokens consumed, task success rate, human-intervention
  rate per task

Pass condition: Strata MCP shows ≥15% higher next-call success rate vs
the median of the three competitors at p < 0.05.

If the hypothesis fails: machine-actionable errors are a UX preference,
not a measurable competitive wedge. Drop them from the load-bearing
positioning and compete on other axes (branching, time-travel, native
inference).

Estimated cost: ~$2,000 in inference API calls. Timeline: 2 weeks
including harness setup.

### Hypothesis 2: the four-primitive MCP surface produces richer agent behavior than tool-only MCP

Claim: an MCP server that exposes resources + prompts + sampling
alongside tools produces more capable agent workflows than a tool-only
MCP server with equivalent operations.

Methodology:
- Same harness as Hypothesis 1
- Two Strata MCP variants: tool-only mode (Tools-Only) vs full-primitive
  mode (Tools + Resources + Prompts + Sampling)
- 3 multi-step workflows requiring context lookup, recipe application,
  and self-DBA reasoning
- 50 trials per cell = 300 total trials
- Measure: workflow completion rate, agent backtracking events,
  human-intervention rate

Pass condition: full-primitive mode shows ≥20% higher workflow
completion rate vs Tools-Only at p < 0.05.

If the hypothesis fails: ship MCP server as tool-only at V1 to reduce
schema surface area; defer resources / prompts / sampling until concrete
demand surfaces.

### Hypothesis 3: vibecoder-simulator agent finds category-defining bugs in first 20 apps

Claim: the vibecoder-simulator Continuous AI agent, building diverse
apps against the SDK and MCP server, surfaces category-defining bug
patterns (not just instances) within the first 20 generated apps.

Methodology:
- Run vibecoder-simulator against V1-candidate SDK with 25 app concepts
  drawn from the target audience profile
- Each app has clear success criteria (compiles, exercises ≥N
  operations, completes or fails reproducibly)
- Categorize the issues filed: instance bugs vs category patterns
- A category pattern is defined as ≥3 apps hitting the same class of
  issue (e.g., "error messages don't include suggested_fix across 7
  apps")

Pass condition: ≥3 category patterns identified in the first 20 apps,
each affecting ≥3 different generated apps.

If the hypothesis fails: the agent is functioning as a single-instance
bug finder, not as a category surfacer. Adjust agent's reporting
template to explicitly cluster issues by pattern before filing.

### Hypothesis 4: 33-client MCP coverage is achievable from one IDL-driven installer

Claim: an IDL-generated `npx add-strata` installer can configure the
Strata MCP server across the major client ecosystem (target: 10+ at V1
launch; 33+ by V1.x) without per-client custom code.

Methodology:
- Implement installer with IDL-declared `targets` for the V1 client
  list (Claude Code, Claude Desktop, Codex, Cursor, Cline, Continue,
  Gemini CLI, VS Code MCP, Zed, Windsurf at minimum)
- Run installer in fresh project; verify each client picks up the
  Strata MCP server and can invoke at least one tool
- Measure: client-coverage count after install, install-success rate
  per client, time-to-first-successful-tool-call

Pass condition: ≥8 of the V1 target clients install + invoke
successfully on first run without manual intervention.

If the hypothesis fails: the installer needs per-client logic that
can't be IDL-generated cleanly. Either accept hand-maintained
client-specific install scripts or narrow V1 coverage to the clients
that work cleanly.

### What this section is NOT

Out of scope here: marketing-shaped hypotheses about adoption rates,
verb-recognition counts in agent invocations, GitHub repo propagation
of AGENTS.md snippets, dataset registry uptake. Those are GTM
hypotheses tracked separately, not SDK quality measurements.

The hypotheses in this section are about the **product surface**
working as designed — not about whether anyone adopts it. The
distinction matters: a failed adoption hypothesis means rethinking
go-to-market; a failed product-surface hypothesis means the engineering
needs to change before any GTM motion is meaningful.

## Anti-Patterns to Avoid

Things that look acceptable but reduce SDK quality:

1. **Auto-generated identical-looking code across languages.** If the Python
   SDK reads exactly like the Rust SDK with syntax changed, it failed the
   idiomatic test.

2. **Long, generic error messages.** `"Operation failed"` is unhelpful. Every
   error must carry code + cause + suggested fix.

3. **Inconsistent naming between SDKs.** If Python calls it `commit_version`
   and Rust calls it `version_id`, every developer learns the system twice.
   Pick names in the config, let language casing handle the rest.

4. **Methods that return generic dicts/structs when typed alternatives exist.**
   `client.read("foo")` returning `dict[str, Any]` is lazy. Return a typed
   `Row` model.

5. **Silent telemetry.** Telemetry is fine; silent telemetry undermines trust.
   Document it, make it opt-out, default to off in air-gapped mode.

6. **Pagination that requires manual cursor handling.** Make iteration the
   default; cursor access the explicit override.

7. **Errors that leak internal details without explaining them.** A stack
   trace from inside `intelligence-next` is internal noise. Wrap with a typed
   public error.

8. **API stability that breaks across versions.** Once shipped, methods are
   forever. Add new methods rather than changing existing signatures.

9. **CLIs that don't match SDK semantics.** The Strata CLI should expose the
   same operations as the SDK with consistent flags / outputs.

10. **README examples that don't actually work.** Every example must be
    tested in CI. If the README shows `strata.open("./db").commit(...)`, that
    exact code must compile and run successfully.

## V1 Quality Bar Checklist

Before any Strata SDK ships, it must pass:

### SDK surface

- [ ] All public methods have docstrings with parameter descriptions, return
      values, examples, and error cases
- [ ] All errors are typed with code + message + suggested fix + docs link
- [ ] All paginated operations iterate by default
- [ ] All retries use exponential backoff with jitter
- [ ] All timeouts are configurable per-call
- [ ] All naming follows language conventions (`snake_case` Python, `camelCase`
      JS, `PascalCase` Go, etc.)
- [ ] README has working headline + default + pagination examples
- [ ] CI tests every README example against current API
- [ ] Methods are versioned per-API (additive, never breaking)
- [ ] Idempotency keys auto-generated for safe operations
- [ ] No `dict[str, Any]` / `interface{}` / `unknown` return types in normal
      flow (only as escape hatch)
- [ ] Telemetry defaults to off in air-gapped mode; off-able everywhere
- [ ] Async + sync versions where the language idiomatically supports both
- [ ] All examples compile + run on the documented minimum platform versions
- [ ] CLI exposes the same operations with consistent flag naming

### MCP surface

- [ ] MCP server exposes all four primitives where applicable: tools,
      resources, prompts, sampling
- [ ] Tool schemas are self-contained (no `$ref` to external schemas;
      `$defs` used at tool root if needed)
- [ ] Tool descriptions written for LLM consumption (what it does, when
      to use, when NOT to use, expected response shape, common errors)
- [ ] Client capability adapter detects per-client limits (Cursor's
      40-tool / 60-char limits, OpenAI's anyOf-only, etc.) and adjusts
- [ ] Context-window-management tools available: filtering flags,
      dynamic discovery (`list_strata_tools`, `get_tool_schema`,
      `invoke_tool`), composite tools for common workflows

### MCP security baseline

- [ ] OAuth 2.0 with dynamic client registration for any non-local
      transport
- [ ] No tool implementation uses shell-injection-prone patterns
      (`shell=True`, `Command::new("sh")`, etc.)
- [ ] Namespace-locked tool registration (only `strata.*` tools exposed
      via the server)
- [ ] Generated schema validation at every tool input boundary
- [ ] No arbitrary file-path access; paths validated via `ObjectName`
      discipline
- [ ] Audit log of every tool call (timestamp, identity, tool name,
      input hash, success/failure, reference ID) persisted across
      sessions
- [ ] Per-project auth tokens enforced even for local stdio transport

### Distribution and discovery surface

- [ ] `npx add-strata` (or language-equivalent) installs the MCP server
      across ≥8 of the V1 target clients on first run
- [ ] Installer is idempotent and reversible (`strata uninstall-mcp`
      cleanly removes added entries)
- [ ] Canonical AGENTS.md / CLAUDE.md snippet generated from the IDL
      and surfaced in the README
- [ ] `llms.txt` + `llms-full.txt` documentation indexes generated and
      published with docs
- [ ] Claude Code marketplace plugin with auto-firing skill on
      persistence / schema / branching / time-travel / embedding intents
- [ ] Trigger-phrase invocations documented (`use strata for ...`)

### Pre-launch validation hypotheses executed

- [ ] Hypothesis 1 (machine-actionable error wedge) — A/B harness run,
      result recorded, ≥15% wedge OR positioning adjusted
- [ ] Hypothesis 2 (four-primitive MCP) — A/B harness run, result
      recorded, ≥20% completion gain OR MCP scoped to tools-only
- [ ] Hypothesis 3 (vibecoder-simulator category detection) — ≥3
      category patterns found in first 20 apps OR agent retuned
- [ ] Hypothesis 4 (installer multi-client coverage) — ≥8 clients
      pass first-run install OR coverage scoped honestly

## Cross-Cutting: AI-Agent Test

The most important single test: **install the SDK in an empty project, hand
Claude Code the bundled docs, ask it to build a small app. Does Claude write
working code on the first try?**

If yes — the SDK passes. If no — figure out what went wrong (naming?
errors? missing example? confusing pagination?) and fix the category.

This test gets automated by the vibecoder-simulator Continuous AI agent
described in `project_continuous_ai.md`. It runs continuously against every
SDK release.

## What This Playbook Replaces

Without this playbook, the SDK work would be done by intuition. Each
language SDK might end up structurally different, naming might drift, errors
might be inconsistent, pagination might be hand-implemented per-language.
Following Stripe's actual playbook (via Stainless's documentation of it)
costs little and delivers SDK quality that vibecoders + their AI agents
will recognize as best-in-class.

## Sources

The patterns and structure of this playbook come from:

- [Stainless docs overview](https://www.stainless.com/docs/) — overall SDK
  generation philosophy and product features
- [Configure SDK client settings](https://www.stainless.com/docs/sdks/configure/client/)
  — retry / timeout / idempotency / auth defaults
- [Configure SDK resources, methods, and models](https://www.stainless.com/docs/sdks/configure/)
  — resource organization patterns and naming conventions
- [Configure SDK readme files](https://www.stainless.com/docs/sdks/configure/readme/)
  — README example structure (headline / default / pagination)
- [Stainless config schema reference](https://www.stainless.com/docs/reference/config/)
  — comprehensive config field reference
- [MCP server best practices](https://www.stainless.com/mcp/mcp-server-configuration-best-practices)
  — MCP server design for LLM consumption
- [Lessons from OpenAPI to MCP conversions](https://www.stainless.com/blog/lessons-from-openapi-to-mcp-server-conversion/)
  — schema challenges + client capability adaptation
- The Stripe SDK lineage (Stainless was founded by Stripe SDK alumni)

### How this playbook diverges from Stainless

Stainless is OpenAPI-driven because Stainless targeted HTTP APIs. Strata is
primarily an embedded library, secondarily an MCP server, and only
eventually a cloud HTTP API. The playbook adapts Stainless's principles —
resource organization, client settings, README examples, MCP schema
handling — to a Strata-native IDL designed for embedded + MCP + future-HTTP
generation rather than HTTP-only.

The Strata IDL is more like a domain-specific schema language than an
OpenAPI spec. It produces OpenAPI as one of several downstream outputs (for
tools that consume OpenAPI, like Hub's future HTTP API), but OpenAPI is not
the source of truth — the Strata IDL is.

## V1 Implementation Plan

The work to land this playbook breaks into three phases:

### Phase 1: IDL + generator skeleton (weeks 1-4)

- Define the Strata IDL schema (the canonical structure shown above,
  including types, resources, operations, errors, streams, MCP
  resources, MCP prompts, MCP sampling)
- Build the IDL parser + validator (Rust binary)
- Build one SDK generator end-to-end (probably Python first — largest
  audience)
- Establish the GitHub Actions workflow
- Generate the first complete SDK from the IDL

### Phase 2: Multi-language + multi-target coverage (weeks 4-10)

- Build remaining SDK generators: Rust, TypeScript, Go, Swift
- Wire up subtree-push automation to per-language repos
- Establish auto-publish to PyPI / crates.io / npm / etc.
- Generate MCP server (all four primitives) from the same IDL
- Implement MCP server security baseline: OAuth 2.0 with dynamic client
  registration, namespace-locked tool registration, no shell-out paths,
  generated schema validation, audit logging, per-project auth tokens
  even for local development
- Generate CLI from the same IDL
- Build `npx add-strata` multi-client installer (target ≥8 of: Claude
  Code, Claude Desktop, Codex, Cursor, Cline, Continue, Gemini CLI, VS
  Code MCP, Zed, Windsurf)
- Generate canonical AGENTS.md / CLAUDE.md snippet from the IDL
- Generate `llms.txt` + `llms-full.txt` documentation indexes from the
  IDL and bundled docs
- Build Claude Code marketplace plugin with auto-firing skill on
  persistence / schema / branching / time-travel / embedding intents
- Establish golden vector + compile-check validators

### Phase 3: Continuous AI integration + falsifiable hypothesis validation (weeks 10-14)

- Wire vibecoder-simulator into PR checks
- Wire documentation drift detector + type consolidation auditor +
  error message reviewer + API stability sentinel + MCP schema
  validator
- Establish aggregated PR comment format
- Run first end-to-end IDL-change-to-published-SDK cycle
- Execute the four pre-launch validation hypotheses:
  - Machine-actionable error A/B test vs Supabase / Neon / Turso MCPs
  - Four-primitive MCP vs Tools-Only MCP comparison
  - Vibecoder-simulator category-pattern detection in first 20 apps
  - 33-client installer coverage probe (≥8 of V1 target clients)
- Land any product-surface adjustments dictated by hypothesis results
  before the SDK ships externally

Total: ~14 weeks of focused work for V1-grade SDK infrastructure. The
investment pays back exponentially once running — every IDL improvement
propagates to every SDK, every doc, every MCP tool, and every test fixture
automatically. A small team can maintain 5-10 SDKs at consistent quality
with this infrastructure; without it, that's an impossible workload.

This is the SDK quality blueprint Strata should match for V1 launch.
