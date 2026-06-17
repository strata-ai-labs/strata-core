# Engine JSON Parity Test Plan

## Purpose

Prove that the rebuilt engine owns JSON document semantics with the same
structure used for KV: public engine types at the API boundary, product behavior
in the JSON service, and storage translation isolated in persistence.

## Test Matrix

| Area | Cache | Durable Local |
| --- | --- | --- |
| Type validation | Required | Required |
| Row-key encoding and decoding | Required | Required |
| Document envelope encoding | Required | Required |
| Create/set/get/delete | Required | Required |
| Path mutation | Required | Required |
| Batch set/get/delete | Required | Required |
| Branch isolation | Required | Required |
| Space isolation | Required | Required |
| List/count/sample | Required | Required |
| Version history | Required | Required |
| Timestamp and version reads | Required | Required |
| Secondary index metadata | Required | Required |
| Reopen persistence | Not applicable | Required |
| Structure and source guards | Required | Required |

## Unit Tests

### Document Id Validation

- Accepts ordinary UTF-8 document ids.
- Rejects empty document ids.
- Rejects document ids that exceed the configured length.
- Rejects reserved internal ids if any are defined.
- Preserves ordering for document ids inside one product space.

### JSON Value Validation

- Accepts null, bool, number, string, array, and object values.
- Rejects values above the max serialized document size.
- Rejects nesting deeper than the configured max depth.
- Rejects arrays above the configured max array length.
- Rejects non-finite numbers if a conversion path can produce them.
- Error class is stable and engine-owned.

### Path Parsing

- Parses root path.
- Parses dot-key object paths.
- Parses quoted or escaped object keys if supported.
- Parses array indices if supported.
- Rejects empty non-root paths.
- Rejects malformed bracket paths.
- Rejects paths above the configured path length.
- Path round-trip formatting is deterministic.

### Row-Key Encoding

- Encodes JSON document rows deterministically.
- Decodes ASCII document ids.
- Decodes binary-safe UTF-8 document ids if supported.
- Rejects unknown key version.
- Rejects unknown discriminator.
- Rejects truncated space length.
- Rejects mismatched space.
- Rejects empty decoded document id.
- Does not decode KV or control-plane rows as JSON rows.

### Document Envelope

- Encodes a fixture document deterministically.
- Decodes the fixture document.
- Rejects unknown format version.
- Rejects truncated envelope payload.
- Rejects document id mismatch.
- Rejects corrupt JSON payload.
- Preserves document version and updated timestamp.

### Outcome Types

- Accessors return key, value, version, timestamp, document version, tombstone
  flag, cursor, counts, and index facts.
- Outcome structs keep fields private.
- Outcome structs can be compared in tests without exposing storage types.

## Engine Behavior Tests

Run each behavior test against both cache and durable-local fixtures unless the
test is specifically about reopen.

### Create

- Create one document at root.
- Read it back at root.
- Read one nested path.
- Assert commit version/timestamp are present.
- Creating an existing document returns stable invalid-input behavior.

### Set Or Create

- Set root on a missing document and assert it creates the document.
- Set a nested object path on an existing document.
- Set a nested path on a missing document and assert the documented materialize
  behavior.
- Assert the returned full document reflects the mutation.

### Set Existing

- Set root on an existing document.
- Set an existing nested path.
- Setting a missing document fails.
- Setting an impossible array path fails.
- Document version increments once per successful mutation.

### Get

- Get root.
- Get nested object value.
- Get array element if arrays are supported in paths.
- Missing document returns `None`.
- Missing path returns `None`.
- Latest versioned read returns value, commit version, timestamp, and document
  version.

### Delete

- Delete a nested object field.
- Delete an array element if arrays are supported in paths.
- Delete root and assert the document is absent.
- Delete missing root returns `deleted=false`.
- Delete missing nested path returns the documented no-op or invalid-input
  behavior.
- Deleting a path increments document version and keeps the document visible.
- Deleting root writes a tombstone and hides the document from latest reads.

### Exists

- Missing document returns false.
- Created document returns true.
- Path deletion that leaves the document present returns true.
- Root deletion returns false.

### Batch Set

- Batch set multiple documents in one engine call.
- Batch set multiple paths in the same document and assert the documented order.
- Results are positional.
- One commit applies the valid batch.
- Empty batch returns an empty outcome.
- Duplicate document/path entries follow documented behavior.

### Batch Get

