---
title: "Import Arrow file"
description: "Import an Arrow-compatible file into a product primitive."
source: strata-core@1.0.0
section: arrow
---

Imports an Arrow-compatible file (Parquet, CSV, or JSONL) into a product primitive on the selected branch and space. Rows are written through the standard batch commands, so the import commits like any other write. Returns a summary of the target primitive, the input file, and the imported, skipped, and batch counts.

Status commands return a scalar or compact status payload and do not mutate database state.

## Examples

Import a primitive's rows from a Parquet file written by export.

### CLI

```console
$ strata kv put greeting hello
$ strata arrow export kv parquet /tmp/exports/kv.parquet
$ strata kv delete greeting
$ strata arrow import /tmp/exports/kv.parquet kv  # Rows are keyed by their source column; kv restores greeting=hello.
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
| `file_path` | `string` | yes | — | Input file path. |
| `target` | `ArrowImportTarget` | yes | — | Product primitive to import into. |
| `collection` | `string` | no | — | Target vector collection for vector imports. |
| `format` | `ArrowFileFormat` | no | extension detection | Input file format. |
| `key_column` | `string` | no | — | Optional key column override. |
| `value_column` | `string` | no | — | Optional value, document, or embedding column override. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`StatusResponse<ArrowImport>`.

| Field | Type | Description |
|---|---|---|
| `batches_processed` | `integer` |  |
| `file_path` | `string` |  |
| `rows_imported` | `integer` |  |
| `rows_skipped` | `integer` |  |
| `target` | `ArrowImportTarget` |  |

## Errors

| Code | Meaning |
|---|---|
| [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed) | The runtime is closed. |
| [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch) | The requested branch was not found. |
| [`invalid_argument.engine.product_space`](https://stratadb.org/e/invalid_argument.engine.product_space) | The requested space operation cannot be completed. |
| [`invalid_argument.executor.arrow_feature_disabled`](https://stratadb.org/e/invalid_argument.executor.arrow_feature_disabled) | Arrow support is not enabled in this build. |
| [`invalid_argument.executor.arrow_format`](https://stratadb.org/e/invalid_argument.executor.arrow_format) | The Arrow file format is invalid. |
| [`invalid_argument.executor.arrow_input_missing`](https://stratadb.org/e/invalid_argument.executor.arrow_input_missing) | Arrow input is missing. |
| [`invalid_argument.executor.arrow_key_column`](https://stratadb.org/e/invalid_argument.executor.arrow_key_column) | The Arrow key column is invalid. |
| [`invalid_argument.executor.arrow_value_column`](https://stratadb.org/e/invalid_argument.executor.arrow_value_column) | The Arrow value column is invalid. |
| [`invalid_argument.executor.arrow_collection`](https://stratadb.org/e/invalid_argument.executor.arrow_collection) | The Arrow collection target is invalid. |
| [`invalid_argument.executor.arrow_embedding_type`](https://stratadb.org/e/invalid_argument.executor.arrow_embedding_type) | The Arrow embedding column type is invalid. |
| [`invalid_argument.executor.arrow_vector_dimension`](https://stratadb.org/e/invalid_argument.executor.arrow_vector_dimension) | The Arrow vector dimension is invalid. |
| [`invalid_argument.executor.arrow_json_key`](https://stratadb.org/e/invalid_argument.executor.arrow_json_key) | The Arrow JSON key column is invalid. |
| [`invalid_argument.executor.arrow_base64`](https://stratadb.org/e/invalid_argument.executor.arrow_base64) | The Arrow base64 input is invalid. |
| [`unavailable.executor.arrow_io`](https://stratadb.org/e/unavailable.executor.arrow_io) | The Arrow input or output path is unavailable. |
| [`internal.executor.arrow`](https://stratadb.org/e/internal.executor.arrow) | An internal Arrow boundary error occurred. |

## Invocation

```text
strata arrow import <file_path> <target> [--collection <string>] [--format <ArrowFileFormat>] [--key-column <string>] [--value-column <string>] [--branch <branch>] [--space <space>]
```

- Wire type: `arrow_import`

## Related

- [Export Arrow file](/docs/arrow/export) — Export a product primitive to an Arrow-compatible file.
- [Delete KV value](/docs/kv/delete) — Delete one visible KV key.
- [Get KV value](/docs/kv/get) — Read the current or historical value for one KV key.
- [Put KV value](/docs/kv/put) — Store or replace a KV value by key.
- [All `arrow` commands](/docs/arrow/)
