# JSON Document Index-Access Surface Plan

## Purpose

This document defines the proposed 80/20 document database surface for the
rebuilt engine and executor command boundary.

The goal is not MongoDB parity. The goal is to support the common document
database workloads without introducing a general Strata query language. The
surface should be small, explicit, branch-aware, historical-read-aware, and
usable from SDKs, CLIs, MCP servers, IPC clients, and benchmarks through the
same serialized executor command model.

## Position

Strata should expose JSON documents through explicit access paths:

- document key access;
- batch key access;
- key-prefix listing;
- atomic single-path JSON helpers;
- named secondary indexes;
- exact index lookup;
- index range scan;
- compound index prefix/range scan;
- index-backed count;
- projection by JSON path;
- optimistic conditional writes.

Strata should not expose a general JSON predicate DSL, hidden query planner, or
MongoDB-style operator language in this layer.

## Why Not A Query Language

General query languages create long-term cost:

- they need a grammar or deeply nested expression schema;
- they need stable semantics for comparison, missing fields, arrays, nulls,
  collation, type ordering, and nested documents;
- they push the engine toward a planner before the access paths are stable;
- they become difficult to bind consistently across Rust, Python, Node, CLI,
  and MCP clients;
- users rarely adopt a small custom query language over direct SDK calls unless
  it is already a known standard.

The 80/20 path is to make index access explicit. Applications can compose
multiple calls or post-filter a bounded result set when they need a shape that
is not worth making into a first-class index.

## RedisJSON Comparison

RedisJSON is the closer comparison than MongoDB. Redis keeps JSON as a data
type with path-level commands such as set, get, delete, multi-get, multi-set,
merge, numeric mutation, array mutation, object introspection, type checks, and
debugging. Query and search live in a separate Redis Search command family.

That split is the useful lesson:

- copy the idea that JSON is a primitive with ergonomic path operations;
- copy the idea that indexes are declared explicitly over JSON paths;
- do not copy the search query string as the core document API;
- do not make the JSON primitive depend on a general query language;
- keep search, if added later, as a separate primitive with its own contract.

RedisJSON also shows one sharp edge to avoid. JSONPath can match multiple
locations inside one document, which makes mutation semantics powerful but easy
to misunderstand. Strata should start with deterministic single-path mutation.
Multi-match path operations can be added later only if the semantics are worth
the complexity.

## Design Principles

1. **Commands name access paths, not predicates.** A command should say "scan
   this named index over this range", not "evaluate this filter tree".

2. **Index definitions own query shape.** Compound ordering, value extraction,
   uniqueness, and missing/null behavior are declared when an index is created.

3. **Sort order is index order.** The document API should not accept arbitrary
   sort clauses. Callers choose an index whose order matches the access pattern.

4. **Projection is selection only.** Projection may include root or selected
   JSON paths. It should not compute expressions, rename fields, join data, or
   transform values.

5. **Cursors are opaque.** Clients can pass cursors back, but they must not
   construct or modify cursor internals.

6. **Branch, space, and timestamp semantics match KV.** Every JSON command that
   reads or writes user data must resolve branch and space the same way as KV.
   Historical reads must use the same timestamp/version model as the rest of
   the engine.

7. **Index maintenance is commit-local.** Document writes and index entry
   updates must commit atomically. Readers must never observe a document/index
   mismatch for a committed version.

8. **No benchmark-only lower-layer bypass.** Benchmarks should use the same
   public engine or executor APIs as normal clients.

9. **JSON helper commands mutate one target path.** Numeric, array, object, and
   merge helpers should resolve exactly one path or fail. They must not apply a
   wildcard update across many document locations.

## Base Document Commands

These commands are the base document surface and should remain independent of
secondary indexes.

| Command | Purpose |
| --- | --- |
| `JsonSet` | Create or replace a full document, or set a value at a JSON path. |
| `JsonGet` | Read a full document or path, latest or as-of a timestamp. |
| `JsonDelete` | Delete a full document or delete a value at a JSON path. |
| `JsonExists` | Check whether a document exists. |
| `JsonGetv` | Return document history for a key. |
| `JsonBatchSet` | Set multiple documents or paths in one engine commit. |
| `JsonBatchGet` | Read multiple documents or paths. |
| `JsonBatchDelete` | Delete multiple documents or paths in one engine commit. |
| `JsonList` | List document keys by key prefix with cursor pagination. |
| `JsonCount` | Count live document keys by key prefix. |
| `JsonSample` | Sample documents by key prefix for inspection and smoke tooling. |