- Batch get existing and missing documents.
- Batch get existing and missing paths.
- Results are positional.
- Duplicate reads are allowed and positional.
- Returned metadata matches individual versioned reads.

### Batch Delete

- Root-delete multiple documents.
- Path-delete multiple documents.
- Mixed root and path deletes follow the documented behavior.
- Results are positional.
- Missing root delete reports `deleted=false`.
- Missing path delete reports item-level error or false according to the API.
- Empty batch returns an empty outcome.

### Branch Isolation

- Create a branch from the default branch.
- Mutate the same document id on both branches.
- Reads return branch-local values.
- List/count/sample/history stay branch-local.
- Root delete on one branch does not hide the document on another branch.

### Space Isolation

- Write the same document id in two spaces.
- Reads return space-local values.
- List/count/sample/history stay space-local.
- Index metadata in one space does not affect another space.

### List

- Insert ordered document ids with multiple prefixes.
- List all document ids in sorted order.
- List prefix matches in sorted order.
- Cursor starts strictly after the cursor document id.
- `has_more` and cursor facts are correct.
- Tombstoned documents are suppressed.
- Internal index metadata is suppressed.
- `limit == 0` follows the documented empty-page or invalid-input behavior.

### Count

- Count all visible documents.
- Count by prefix.
- Count after root delete.
- Count in an empty space.
- Count excludes internal index metadata.

### Sample

- `sample(count=0)` returns total count and zero items.
- `sample(count >= total)` returns all matching documents.
- `sample(count < total)` returns at most count documents.
- Every sampled document id matches the prefix.
- Sampling is deterministic over unchanged data.
- Sampling excludes internal index metadata.

## Historical Tests

### Version History

- Create a document.
- Mutate it multiple times.
- Delete a path.
- Root-delete it.
- History returns newest-first rows.
- History includes document versions, commit versions, timestamps, and
  tombstone facts.
- Missing document returns `None` or empty history according to the documented
  API.

### Point Read At Version

- Capture versions from create, update, path delete, and root delete.
- Read at each version.
- Path selection happens after historical document selection.
- Reads before creation return `None`.
- Reads after root delete return `None`.

### Point Read At Timestamp

- Capture timestamps from create, update, path delete, and root delete.
- Read at each timestamp.
- Reads before creation return `None`.
- Reads after root delete return `None`.

### List At Timestamp

- Create document A.
- Create document B.
- Delete document A.
- List at each timestamp and assert the historical document set.
- Prefix filtering and cursor behavior match latest list semantics.

## Secondary Index Tests

### Index Metadata

- Create a numeric index.
- Create a tag index.
- Create a text index if supported.
- List indexes returns stable definitions.
- Duplicate index name fails.
- Invalid index name fails.
- Invalid field path fails.
- Drop existing index returns true.
- Drop missing index returns false.
- Durable reopen preserves index definitions.

### Index Entry Maintenance

- Creating a document creates matching index entries.
- Updating an indexed field replaces old index entries.
- Deleting an indexed field removes index entries.
- Root delete removes all index entries for the document.
- Batch set/delete maintains index entries once per committed batch.
- Index entry rows are hidden from normal document list/count/sample.

## Durable Reopen Tests

- Open durable-local database.
- Create documents in two spaces.
- Mutate paths.
- Create indexes.
- Delete a subset of documents.
- Close and reopen.
- Assert latest reads, list, count, sample, history, and index definitions match
  pre-close state.

## Error Tests

- Missing branch returns not-found.
- Closed database rejects every JSON operation.
- Invalid space returns invalid input.
- Invalid document id returns invalid input.
- Invalid path returns invalid input.
- Invalid JSON value returns invalid input.
- Corrupt row key maps to engine corruption/data-loss.
- Corrupt document envelope maps to engine corruption/data-loss.
- Error messages do not leak storage request or table type names.

## Source Guards

- Engine public JSON modules expose engine-owned service and outcome types only.
- JSON service does not import executor command or output types.
- JSON service does not construct storage request types directly.
- Persistence modules own storage request construction and storage outcome
  mapping.
- JSON row-key helpers are the only source of JSON row-key encoding.
- Internal index rows are explicitly filtered from document-facing APIs.
- No test names or comments use planning-slice labels.

## Verification Commands

```text
cargo fmt --all
cargo test -p strata-engine-next --all-features json
cargo test -p strata-engine-next --all-features
cargo clippy -p strata-engine-next --all-features --all-targets -- -D warnings
```
