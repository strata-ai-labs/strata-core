---
name: strata
description: Use when working with StrataDB / stratadb — an embedded multi-model database (key-value, JSON documents, vectors, events, graphs) with git-style branches and time travel. Covers opening databases from Python or the CLI, choosing a primitive, branch isolation, as-of reads, typed error handling, and built-in inference (chat, embeddings, reranking).
---

# StrataDB {version}

StrataDB is an embedded database: it runs inside the process against a local
directory, like SQLite or DuckDB — no server, no port, no connection string.
One database holds five data primitives on one branch-aware, time-travelling
substrate. This skill matches strata {version}; the binary and the wheel are
self-describing (see "Deeper reference" below).

## Open a database (Python)

```python
import stratadb

db = stratadb.open("./mydb")       # durable directory (created if absent)
db = stratadb.open(cache=True)     # ephemeral, in-memory
db = stratadb.from_env()           # path from $STRATA_DB
```

Never rely on the current directory — `stratadb.open()` with no target raises
`invalid_argument.cli.no_database`. Use a context manager or `db.close()`.

## Choose a primitive

| Data shape | Use | Namespace |
|---|---|---|
| Opaque value by key | key-value | `db.kv` — values are bytes; `str` encodes as UTF-8 |
| Structured record, addressed by field | JSON documents | `db.json` — read/write inside a doc by path (`"$.name"`) |
| Embeddings, similarity search | vectors | `db.vectors` — collections fix dimension + metric |
| Ordered, append-only history | events | `db.events` — hash-chained; count is `len()` |
| Relationships, traversal, analytics | graph | `db.graphs` — both edge endpoints must exist first |

```python
db.kv.put("greeting", "hello");  db.kv.get("greeting")        # b"hello"
db.json.set("user:1", "$", {"name": "Ada"});  db.json.get("user:1", "$.name")
db.vectors.create_collection("notes", dimension=384)
db.vectors.upsert("notes", "n1", vec, metadata={"kind": "note"})
db.vectors.query("notes", query_vec, k=5)
db.events.append("deploy", {"ok": True})
db.graphs.create("g"); db.graphs.add_node("g", "a"); db.graphs.add_node("g", "b")
db.graphs.add_edge("g", "a", "link", "b")
```

## Branches and time travel

Every primitive is branch-aware. A fork is cheap and isolates writes; the
default branch is named `default` (not `main`):

```python
db.branches.fork("default", "experiment")
exp = db.at(branch="experiment")       # scoped view over the same handle
exp.kv.put("k", "risky-value")
db.kv.get("k")                         # None on default — isolated
```

Every write returns a receipt; pass its commit timestamp back to any read:

```python
r = db.kv.put("k", "v1")
db.kv.put("k", "v2")
db.kv.get("k", as_of=r.commit.timestamp)   # b"v1"
```

There is no merge command in this line — bring work across branches by
re-applying writes.

## Errors: recover by code, never by message

Failures raise typed exceptions from `stratadb.errors` carrying a stable code
(`<class>.<area>.<detail>`), a hint, and a per-code URL
(`https://stratadb.org/e/<code>`). **A miss is not an error** — reading an
absent key/document/path returns `None`.

```python
from stratadb import errors

try:
    db.branches.fork("default", "experiment")
except errors.AlreadyExistsError:
    pass                                   # idempotent setup
except errors.StrataError as e:
    handle(e.code)                         # match codes, not message text
```

## Inference (`db.ai`)

Chat, embeddings, and reranking on the database handle — OpenAI-shaped.
Cloud providers need your key (`OPENAI_API_KEY` / `ANTHROPIC_API_KEY` /
`GOOGLE_API_KEY`, or `strata config set <provider>.api_key <KEY>`); Strata
ships none.

```python
r = db.ai.chat("Summarize this.", model="openai:gpt-4o-mini", max_tokens=100)
e = db.ai.embed(["hello"], model="openai:text-embedding-3-small")
db.vectors.upsert("notes", "n1", e.vector)     # embed → upsert is the RAG loop
db.ai.capability("openai:gpt-4o-mini")         # offline: what a model supports
```

## CLI equivalents

The `strata` binary speaks the same commands, value shapes, and error codes:

```
strata ./mydb kv put greeting hello
strata ./mydb kv get greeting
strata --json ./mydb kv get greeting     # {"type": ..., "data": ...} envelope
strata ./mydb branch fork default experiment
strata <db> mcp serve                    # MCP over stdio (~20 tools)
```

## Deeper reference

- `stratadb.agents_guide()` — the complete Python guide, embedded in the wheel
- `strata agents guide` / `strata agents commands --json` / `strata agents errors --json`
- https://stratadb.org/llms.txt (append `.md` to any docs URL for CommonMark)
- `strata doctor` — coded diagnostics when anything is off