`JsonList`, `JsonCount`, and `JsonSample` are keyspace operations. They are not
filter operations.

## Atomic JSON Helper Commands

These commands are part of the document primitive, not the index/query layer.
They cover the common cases where applications otherwise need to read a whole
document, edit one small piece, and write the document back.

| Command | Purpose |
| --- | --- |
| `JsonMerge` | Merge an object into the object at a path using a documented merge policy. |
| `JsonNumberIncrement` | Add a numeric delta to the number at a path and return the new value. |
| `JsonNumberMultiply` | Multiply the number at a path and return the new value. |
| `JsonArrayAppend` | Append one or more values to an array at a path. |
| `JsonArrayPop` | Remove and return one array element by index. |
| `JsonArrayLen` | Return the length of an array at a path. |
| `JsonObjectKeys` | Return object keys at a path. |
| `JsonObjectLen` | Return the number of object fields at a path. |
| `JsonType` | Return the JSON type at a path. |

The first implementation should prioritize:

1. `JsonNumberIncrement`;
2. `JsonMerge`;
3. `JsonArrayAppend`;
4. `JsonArrayPop`;
5. `JsonArrayLen`;
6. `JsonObjectKeys`;
7. `JsonObjectLen`;
8. `JsonType`.

`JsonNumberMultiply` can be deferred unless a product use case needs it.

### Helper Semantics

Helper commands must follow these rules:

- branch and space defaults match every other JSON command;
- the path must resolve to exactly one value, except when the command creates a
  missing terminal field by contract;
- type mismatch returns a typed error and does not mutate the document;
- successful mutation increments document version;
- commit version and timestamp come from the underlying engine commit;
- affected secondary index entries are updated atomically with the document;
- historical reads can observe helper-produced versions;
- helper commands accept conditional-write facts when conditional writes land.

### Merge Policy

`JsonMerge` needs one documented merge policy before implementation. The
recommended policy is JSON Merge Patch-style object merge at one path:

- object members in the patch are added or replaced;
- nested objects merge recursively;
- patch values that are not objects replace the target value;
- null-delete behavior must be decided explicitly before implementation.

The null decision matters because JSON null is a valid stored value. If
null-as-delete is selected, callers must use `JsonSet` to store a literal null
at a path. If null-as-value is selected, deletion remains only `JsonDelete`.

### Deferred Helper Commands

The following RedisJSON-style commands are useful but not required for the
first 80/20 surface:

- string append;
- string length;
- array insert;
- array trim;
- array index lookup;
- boolean toggle;
- clear object/array;
- memory/debug introspection.

They should be added only after the core helpers prove their shape across
engine, executor, SDK, and benchmark use.

## Index Definition Commands

Secondary indexes are named, explicit, and scoped to a product space.

| Command | Purpose |
| --- | --- |
| `JsonCreateIndex` | Create a named index over one or more JSON paths. |
| `JsonDropIndex` | Drop a named index and its entries. |
| `JsonListIndexes` | List index definitions for a space. |
| `JsonDescribeIndex` | Return one index definition and maintenance facts. |

`JsonDescribeIndex` is optional for the first implementation, but the model
should leave room for it because explicit index access benefits from visible
index metadata.

### Index Definition Shape

An index definition should include:

- `name`;
- `space`;
- ordered `fields`;
- optional `unique` flag;
- optional `multi_value` flag for array item indexing;
- missing-field policy;
- null-value policy;
- value type policy;
- creation version/timestamp;
- index status.

Suggested field shape:

```text
JsonIndexField {
  path: JsonPath,
  direction: Asc | Desc,
  value_type: Any | String | Number | Bool | Timestamp,
}
```

Suggested missing/null defaults:

