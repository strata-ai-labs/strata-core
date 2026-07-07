# @stratadb/core (Node) — V1 SDK specification

Prereq reading: `00-shared-contracts.md`. This is a **ground-up specification**:
the existing package binds the old engine and none of its binding code survives
the V1 cutover. The repo, npm name (`@stratadb/core`), and napi prebuild matrix
carry over; the architecture, API surface, and error model below are new.

This document mirrors the Python spec (`strata-python.md`) — the two SDKs are
deliberately isomorphic (P6: same verbs, same shapes, same error codes). Where
this document is silent, the Python spec's rule applies with camelCase names.

---

## 1. Positioning and principles

Strata is an **embedded** multi-model database — the Node peer precedent is
better-sqlite3, not a network client. Non-negotiables:

- **P6 — same verbs everywhere**: the executor's command surface, JSON value
  shapes, and stable error codes, identical to CLI/MCP/Python.
- **One canonical path**: curated methods wrap the same serialized command
  boundary as `strata command run`; no second implementation of semantics.
- **The package is the documentation**: guide, `.d.ts` doc comments, README —
  all generated from executor metadata; drift fails CI.
- **Synchronous API** (decision): embedded engines with in-process calls are
  faster and *more* ergonomic sync (the better-sqlite3 lesson). The executor
  is `&mut self`, so concurrent Promises on one handle would serialize anyway
  — a sync API makes the truth visible instead of accidental. An async
  wrapper (worker_threads) can layer post-V1 without wire changes.

## 2. Repo strategy

