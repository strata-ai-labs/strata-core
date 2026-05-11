# L2. Object Layout

Status: V1 architecture draft

## Purpose

Object layout maps storage concepts to backend object names.

L1 moves opaque bytes by object name. L2 defines the canonical object namespace
under a database root so upper layers do not construct ad hoc paths, filenames,
or prefixes.

L2 owns names, not bytes and not policy.

## Core Decision

Storage-next should have one canonical object namespace that works for:

1. Browser/cache backend.
2. Local filesystem backend.
3. Future OpenDAL/object-store backends.

The layout is object-shaped. Local filesystem maps object names to paths.
Browser/cache maps object names to keys. Future object stores map object names
to object keys.

The layout should not expose filesystem-only concepts such as file descriptors,
directory handles, parent directory fsync, or rename. Those are backend or
durable-service concerns.

## Responsibilities

Object layout owns:

- database-relative object names
- object-name validation
- object family definitions
- mapping logical storage roles to object names
- stable naming conventions for WAL, tables, manifests, snapshots, temporary
  objects, quarantine, and locks/leases
- validated object-name components that backends can map to paths or keys
- reserved prefixes

Object layout does not own:

- object bytes
- WAL record format
- table format
- manifest format
- snapshot format
- checkpoint policy
- recovery policy
- retention policy
- branch product semantics
- backend IO
- backend-specific path or key translation
- engine data capability semantics

## Naming Model

An object name is a normalized database-relative string.

Rules:

1. UTF-8 ASCII subset only for V1.
2. `/` separates logical path components.
3. No leading slash.
4. No trailing slash.
5. No empty component.
6. No `.` or `..` component.
7. No platform path separators other than `/`.
8. No absolute paths.
9. No backend URL syntax.
10. Only layout-owned constructors create object names used by storage.

L2 should provide typed constructors rather than letting upper layers format
strings directly.

## Object Families

The V1 layout should reserve these families:

```text
manifest/
wal/
tables/
snapshots/
tmp/
quarantine/
locks/
meta/
```

The exact names can change before implementation. The important property is
that every object belongs to a known family and each family has one owner.

## Proposed Canonical Layout

```text
manifest/current

wal/<segment-id>

tables/<branch-id>/<level>/<table-id>
tables/<branch-id>/manifest

snapshots/<snapshot-id>

tmp/<operation-id>/<object-id>

quarantine/<branch-id>/<object-id>
quarantine/<branch-id>/manifest

locks/writer

meta/database
```

This is a target naming model, not a format commitment.

`manifest/current` is the database-level durable manifest location. Its bytes
belong to L3/L4.

`wal/<segment-id>` names WAL segment objects. It does not imply appendable
files. L4 decides how WAL segments are written and published.

`tables/<branch-id>/<level>/<table-id>` names immutable table objects. L5 owns
the table bytes. L6 owns branch mechanics. L2 only defines the object name.

`tables/<branch-id>/manifest` names branch/table reachability metadata if the
design keeps a per-branch table manifest. Whether this remains separate from
the database manifest is an L4/L6 decision.

`snapshots/<snapshot-id>` names snapshot/checkpoint objects. Snapshot bytes and
retention behavior are owned by L3/L4/L8.

`tmp/<operation-id>/<object-id>` names temporary objects. L2 defines the
namespace; L4/L8 define publish and cleanup rules.

`quarantine/<branch-id>/<object-id>` and `quarantine/<branch-id>/manifest`
reserve recovery/repair inventory locations. L8 owns the protocol.

`locks/writer` reserves the logical writer-lock/lease object if a backend uses
object names for locking. The lock protocol itself belongs to L1/L8.

`meta/database` is reserved for non-manifest database identity or static
metadata if needed. It should not become a dumping ground.

## ID Encoding

Object names should use stable, sortable, fixed-width encodings where ordering
matters.

Recommended encodings:

- manifest generation: zero-padded decimal or fixed-width hex
- WAL segment ID: zero-padded decimal or fixed-width hex
- snapshot ID: zero-padded decimal or fixed-width hex
- branch ID: canonical lowercase hex or another core/storage-owned stable ID
  encoding
- table ID: fixed-width generated ID
- level: `l0`, `l1`, `l2`, etc.

The exact encoding should be chosen once core-next/storage-next decide which ID
types live where. L2 should own the string form used in object names even if the
ID type itself lives in core-next.

## Backend Mappings

### Browser / Cache Backend

The browser/cache backend maps validated object names directly to in-memory
keys.

It should not require directory creation. Prefix listing is implemented by key
prefix scan over the in-memory map.