- missing path: no index entry;
- JSON null: indexed as a distinct null value;
- incompatible value type: write rejected for strict typed indexes, accepted for
  `Any` indexes with deterministic type ordering.

These defaults avoid MongoDB's ambiguous missing-vs-null behavior while keeping
index lookup predictable.

### Index Status

Index status should be explicit:

- `building`;
- `ready`;
- `deleting`;
- `failed`.

The first implementation may build synchronously for small datasets, but the
public contract should not assume index creation is always immediate.

## Index Access Commands

These commands replace a general query language.

| Command | Purpose |
| --- | --- |
| `JsonIndexLookup` | Read documents whose index tuple exactly matches a value tuple. |
| `JsonIndexRange` | Scan one named index over lower/upper tuple bounds. |
| `JsonIndexPrefix` | Scan a compound index by exact prefix plus optional final range. |
| `JsonIndexCount` | Count entries over exact, prefix, or range bounds. |
| `JsonIndexSample` | Sample entries from an index range for diagnostics. |
| `JsonIndexExplain` | Return access-path facts for one index command. |

`JsonIndexSample` and `JsonIndexExplain` can be deferred. They are listed here
because they keep observability explicit without creating a planner.

### Lookup

`JsonIndexLookup` inputs:

- branch?;
- space?;
- index name;
- value tuple;
- projection?;
- limit?;
- cursor?;
- as-of timestamp?;

Output:

- rows containing document key, projected value or full document, commit
  version, commit timestamp, document version, and cursor facts.

### Range

`JsonIndexRange` inputs:

- branch?;
- space?;
- index name;
- lower bound?;
- upper bound?;
- bound inclusivity;
- direction?;
- projection?;
- limit?;
- cursor?;
- as-of timestamp?;

Range scans use index order only. Reverse scans are allowed when supported by
the index cursor.

### Compound Prefix

`JsonIndexPrefix` inputs:

- branch?;
- space?;
- index name;
- exact leading values;
- optional final lower/upper bound;
- direction?;
- projection?;
- limit?;
- cursor?;
- as-of timestamp?;

This covers common patterns such as:

- tenant plus created timestamp;
- status plus updated timestamp;
- account plus type plus sequence;
- owner plus slug.

It avoids a boolean predicate language while still supporting the common
compound-index access patterns.

### Count

`JsonIndexCount` accepts the same bound shapes as lookup, range, and prefix.
It returns a count and, if the implementation cannot answer from index metadata,
facts showing whether it counted by scanning index entries.

## Projection

Projection should be a small type, not a query expression.

```text
JsonProjection =
  Root
  Paths(Vec<JsonPath>)
```

Rules:

- `Root` returns the full document.
- `Paths` returns a JSON object containing only requested paths.
- Missing projected paths are omitted.
- Projection does not compute expressions.
- Projection does not rename fields.
- Projection does not change index selection.

Projection may be applied to key reads, batch gets, and index access commands.

## Conditional Writes

The document API should support optimistic concurrency without introducing an
interactive transaction session.

Suggested condition shape:

```text
JsonWriteCondition {
  expected_version: Option<u64>,
  expected_absent: bool,
}
```

Use cases:

- create only if absent;
- update only if the caller read the current version;
- delete only if the caller read the current version;
- batch writes with per-item expected versions.

This is enough for many web and workflow use cases and matches the engine's
versioned storage model.

## Mixed Bulk Writes

After base batch commands are stable, add a mixed bulk command:

```text
JsonBulkWrite {
  branch?,
  space?,
  operations: Vec<JsonBulkOperation>,
}
```

Operations:

- set document;
- set path;
- delete document;
- delete path;
- conditional variants of the above.

The command should apply valid operations in one engine commit or fail the
whole bulk write before mutation when validation fails. If item-level partial
success is required later, it should be a separate explicitly named mode.

## Array Indexing

Array indexing should be deliberate, not automatic.

First implementation:

- index scalar values and null;
- reject arrays for non-`multi_value` indexes unless `Any` indexing explicitly
  supports whole-array deterministic ordering.

Later implementation:

- `multi_value: true` indexes one entry per scalar array item;
- unique multi-value indexes reject duplicates across documents;
- nested array expansion is not supported unless explicitly added.

