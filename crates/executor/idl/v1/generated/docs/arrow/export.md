---
title: "Export Arrow file"
description: "Export a product primitive to an Arrow-compatible file."
source: strata-core@1.0.0
section: arrow
---

Exports a product primitive from the selected branch and space to an Arrow-compatible file (Parquet, CSV, or JSONL). Graph exports treat the path as a stem and write separate node and edge files. Returns a summary of the exported primitive, the concrete output paths, the row count, and the total output size.

Status commands return a scalar or compact status payload and do not mutate database state.

## Examples

Export a primitive to a Parquet file, then import it back.

### CLI

```console
$ strata kv put greeting hello
$ strata arrow export kv parquet /tmp/exports/kv.parquet  # One file per primitive; Parquet by default.
$ strata kv delete greeting
$ strata arrow import /tmp/exports/kv.parquet kv
$ strata kv get greeting
```

### Wire

```json
{"key":"Z3JlZXRpbmc=","type":"kv_put","value":"aGVsbG8="}
{"format":"parquet","path":"/tmp/exports/kv.parquet","primitive":"kv","type":"arrow_export"}
{"key":"Z3JlZXRpbmc=","type":"kv_delete"}
{"file_path":"/tmp/exports/kv.parquet","target":"kv","type":"arrow_import"}
{"key":"Z3JlZXRpbmc=","type":"kv_get"}
```

### Output

One response per step, in order:

```json
{"data":{"commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":3,"version":3},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"key":"Z3JlZXRpbmc="},"type":"write_result"}
{"data":{"format":"parquet","paths":["/tmp/exports/kv.parquet"],"primitive":"kv","row_count":1,"size_bytes":1915},"type":"arrow_export_result"}
{"data":{"commit":{"delete_count":1,"durable":false,"put_count":0,"timestamp":4,"version":4},"effect":{"affected_count":1,"applied":true,"kind":"deleted","matched":true},"key":"Z3JlZXRpbmc="},"type":"delete_result"}
{"data":{"batches_processed":1,"file_path":"/tmp/exports/kv.parquet","rows_imported":1,"rows_skipped":0,"target":"kv"},"type":"arrow_import_result"}
{"data":{"found":true,"value":{"timestamp":5,"value":"aGVsbG8=","version":5}},"type":"kv_versioned_value"}
```

## Parameters

| Name | Type | Required | Default | Description |
|---|---|---|---|---|
| `format` | `ArrowFileFormat` | yes | — | Output file format. |
| `path` | `string` | yes | — | Output file path. Graph exports treat this as a stem and return concrete node and edge paths. |
| `primitive` | `ArrowExportPrimitive` | yes | — | Product primitive to export. |
| `collection` | `string` | no | — | Target vector collection for vector exports. |
| `event_type` | `string` | no | — | Optional event type filter for event exports. |
| `graph` | `string` | no | — | Target graph for graph exports. |
| `limit` | `integer` | no | — | Optional row limit. |
| `prefix` | `string` | no | — | Optional key, document, vector-key, or node-id prefix. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`StatusResponse<ArrowExport>`.

| Field | Type | Description |
|---|---|---|
| `format` | `ArrowFileFormat` |  |
| `paths` | `string[]` |  |
| `primitive` | `ArrowExportPrimitive` |  |
| `row_count` | `integer` |  |
| `size_bytes` | `integer` |  |

## Errors

| Code | Meaning |
|---|---|
| [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed) | The runtime is closed. |
| [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch) | The requested branch was not found. |
| [`invalid_argument.engine.product_space`](https://stratadb.org/e/invalid_argument.engine.product_space) | The requested space operation cannot be completed. |
| [`invalid_argument.executor.arrow_feature_disabled`](https://stratadb.org/e/invalid_argument.executor.arrow_feature_disabled) | Arrow support is not enabled in this build. |
| [`invalid_argument.executor.arrow_format`](https://stratadb.org/e/invalid_argument.executor.arrow_format) | The Arrow file format is invalid. |
| [`invalid_argument.executor.arrow_empty_export`](https://stratadb.org/e/invalid_argument.executor.arrow_empty_export) | The Arrow export source is empty. |
| [`invalid_argument.executor.arrow_value_column`](https://stratadb.org/e/invalid_argument.executor.arrow_value_column) | The Arrow value column is invalid. |
| [`invalid_argument.executor.arrow_vector_key`](https://stratadb.org/e/invalid_argument.executor.arrow_vector_key) | The Arrow vector key column is invalid. |
| [`invalid_argument.executor.arrow_graph`](https://stratadb.org/e/invalid_argument.executor.arrow_graph) | The Arrow graph request is invalid. |
| [`invalid_argument.executor.arrow_collection`](https://stratadb.org/e/invalid_argument.executor.arrow_collection) | The Arrow collection target is invalid. |
| [`unavailable.executor.arrow_io`](https://stratadb.org/e/unavailable.executor.arrow_io) | The Arrow input or output path is unavailable. |
| [`internal.executor.arrow`](https://stratadb.org/e/internal.executor.arrow) | An internal Arrow boundary error occurred. |

## Invocation

```text
strata arrow export <primitive> <format> <path> [--collection <string>] [--event-type <string>] [--graph <string>] [--limit <integer>] [--prefix <string>] [--branch <branch>] [--space <space>]
```

- Wire type: `arrow_export`

## Related

- [Import Arrow file](/docs/arrow/import) — Import an Arrow-compatible file into a product primitive.
- [Delete KV value](/docs/kv/delete) — Delete one visible KV key.
- [Get KV value](/docs/kv/get) — Read the current or historical value for one KV key.
- [Put KV value](/docs/kv/put) — Store or replace a KV value by key.
- [All `arrow` commands](/docs/arrow/)
