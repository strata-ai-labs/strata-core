<div align="center">

# Strata

**Branch, time-travel, and search your data like code.**

The embedded database for the agent era — five data models, git-like branching,
and built-in time travel. One binary, one directory, zero infrastructure.

[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.88%2B-orange.svg)](https://www.rust-lang.org)
[![Version](https://img.shields.io/badge/version-1.0.0-brightgreen.svg)](https://stratadb.org/changelog)

[Website](https://stratadb.org) · [Documentation](https://stratadb.org/docs) · [Playground](https://stratadb.org/playground)

</div>

---

Strata is what a database looks like when versioning isn't an afterthought. It runs inside your process like SQLite — no server, no containers, no ops — but every write is versioned, any state can be forked instantly, and any moment in history can be read back, across key-value pairs, JSON documents, event logs, vectors, and graphs.

```bash
strata ./mydb kv put user:ada '{"role":"engineer"}'

strata ./mydb branch fork default experiment              # instant copy-on-write fork
strata ./mydb --branch experiment kv put user:ada '{"role":"cto"}'

strata ./mydb --branch experiment kv get user:ada         # {"role":"cto"}
strata ./mydb kv get user:ada                             # {"role":"engineer"} — untouched
```

Run an experiment on a fork. Let an agent loose on a branch. Read yesterday's state without having built a snapshot system. Delete the branch, and it never happened.

## Highlights

- 🌿 **Branch anything, instantly.** Forks are copy-on-write and constant-time regardless of database size. Branches isolate *all* data models at once.
- ⏳ **Time travel is built in.** Every write gets a version and timestamp. Read any key, document, event range, vector search, or graph *as of* any past moment with `--as-of`.
- 🧩 **Five data models, one engine.** KV, JSON documents, append-only events, vector search, and property graphs share one storage substrate, one branch model, one history.
- 📦 **Embedded, like SQLite.** A single binary and a single data directory. Use it as a Rust library, a CLI, or an MCP server. It also runs in the browser via WebAssembly.
- 🤖 **Agent-native.** `strata mcp serve` exposes the database to Claude, Cursor, or any MCP client. Every command emits clean JSON with `--json`. Events are hash-chained for tamper-evident audit trails.
- 🛡️ **Durable by default.** Write-ahead log, crash recovery, and explicit durability modes — or run pure in-memory with `--cache` when persistence is noise.

## The five data models

Point any command at a database path (or `--cache` for ephemeral in-memory). Every command accepts `--branch` and `--space`, and reads accept `--as-of`.

**Key-value** — working memory with full version history:

```bash
strata ./mydb kv put user:ada '{"name":"Ada","role":"engineer"}'
strata ./mydb kv history user:ada
strata ./mydb kv list --prefix user:
```

**JSON documents** — path-level reads and writes, secondary indexes:

```bash
strata ./mydb json set config '$.model' '"claude"'
strata ./mydb json get config '$.model'          # "claude"
```

**Events** — append-only, hash-chained, verifiable:

```bash
strata ./mydb event append tool_call '{"tool":"search","query":"docs"}'
strata ./mydb event list --event-type tool_call
strata ./mydb event verify-chain                 # sequence density + hash linkage
```

**Vectors** — similarity search with metadata filters:

```bash
strata ./mydb vector collection create embeddings 384
strata ./mydb vector upsert embeddings doc1 @embedding.json --metadata '{"title":"intro"}'
strata ./mydb vector query embeddings @query.json -k 5
```

**Graph** — property graphs with real algorithms, not just traversal:

```bash
strata ./mydb graph create social
strata ./mydb graph add-edge social ada knows lin
strata ./mydb graph pagerank social              # also: wcc, sssp, cdlp, lcc, neighbors
strata ./mydb graph bulk-insert social --file graph.json
```

## Branching and time travel

Branches are the core abstraction, not a bolt-on. A fork captures every data model at a point in time; branches then evolve independently.

```bash
# Agent A explores on its own branch; production is untouchable from there
strata ./mydb branch fork default agent-a
strata ./mydb --branch agent-a kv put plan '{"step":1}'

# Time travel: read state as of any past timestamp — on any branch
strata ./mydb kv get user:ada --as-of 1783660565504764
strata ./mydb vector query embeddings @query.json --as-of 1783660565504764

# Keep the branch, or make it never have happened
strata ./mydb branch delete agent-a
```

This is what makes Strata fit agents: exploration is cheap, mistakes are disposable, and every state an agent ever produced remains inspectable after the fact.

## Built for agents

```bash
strata --db ./agent-memory mcp serve       # MCP server over stdio — plug into Claude, Cursor, ...
strata ./mydb agents guide                 # self-describing surface, written for LLMs
strata ./mydb kv get user:ada --json       # every command speaks compact JSON
```

Model execution is in the box too — run local GGUF models or call cloud providers for embeddings and generation:

```bash
strata inference models list
strata inference embed <model> "how do branches work?"
strata inference generate <model> "summarize this changelog"
```

## Use it as a library

The same engine, embedded in your Rust process:

```rust
use stratadb::{BranchName, CacheOpenOptions, Database, KvKey, KvValue, ProductSpace};

let mut db = Database::open_cache(CacheOpenOptions::new())?.into_database();
let mut kv = db.kv(
    BranchName::new("default")?,
    ProductSpace::new("default")?,
)?;

kv.put(KvKey::new("greeting")?, KvValue::new(b"hello".to_vec()))?;
assert!(kv.get(&KvKey::new("greeting")?)?.is_some());
```

Durable databases open the same way with `Database::open_local(path, DurableLocalOpenOptions::new())`. The browser build (`crates/wasm`) exposes the cache-mode engine to JavaScript via WebAssembly.

## Install

Package releases (crates.io, PyPI, npm, Homebrew) are on the way. Today, build from source:

```bash
git clone https://github.com/stratalab/strata-core
cd strata-core
cargo install --path crates/cli     # installs the `strata` binary

strata init
strata --cache ping                 # pong 1.0.0
```

Rust 1.88+ required.

## How it works

Under every data model sits a single branch-aware MVCC storage engine: one write-ahead log, one commit clock, one copy-on-write branch tree. KV, JSON, events, vectors, and graph are capabilities layered over that substrate — which is why a fork captures all of them atomically and why time travel works uniformly everywhere. Durable databases recover from crashes via WAL replay; cache mode skips persistence entirely and lives in memory.

A database is a directory. Copy it, back it up, `rsync` it — it behaves the way embedded databases should.

Deeper internals live in [`docs/architecture/`](docs/architecture/).

## Status

Strata 1.0 is complete and hardening toward its public release: package channels, the Python and TypeScript SDKs, and the hosted docs are landing next. The on-disk format, error codes, and CLI surface documented here are the stable 1.0 contracts.

## License

[Apache 2.0](LICENSE)