This covers tags and membership-style lookups without inheriting MongoDB's full
array query/update semantics.

## Semantics To Guard

The implementation should explicitly guard these rules:

- no general `JsonFind` filter tree in executor command types;
- no hidden planner that selects between multiple indexes;
- no arbitrary sort clause outside index direction;
- no expression projection;
- no wildcard or multi-match path mutation in first-pass helper commands;
- no executor-side JSON mutation;
- no storage request construction from executor JSON handlers;
- index rows never appear in normal document list/count/sample;
- index writes are included in the same engine commit as document writes;
- branch isolation applies to document rows and index rows;
- historical index reads match historical document reads.

## Implementation Order

1. Land base JSON CRUD, history, list, count, sample, and batch commands.
2. Add atomic single-path JSON helper commands.
3. Add index definition types and metadata persistence.
4. Add index-entry maintenance for root set, path set, path delete, helper
   mutations, and document delete.
5. Add `JsonIndexLookup`.
6. Add `JsonIndexRange`.
7. Add `JsonIndexPrefix`.
8. Add `JsonIndexCount`.
9. Add projection to key reads, batch reads, and index access commands.
10. Add conditional writes.
11. Add mixed bulk writes.
12. Add explain and diagnostics.
13. Add multi-value array indexes if product needs justify the extra semantics.

## Test Strategy

### Command Contract Tests

- Every command serializes and deserializes with stable snake-case names.
- Unknown fields are rejected.
- Omitted branch and space use the same defaults as KV.
- Cursor round-trips as an opaque value.
- Projection round-trips as root or paths.
- There is no command variant that accepts a general filter tree.
- Helper command paths cannot encode wildcard or multi-match mutation.

### Engine Behavior Tests

- Root set, path set, path delete, and document delete maintain index entries.
- Missing path produces no index entry.
- JSON null produces a distinct null index entry.
- Type mismatch follows the index value type policy.
- Unique indexes reject duplicate committed values.
- Updating an indexed field removes the old index entry and adds the new one in
  the same commit.
- Helper mutations update affected index entries in the same commit.
- Numeric increment rejects non-numeric values without mutation.
- Array append rejects non-array values without mutation.
- Array pop returns the removed value and updates document history.
- Object key/length commands reject non-object values without mutation.
- Type reports missing, null, bool, number, string, array, and object
  distinctly.
- Merge follows the documented null policy.
- Deleting an indexed field removes the entry.
- Root replacement recomputes all affected entries.
- Historical reads through indexes match historical document reads.
- Branch forks see source index state at the fork point.
- Child branch updates do not change source branch index reads.

### Access Path Tests

- Exact lookup returns all matching documents in deterministic order.
- Range scan honors lower/upper bounds and inclusivity.
- Reverse range scan returns the same rows in reverse order.
- Compound prefix scan returns only matching leading tuples.
- Compound prefix plus final range returns bounded rows.
- Count matches lookup/range/prefix result size.
- Pagination returns stable non-overlapping pages.
- Cursor reuse after incompatible command shape is rejected.

### Source Guards

- Executor JSON code delegates to engine JSON APIs.
- Engine JSON service does not import executor command or output types.
- Index access commands do not accept a generic predicate enum or expression
  tree.
- Benchmarks use engine or executor public APIs.

## Non-Goals

- MongoDB command parity.
- General predicate language.
- Aggregation pipeline.
- Joins.
- Full-text search.
- Geospatial search.
- Hidden multi-index query planning.
- Interactive multi-statement transactions.
- Change streams.
- Server admin, replication, auth, or sharding commands.

## Close Criteria

The document surface is sufficient when a client can:

1. store and retrieve JSON documents by key;
2. update document paths without rewriting client-side documents;
3. batch write and batch read documents;
4. create named indexes for common access patterns;
5. lookup by exact indexed value;
6. page through an indexed range;
7. page through a compound index prefix;
8. count through the same explicit access paths;
9. project selected fields;
10. perform optimistic conditional writes;
11. use the same commands over cache and durable-local modes;
12. use the same commands through executor serialization.
