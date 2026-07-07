# stratadb (Python) — V1 SDK specification

Prereq reading: `00-shared-contracts.md`. This is a **ground-up specification**:
the existing wheel binds the old engine and none of its binding code survives
the V1 cutover. The repo, PyPI name (`stratadb`), and platform matrix carry
over; the architecture, API surface, and error model below are new.

Status of upstream inputs: the executor command boundary, error registry, agents
guide, and 32-command resolved catalog are shipped in `stratalab/strata-core`
(`v1` branch). The full-catalog IDL (all families) and schemars schemas are the
IDL workstream's next phases — Phase 2 below consumes them when they land.

---

## 1. Positioning and principles

Strata is an **embedded** multi-model database — SQLite-shaped, not a service.
The Python SDK is a native in-process binding, not a client. Non-negotiables
inherited from the executor charter and the first-run design:

- **P6 — same verbs everywhere.** The SDK exposes the executor's command
  surface under the same names, the same JSON value shapes, and the same error
  codes as the CLI and MCP. Learning one channel is learning all of them.
- **One canonical path per operation.** Curated methods are ergonomic wrappers
  over the same serialized command boundary the CLI's `command run` uses —
  never a second implementation of any semantics.
- **The wheel is the documentation.** Guide, docstrings, stubs, and README all
  generate from the executor's own metadata; drift is a CI failure.
- **Engine owns semantics.** The SDK never invents behavior: no client-side
  validation beyond type conversion, no retries, no defaulting the engine
  doesn't do.

## 2. Repo strategy

Mirror strata-core's pattern: a `v1` branch in the existing repo as a clean
break. No compatibility shims with the old binding, no migration tooling for
pre-V1 databases (they are rejected by the engine with structured format
errors). The old wheel keeps shipping from `main` until the M9 cutover; at V1
promotion the package version jumps to the unified engine version (`1.0.0`)
and `main` is replaced.

## 3. Architecture: three layers

```
┌─ curated namespaces (handwritten Python)  db.kv / db.json / db.vectors / …
├─ generated core (from the resolved IDL)   models + one typed method per command
└─ native binding (PyO3, abi3)              _stratadb.Handle: open/execute/close
```

### 3.1 Native binding (`_stratadb`, PyO3)

The binding is deliberately tiny — three operations:

```
Handle.open_durable(path: str) -> Handle      # exclusive process lock, like SQLite
Handle.open_cache() -> Handle                 # in-memory, per-process
Handle.execute(command_json: str) -> str      # serialized Command in, Output envelope out
Handle.close()                                # explicit close; Drop also closes
```

- `execute` crosses the boundary as serialized JSON (the executor's `Command`/
  `Output` wire, `deny_unknown_fields`) — the exact machinery behind the CLI's
  `command run --command-json`. Rust-side, this is
  `strata_executor_next::Executor` linked in-process; the wheel embeds the
  engine, no external binary.
- Errors return as the shared error envelope (shared contracts §4); the
  binding raises a single `_StrataNativeError(payload_json)` that layer 3 maps
  to the typed hierarchy.