`v1` branch in the existing repo as a clean break (strata-core's own pattern).
No compat shims with the old binding; no migration tooling for pre-V1
databases (the engine rejects them with structured format errors). The old
package ships from `main` until M9 cutover; at V1 promotion the version jumps
to the unified engine version (`1.0.0`).

## 3. Architecture: three layers

```
┌─ curated namespaces (handwritten TS)     db.kv / db.json / db.vectors / …
├─ generated core (from the resolved IDL)  types + one method per command
└─ native binding (napi-rs)                Handle: open/execute/close
```

### 3.1 Native binding (napi-rs)

Deliberately tiny:

```
Handle.openDurable(path: string): Handle    // exclusive process lock
Handle.openCache(): Handle
Handle.execute(commandJson: string): string // serialized Command → Output envelope
Handle.close(): void
Handle.setScope(branch: string, space: string): void
version(): string
agentsGuide(): string                       // embedded at build time
```

- `execute` is the executor's serialized wire (`deny_unknown_fields`) — the
  engine linked in-process via `strata_executor_next::Executor`; no external
  binary.
- Errors surface as the shared error envelope; the binding throws one native
  error carrying the payload, which layer 3 maps to typed classes.
- Calls are synchronous on the JS thread and serialized per handle (`&mut
  self` made explicit). One durable database = one exclusive process lock.

### 3.2 Generated core (IDL Phase 2)

From the resolved `command-index.json` (ships in the binary, at
`stratadb.org/idl/v1/`, and inside this package): one typed function per
cataloged command, discriminated-union types for `Command`/`Output` on `type`,
and shared models (`CommitReceipt`, `MutationEffect`, `PageInfo`,
`BatchResult`, `ErrorStatus`) from the schemars schemas. Until the full
catalog lands (32/~90 commands today), curated methods construct wire JSON
directly and the generator later replaces internals without public-API change.

Escape hatch (public, permanent):

```ts
db.execute({ type: "kv_scan", limit: 10 }): { type: string; data: unknown }
```

### 3.3 Curated namespaces (public API — handwritten TS)

## 4. The public API

### 4.1 Entry point and scoping

```ts
import { Strata } from "@stratadb/core";

const db = new Strata("./app-data");            // durable; creates if absent
const mem = new Strata({ cache: true });        // ephemeral
// No implicit cwd, ever (D2). new Strata() with no target throws
// InvalidArgumentError; Strata.fromEnv() reads STRATA_DB explicitly.

db.close();                                     // also Symbol.dispose (using)

const scoped = db.at({ branch: "experiment" }); // cheap scoped view; also space
scoped.kv.get("k");
```

Namespaces on `Strata`: `kv`, `json`, `vectors`, `events`, `graphs`,
`branches`, `spaces`, `admin`, `arrow`; plus `execute()`, `close()`,
`version`, `at()`.

### 4.2 Type conventions

| Wire concept | TypeScript |
|---|---|
| Bytes | accept `string \| Buffer \| Uint8Array` (string = UTF-8); return `Buffer` |
| JSON value | `unknown` in / typed generics where useful |
| Commit receipt | `Receipt { version: number; timestamp: number; durable: boolean; putCount: number; deleteCount: number }` |
| Mutation effect | `Effect { applied: boolean; kind: string; matched: boolean; affectedCount: number }` |
| Maybe-read | `null` for miss (absence is not an exception) |
| Page | `Page<T> { items: T[]; hasMore: boolean; cursor: string \| null }` — cursors opaque, passed back verbatim |
| Timestamps | `number` (commit-timestamp domain); every read takes `asOf?: number` |
| Batch | `BatchResult { status: string; items: BatchItem[] }` |

Writes return `Receipt` + `Effect`. Every list surface has the page call and
an auto-paginating iterator (`iter*` returning `IterableIterator<T>`).

### 4.3 Namespace surface (complete — signatures abbreviated)

**`db.kv`** (13 commands): `put(key, value)`, `get(key, {asOf?})`,
`getEntry(key, {asOf?})`, `delete(key)`, `exists(key)`, `history(key)`,
`keys({prefix?, limit?, cursor?, asOf?}): Page<Buffer>` + `iterKeys()`,
`scan({start?, limit?, cursor?}): Page<ScanRow>` + `iterRows()`,
`count({prefix?})`, `sample({prefix?, count?})`, `putMany(entries)`,
`getMany(keys)`, `deleteMany(keys)`, `existsMany(keys)`.

**`db.json`** (14): `set(key, path, value)`, `get(key, path, {asOf?})`,
`getEntry`, `delete(key, path)`, `exists(key)`, `history(key)`,
`keys({prefix?, …}): Page<string>`, `count`, `sample`, `setMany/getMany/
deleteMany`, `createIndex(name, fieldPath, {indexType?})`, `dropIndex(name)`,
`listIndexes()`.

**`db.vectors`** (19): `createCollection(name, dimension, {metric?})`,
`deleteCollection`, `listCollections`, `stats(name)`, `count(name)`,
`upsert(collection, key, vector: number[] | Float32Array, {metadata?})`,
`get(collection, key, {asOf?})`, `history`, `exists`, `keys(collection,
{limit?, cursor?})`, `updateMetadata(collection, key, metadata)`, `delete`,
`deleteAll(collection)`, `deleteByFilter(collection, filter)`,
`query(collection, vector, {k?, filter?, asOf?}): Match[]`,
`indexQuery(...): { matches: Match[]; diagnostics: IndexDiagnostics }`,
`upsertMany/getMany/deleteMany`.

Filter helper (V1 filters are AND-of-equality only):

```ts
import { filters } from "@stratadb/core";
filters.and(filters.eq("kind", "note"), filters.eq("rank", 5))
```

**`db.events`** (10): `append(eventType, payload): AppendResult`,
`appendMany(entries)`, `get(sequence, {asOf?})`, `exists(sequence)`,
`length({asOf?})`, `range(start, {end?, limit?, reverse?, eventType?})`,
`rangeByTime(startTs, {endTs?, …})` (**occurrence-time domain** — the one
intentionally wall-clock API), `list({eventType?, limit?, afterSequence?,
asOf?})`, `types({asOf?})`, `verifyChain()`.

**`db.graphs`** (14): `create(name)`, `delete(name)`, `list()`, `meta(name)`,
`addNode(graph, nodeId, {properties?})`, `getNode(graph, nodeId, {asOf?})`,
`removeNode`, `listNodes(graph, {limit?, cursor?, asOf?})`, `addEdge(graph,
src, edgeType, dst, {weight?, properties?})`, `getEdge(…, {asOf?})`,
`removeEdge`, `neighbors(graph, nodeId, {direction?, edgeType?, limit?,
asOf?})`, `bindingsForEntity(target)`, `batchWrite(operations)` (atomic).

**`db.branches`** (7): `list()`, `get(name)`, `create(name)` (empty root),
`fork(source, branch, {version?, timestamp?})` (anchors mutually exclusive),
`delete(name)`.

**`db.spaces`** (4): `list()`, `create(name)`, `exists(name)`,
`delete(name)` (refuses when non-empty).

**`db.admin`**: `ping()`, `info()`, `health()`, `metrics()`, `describe()`,
`config()`, `configValue(key)`.

**`db.arrow`**: `import(target, path, options?)`, `export(primitive, path,
{format?, …})`.

**Package level**: `agentsGuide(): string`, `VERSION` (== engine version),
error classes, `filters`.

## 5. Error model

Structured errors, never `[CODE] message` string parsing (the old SDK's
pattern — dead). One base class, one subclass per public error class (15
today; the wire enum is non-exhaustive — unknown classes map to the base):

```ts
class StrataError extends Error {
  code: string;            // "not_found.engine.branch" — match on this
  errorClass: string;      // "not_found"
  hint: string;            // suggested_fix
  ref: string;             // https://stratadb.org/e/<code>
  referenceId: string;
  retryPolicy: "never" | "after_state_change" | "same_request" | "idempotent_only" | "unknown";
  retryable: boolean;
  commitOutcome: string;
  details?: Record<string, string>; hints?: string[];
}

NotFoundError, AlreadyExistsError, InvalidArgumentError, FailedPreconditionError,
AccessDeniedError, ConflictError, AmbiguousCommitError, HistoryUnavailableError,
UnsupportedError, ResourceExhaustedError, UnavailableError, IoError,
CorruptionError, SerializationError, InternalError
```

Misses are `null`, not throws. Out-of-range time travel is
`HistoryUnavailableError`, distinct from `NotFoundError` (the engine's F7/F8
contract). `error.message` renders `code: message` with hint/ref appended —
tests assert `code`/`errorClass` only.

## 6. Agent surfaces

- `agentsGuide()` — byte-identical to `strata agents guide` for the same
  version (embedded at build; CI guard).
- npm README = full quickstart inline (agents read `node_modules` before the
  web); taught commands mirror `strata init --json` → `next_steps`.
- Complete `.d.ts` with a one-line example per public method, generated from
  the command catalog where covered. Wire types are discriminated unions on
  `type` — keep them exactly the executor's envelopes.

## 7. Packaging and release

- napi-rs prebuilds: linux gnu+musl (x86_64, aarch64), macOS (arm64, x86_64),
  Windows x86_64. **No toolchain required to install.**
- Version == engine version; published by strata-core's release dispatch at
  the tag (D7).
- Ships the resolved `command-index.json`. No telemetry.
- `@stratadb/mcp` (the npx shim) is specified separately in `strata-mcp.md` —
  it is packaging, not part of this SDK.

## 8. Testing (CI)

1. **Golden transcript** post-publish on the full prebuild matrix: install
   from the registry, put/get round-trip, one error-path assertion
   (`code === "not_found.engine.branch"`, `ref` starts with
   `https://stratadb.org/e/`).
2. **Guide drift guard**: `agentsGuide()` == `strata agents guide`.
3. **Coverage guard**: every catalog command reachable via a curated method or
   listed in a shrink-only allowlist.
4. **Behavior suite** mirroring strata-core's e2e shapes: branch isolation,
   spaces, time travel, honest pagination, batch semantics — codes/classes,
   never messages.
5. **Type tests**: `.d.ts` compiles under `strict`; discriminated-union
   narrowing on `Output.type` works.

## 9. Explicitly out of V1 scope

State-cell primitive (CAS/put-if-absent), public transactions/sessions,
follower mode, branch bundles, tags/notes, flush/compact, disk-backed cache,
graph ontology/BFS/analytics beyond the 14 core commands, vector filter
operators beyond AND-of-equality, string-parsed errors, async API (deferred;
see §1), progress/cancellation for long operations (post-V1 with model pulls).

## 10. Open questions for the repo owner

1. ESM/CJS: dual-publish or ESM-only with CJS shim? Recommendation: dual via
   napi-rs defaults; agents still hit CJS `require` paths constantly.
2. Reserve the old `db.state` namespace with a teaching `UnsupportedError`
   for one release? Recommendation: yes (mirrors Python spec).
3. `Strata.fromEnv()` (STRATA_DB) in V1? Recommendation: yes — mirrors the
   CLI contract exactly.
4. Ship `mcpConfig()` returning the client-config snippet? Recommendation:
   yes.
