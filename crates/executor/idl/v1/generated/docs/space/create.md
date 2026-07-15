---
title: "Create product space"
description: "Create a product space on a branch."
source: strata-core@1.0.0
section: space
---

Creates a product space in the branch catalog. Creation is idempotent: creating a space that already exists succeeds with `created: false` and no mutation effect. Names reserved for engine control data are rejected with `invalid_argument.engine.product_space_reserved`.

Successful mutations return an acknowledgement that identifies the affected target, the mutation effect, and commit facts when the operation changed stored state.

## Examples

Create a product space.

### CLI

```console
$ strata space create app
$ strata space list
```

### Wire

```json
{"space":"app","type":"space_create"}
{"type":"space_list"}
```

### Output

One response per step, in order:

```json
{"data":{"commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":3,"version":3},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"space":"app"},"type":"space_create_result"}
{"data":{"cursor":null,"has_more":false,"items":["app","default"]},"type":"space_list"}
```

## Parameters

_No parameters._

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`MutationAck<SpaceCreate>`.

| Field | Type | Description |
|---|---|---|
| `effect` | `MutationEffect` | Mutation effect facts. |
| `space` | `string` | Product space name. |
| `commit` | `CommitReceipt` | Commit receipt when a catalog mutation was applied. |

## Errors

| Code | Meaning |
|---|---|
| [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed) | The runtime is closed. |
| [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch) | The requested branch was not found. |
| [`invalid_argument.engine.product_space`](https://stratadb.org/e/invalid_argument.engine.product_space) | The requested space operation cannot be completed. |
| [`invalid_argument.engine.product_space_reserved`](https://stratadb.org/e/invalid_argument.engine.product_space_reserved) | The requested space operation cannot be completed. |

## Invocation

```text
strata space create [--branch <branch>] [--space <space>]
```

- Wire type: `space_create`

## Related

- [List product spaces](/docs/space/list) — List product spaces on a branch.
- [All `space` commands](/docs/space/)