- **Concurrency**: the executor is `&mut self`. The handle wraps it in a
  `Mutex`; the GIL is released for the duration of each engine call
  (`py.allow_threads`), so threads interleave but calls on one handle
  serialize. Document this plainly. One durable database = one exclusive
  process lock (attempting a second open surfaces the engine's lock error).
- Also exposed: `Handle.set_scope(branch, space)` (executor session context),
  `version() -> str`, and `agents_guide() -> str` (embedded at build time).

Everything else is pure Python — debuggable, patchable, readable by agents in
`site-packages`.

### 3.2 Generated core (IDL Phase 2)

Generated from the resolved `command-index.json` (the artifact that ships in
the binary, at `stratadb.org/idl/v1/`, and inside this wheel):

- One typed method per cataloged command on `stratadb.core.Commands`
  (`kv_put(...)`, `vector_query(...)`, …) with docstrings from the catalog's
  title/summary/errors — signatures mirror wire field names exactly.
- Typed models for the shared envelopes (schemars → generated dataclasses or
  TypedDicts): `CommitReceipt`, `MutationEffect`, `PageInfo`, `BatchResult`,
  `ErrorStatus`, per-primitive DTOs.
- **Until the full catalog lands** (32/~90 commands today), the core is the
  untyped escape hatch below, and curated methods construct wire JSON
  directly. The generator replaces their internals without changing the
  public API — build Phase 1 without waiting.

Escape hatch (public, permanent — the long tail is always reachable):

```python
db.execute({"type": "kv_scan", "limit": 10}) -> dict   # {"type": ..., "data": ...}
```

### 3.3 Curated namespaces (the public API — handwritten)

Full Stainless-style ergonomic codegen was explicitly rejected (IDL decision
#2); the namespaces below are handwritten and stable.

## 4. The public API

### 4.1 Entry point and scoping

```python
import stratadb

db = stratadb.Strata("./app-data")                  # durable (creates if absent)
db = stratadb.Strata(cache=True)                    # ephemeral, in-memory
# NEVER opens cwd implicitly; no-arg without cache=True raises InvalidArgumentError
# honoring the D2 contract. (STRATA_DB env fallback: honored only via
# Strata.from_env(), explicit by name.)

db.close()                                          # or: with stratadb.Strata(...) as db:

scoped = db.at(branch="experiment")                 # cheap scoped view; also space=
scoped.kv.get("k")                                  # every namespace honors the scope
```

`Strata` composition: `kv`, `json`, `vectors`, `events`, `graphs`, `branches`,
`spaces`, `admin`, `arrow` namespaces (lazily constructed), plus `execute()`,
`close()`, `version`, `at()`.

### 4.2 Common types and conventions

| Wire concept | Python |
|---|---|
| Bytes (KV keys/values) | accept `str \| bytes` (str encodes UTF-8); return `bytes` |
| JSON value | any JSON-serializable object |
| Commit receipt | `Receipt(version: int, timestamp: int, durable: bool, put_count: int, delete_count: int)` |
| Mutation effect | `Effect(applied: bool, kind: str, matched: bool, affected_count: int)` on write results |
| Maybe-read | `None` for miss (no exceptions for absence) |
| Page | `Page(items: list[T], has_more: bool, cursor: str \| None)` — cursors are opaque; pass back verbatim |
| Timestamps | `int` (commit-timestamp domain; from `Receipt.timestamp`) — every read namespace takes `as_of: int \| None` |
| Batch results | `BatchResult(status: str, items: list[BatchItem])`, per-item effect/error |

Every write returns a result object carrying `Receipt` + `Effect`. Every list
surface offers both the page call and an auto-paginating iterator
(`iter_*`), which loops the cursor internally.

### 4.3 Namespace surface (complete)

**`db.kv`** — 13 commands:

```python
put(key, value) -> WriteResult
get(key, *, as_of=None) -> bytes | None
get_entry(key, *, as_of=None) -> VersionedValue | None      # value+version+timestamp
delete(key) -> DeleteResult
exists(key) -> bool
history(key) -> list[HistoryEntry] | None                    # kv_getv; tombstones included
keys(prefix=None, *, limit=None, cursor=None, as_of=None) -> Page[bytes]     # kv_list
iter_keys(prefix=None, *, as_of=None) -> Iterator[bytes]
scan(start=None, *, limit=None, cursor=None) -> Page[ScanRow]                # kv_scan
iter_rows(start=None) -> Iterator[ScanRow]
count(prefix=None) -> int
sample(prefix=None, *, count=None) -> Sample                 # total_count + rows
put_many(entries: dict | list[tuple]) -> BatchResult         # kv_batch_put
get_many(keys) -> list[bytes | None]                         # kv_batch_get
delete_many(keys) -> BatchResult
exists_many(keys) -> list[bool]
```

**`db.json`** — 14 commands: `set(key, path, value)`, `get(key, path, *,
as_of=None)`, `get_entry`, `delete(key, path)`, `exists(key)`,
`history(key)`, `keys(prefix=None, …) -> Page[str]` (+ iterator),
`count(prefix=None)`, `sample(...)`, `set_many/get_many/delete_many`,
`create_index(name, field_path, *, index_type="tag")`, `drop_index(name)`,
`list_indexes()`. Document ids are `str`. Paths are JSONPath-lite (`$`,
`$.field`, `$.arr[0]`).

**`db.vectors`** — 19 commands:

```python
create_collection(name, dimension, *, metric="cosine") -> CollectionResult
delete_collection(name); list_collections() -> Page[CollectionInfo]
stats(name) -> CollectionInfo; count(name) -> int
upsert(collection, key, vector: Sequence[float], *, metadata=None) -> VectorWriteResult
get(collection, key, *, as_of=None) -> VectorEntry | None
history(collection, key); exists(collection, key) -> bool
keys(collection, *, limit=None, cursor=None) -> Page[str]
update_metadata(collection, key, metadata) -> VectorWriteResult
delete(collection, key); delete_all(collection)
delete_by_filter(collection, filter) -> BulkDeleteResult
query(collection, vector, *, k=10, filter=None, as_of=None) -> list[Match]
index_query(...) -> (list[Match], IndexDiagnostics)
upsert_many / get_many / delete_many -> BatchResult
```

Filters: V1 supports AND-composed equality only. Ship the helper so nobody
hand-writes the tagged wire shape:

```python
from stratadb import filters
filters.eq("kind", "note") & filters.eq("rank", 5)
# → {"conditions":[{"field":"kind","op":"eq","value":{"type":"string","value":"note"}}, …]}
```

**`db.events`** — 10 commands: `append(event_type, payload) -> AppendResult`
(sequence + receipt), `append_many(entries)`, `get(sequence, *, as_of=None)`,
`exists(sequence)`, `len(*, as_of=None)` (also `__len__` on the namespace),
`range(start, *, end=None, limit=None, reverse=False, event_type=None)`,
`range_by_time(start_ts, *, end_ts=None, …)` (**occurrence-time domain** — the
one intentionally wall-clock API), `list(event_type=None, *, limit=None,
after_sequence=None, as_of=None)`, `types(*, as_of=None)`, `verify_chain() ->
ChainVerification`. Events are immutable and hash-chained; expose
`previous_hash`/`hash` on records.

**`db.graphs`** — 14 commands: `create(name)`, `delete(name)`, `list()`,
`meta(name)`, `add_node(graph, node_id, *, properties=None)`,
`get_node(graph, node_id, *, as_of=None)`, `remove_node`, `list_nodes(graph,
*, limit=None, cursor=None, as_of=None)`, `add_edge(graph, src, edge_type,
dst, *, weight=None, properties=None)`, `get_edge(…, as_of=None)`,
`remove_edge`, `neighbors(graph, node_id, *, direction="outgoing",
edge_type=None, limit=None, as_of=None)`, `bindings_for_entity(target)`,
`batch_write(operations)` (atomic).

**`db.branches`** — 7 commands: `list()`, `get(name)`, `create(name)` (empty
root), `fork(source, branch, *, version=None, timestamp=None)` (current /
at-version / at-timestamp — mutually exclusive anchors), `delete(name)`.

**`db.spaces`** — 4 commands: `list()`, `create(name)`, `exists(name)`,
`delete(name)` (refuses when non-empty).

**`db.admin`** — `ping()`, `info()`, `health()`, `metrics()`, `describe()`,
`config()`, `config_value(key)`.

**`db.arrow`** — `import_(target, path, **options)`, `export(primitive, path,
*, format="parquet", **options)`.

**Module level** — `stratadb.agents_guide() -> str`, `stratadb.__version__`
(== engine version), `stratadb.errors` (the exception hierarchy),
`stratadb.filters`.

## 5. Error model

One base exception, one subclass per public error class (15 today; the enum is
`#[non_exhaustive]` — unknown classes map to the base):

```python
class StrataError(Exception):
    code: str                # "not_found.engine.branch" — stable, match on this
    error_class: str         # "not_found"
    message: str             # human text — never match on it
    hint: str                # suggested_fix
    ref: str                 # https://stratadb.org/e/<code>
    reference_id: str
    retry_policy: str        # never|after_state_change|same_request|idempotent_only|unknown
    retryable: bool
    commit_outcome: str      # for writes: not_started|not_applicable|…
    details: dict; hints: list[str]

NotFoundError, AlreadyExistsError, InvalidArgumentError, FailedPreconditionError,
AccessDeniedError, ConflictError, AmbiguousCommitError, HistoryUnavailableError,
UnsupportedError, ResourceExhaustedError, UnavailableError, IoError,
CorruptionError, SerializationError, InternalError
```

Misses are `None`, not exceptions. Out-of-range time travel is
`HistoryUnavailableError` (the engine's F7/F8 contract), distinct from
`NotFoundError` — preserve that distinction. `str(error)` renders
`code: message\n  hint: …\n  ref: …` (same shape as the CLI).

## 6. Agent surfaces

- `stratadb.agents_guide()` — byte-identical to `strata agents guide` for the
  same version (embedded at build time; CI guard).
- PyPI long-description = full quickstart inline (agents read `site-packages`
  and metadata before the web); taught commands mirror the binary's
  `strata init --json` → `next_steps`.
- `py.typed` + complete stubs; every public method's docstring carries a
  one-line example, generated from the command catalog where covered.

## 7. Packaging and release

- maturin, abi3 (one wheel per platform): manylinux + musllinux
  (x86_64, aarch64), macOS (arm64, x86_64), Windows x86_64. **No Rust
  toolchain ever required to install.**
- Version == engine version; built and published by strata-core's release
  dispatch at the tag (D7). No independent releases after cutover.
- The wheel ships the resolved `command-index.json` (IDL decision #5).
- No telemetry.

## 8. Testing (CI)

1. **Golden transcript** (post-publish, full matrix): install from the real
   registry, put/get round-trip, one error-path assertion
   (`code == "not_found.engine.branch"`, `ref` startswith
   `https://stratadb.org/e/`).
2. **Guide drift guard**: `stratadb.agents_guide()` == `strata agents guide`.
3. **Coverage guard**: every catalog command is reachable via a curated method
   or listed in a shrink-only `uncovered-commands` allowlist (IDL guard
   pattern).
4. **Behavior suite**: port the shapes of strata-core's e2e suites
   (`scripts/cli-tests/`) — branch isolation, spaces, time travel across
   primitives, honest pagination, batch semantics — asserting codes/classes,
   never messages.

## 9. Explicitly out of V1 scope

Old-SDK surfaces that do **not** return (the V1 engine removed them):
state-cell primitive (CAS/put-if-absent), public transactions/sessions,
follower mode, branch bundles, tags/notes, flush/compact maintenance calls,
disk-backed cache mode, graph ontology/BFS/analytics beyond the 14 core
commands, vector filter operators beyond AND-of-equality, `[CODE] message`
string-parsed errors. Async API: deferred — the binding is synchronous
(embedded-DB precedent); an asyncio wrapper can layer later without wire
changes.

## 10. Open questions for the repo owner

1. Keep the old `db.state` namespace name reserved (error with
   `UnsupportedError` + pointer) or drop silently? Recommendation: reserve
   with a teaching error for one release.
2. `Strata.from_env()` (STRATA_DB) — include in V1 or defer? Recommendation:
   include; it mirrors the CLI contract exactly.
3. Ship `stratadb.mcp_config()` returning the client-config snippet?
   Recommendation: yes, trivial and useful.