### Local Filesystem Backend

The local filesystem backend maps validated object names to paths under the
database root.

Rules:

1. Object name validation happens before path construction.
2. Object names never contain `..` or absolute path syntax.
3. The backend joins the database root with validated object components.
4. Parent directory creation is a backend/publish concern, not an object-layout
   semantic.
5. Atomic publish and directory fsync remain L1/L4 concerns.

Local filesystem should be efficient, but upper layers should not receive raw
paths unless they are inside the local filesystem backend.

### Future OpenDAL / Object Backend

A future OpenDAL/object backend maps object names to object keys.

L2 should avoid assumptions that object stores have real directories, cheap
rename, append, file handles, or parent fsync.

## Current Code Evidence

Current filesystem-shaped layout is spread across:

- `durability/layout.rs`
- `durability/wal/mod.rs`
- `durability/format/snapshot.rs`
- `manifest.rs`
- `quarantine.rs`

Current names include:

- `wal/`
- `segments/`
- `snapshots/`
- `MANIFEST`
- `wal-NNNNNN.seg`
- `snap-NNNNNN.chk`
- `segments.manifest`
- `quarantine.manifest`
- `__quarantine__/`
- `follower_state.json`
- `follower_audit.log`

Storage-next should not preserve `follower_state.json` or
`follower_audit.log`. Follower mode is not a V1 product path.

The current names are useful evidence, not binding target names.

## Failure Model

L2 failures are object-name failures:

- invalid object name
- invalid ID encoding
- unsupported object family
- reserved prefix misuse
- path traversal attempt
- object name too long for storage's portable object-name limit

L2 should not classify IO failures. Once an object name is valid, backend
mapping and IO failures belong to L1.

## Testing Requirements

Minimum tests:

1. Every constructor produces a valid object name.
2. Invalid names are rejected.
3. No constructor can produce `..`, absolute paths, empty components, or trailing
   slashes.
4. Lexical ordering works for ordered IDs.
5. Prefix listing prefixes are unambiguous.
6. Reserved prefixes cannot be used by the wrong object family.
7. Follower-state names are absent from the target layout.

Property tests:

1. Random valid IDs roundtrip through object-name construction and parsing.
2. Random invalid strings are rejected or normalized only through explicit
   constructors.
3. Backend path/key mapping tests prove validated object names cannot escape
   their backend namespace.

## V1 Minimum

The first storage-next implementation needs:

1. A storage-owned `ObjectName` or equivalent validated type.
2. Constructors for database manifest, WAL segments, table objects, snapshots,
   temporary objects, quarantine objects, and writer lock/lease object.
3. Prefix constructors for listing WAL, tables, snapshots, tmp, and quarantine.
4. Tests that no storage layer above L2 constructs object names with raw string
   formatting.
5. Backend-owned tests for local filesystem path mapping and browser/cache key
   mapping.

The first implementation does not need:

1. Manifest history.
2. Production object-store naming proof.
3. Multi-writer lease naming beyond reserving the writer-lock object family.
4. Provider-specific layout tuning.
5. Backward-compatible preservation of current filenames.

## Deferred

Deferred decisions:

1. Whether database manifest history is required.
2. Whether branch/table manifests remain separate from the database manifest.
3. Whether object names include checksums or only IDs.
4. Whether table IDs are content-addressed.
5. Whether snapshot IDs are version IDs, checkpoint IDs, or independent
   sequence IDs.
6. Whether object-store mode needs layout partitioning to avoid hot prefixes.

## Open Questions

1. Which ID types and encodings belong in core-next versus storage-next?
2. Should local filesystem preserve human-readable names like `wal-000001.seg`
   for debugging, or use the same object names exactly?
3. Should `tables/<branch-id>/<level>/<table-id>` include a table generation or
   creation version?
4. Should temporary objects be operation-scoped, transaction-scoped, or
   backend-generated?

Resolved first-pass decision: object names do not carry a storage format
version prefix such as `v1/` by default. The manifest and object bytes carry
format identity. Add a namespace prefix only if a future stable format must
coexist with V1 data.

Resolved first-pass quarantine decision: quarantine objects live under the
global `quarantine/` family. If later recovery tooling needs source-family
information, that fact should be stored in quarantine metadata rather than by
placing quarantine objects back under their source families.

## Next Layer Dependency

L3 durable format/codec consumes object names only indirectly through durable
services. It should define bytes without assuming local paths. L4 durable
services consume L2 directly when choosing where WAL, manifest, snapshot, and
table-related service objects live.
