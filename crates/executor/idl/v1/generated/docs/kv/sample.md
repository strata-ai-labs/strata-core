---
title: "Sample KV rows"
description: "Sample visible KV rows."
source: strata-core@1.0.0
section: kv
---

Returns a deterministic bounded sample of visible KV rows plus the total matching count.

Paginated responses use opaque cursors. Clients should pass the returned cursor back to the same command shape and must not parse cursor contents.

## Examples

A representative sample plus the total population size.

### CLI

```console
$ strata command run --command-json '{"entries":[{"key":"YQ==","value":"MQ=="},{"key":"Yg==","value":"Mg=="},{"key":"Yw==","value":"Mw=="}],"type":"kv_batch_put"}'
$ strata kv sample
```

### Wire

```json
{"entries":[{"key":"YQ==","value":"MQ=="},{"key":"Yg==","value":"Mg=="},{"key":"Yw==","value":"Mw=="}],"type":"kv_batch_put"}
{"type":"kv_sample"}
```

## Parameters

| Name | Type | Required | Description |
|---|---|---|---|
| `count` | `integer` | no | Optional sample count. Defaults to 10. |
| `prefix` | `Bytes` | no | Optional key prefix. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`SamplePage<SampleItem>`.

## Errors

- [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed)
- [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch)
- [`invalid_argument.engine.product_space`](https://stratadb.org/e/invalid_argument.engine.product_space)
- [`invalid_argument.engine.kv_key`](https://stratadb.org/e/invalid_argument.engine.kv_key)

## Invocation

- CLI: `strata kv sample`
- Wire type: `kv_sample`
