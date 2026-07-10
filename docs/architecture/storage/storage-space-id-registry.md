# Storage-Next Storage Space ID Registry

Status: V1 architecture draft

## Purpose

Storage-next replaces primitive-aware `TypeTag` ownership with an opaque
`storage_space_id` byte in physical row keys. Storage may route, order, and
compact by this byte, but it must not know whether an engine-owned value means
KV, JSON, event, graph, vector, search, recipe state, or a future capability.

The byte is durable. It needs a registry before implementation so storage-owned
system rows do not collide with engine-owned product rows.

## Binding Allocation

The V1 durable keyspace reserves the byte as follows:

| Range | Owner | Meaning |
| --- | --- | --- |
| `0x00` | Storage-next | Invalid sentinel. Must not appear in durable user rows. |
| `0x01` | Storage-next | Commit timeline rows. |
| `0x02..=0x1f` | Storage-next | Reserved for future storage-internal row families. |
| `0x20..=0xff` | Engine-next | Engine-owned product/data-capability row families. |

Storage-next must reject engine-supplied rows that use storage-reserved IDs.

Engine-next must publish its own product-space assignment registry before V1
format freeze. That registry is the durable compatibility contract for product
capabilities above storage.

## Storage-Owned IDs

### `0x00`: Invalid

`0x00` is reserved as an invalid/sentinel value. It exists to make accidental
zero-initialized storage-space IDs fail loudly.

### `0x01`: Commit Timeline

`0x01` stores the storage-native per-branch commit timeline described in
`docs/architecture/storage/commit-timeline-substrate.md`.

Timeline rows are storage-owned. Engine may read timeline facts through L9
APIs, but engine must not write directly into this storage space.

### `0x02..=0x1f`: Reserved

These values are reserved for storage-internal substrate that may become
necessary after L3-L8 implementation hardens. Examples might include future
storage metadata rows, recovery indexes, or retained-history accelerators.

Adding a storage-owned ID requires updating this registry, the durable format
spec, and the relevant layer document.

## Engine-Owned IDs

`0x20..=0xff` belongs to engine-next.

Engine-next owns:

1. Stable byte assignments for KV, JSON, events, graph relationships, vectors,
   search substrate rows, recipes, intelligence substrate rows, and future
   product capabilities.
2. Any reverse maps needed to recover product references from storage rows.
3. Product compatibility rules when assignments change before V1 freeze.

Storage-next treats these bytes as opaque ordering and routing facts.

## Engine Registry

Engine-owned assignments are documented in
`docs/architecture/engine/storage-space-id-registry.md`.

Storage-next must not duplicate that registry or map engine-owned bytes to
product names. Storage validates byte ownership at the range level; engine-next
validates whether an engine-owned byte is assigned, known, or compatible with
the database's persisted engine registry.

## Testing Requirements

1. Storage rejects storage-reserved IDs in engine-supplied commit rows.
2. Storage system rows use only storage-owned IDs.
3. Engine-next registry tests prove no duplicate product-space assignments.
4. Durable-format tests include at least one storage-owned row family and one
   engine-owned row family.
5. Fuzz tests reject `0x00` and other invalid key encodings.

## Open Questions

1. Does storage-next need more than one timeline-related ID, or can timeline
   key prefixes inside `0x01` cover both timestamp-to-version and
   version-to-timestamp indexes?
