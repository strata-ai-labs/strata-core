---
title: "Delete product space"
description: "Delete a product space from a branch."
source: strata-core@1.0.0
section: space
---

Drops the product space from the branch catalog. The `default` space refuses deletion with `invalid_argument.engine.space_delete_default`. A space that still contains visible data refuses deletion with `failed_precondition.engine.space_not_empty` unless `force: true` is set, which tombstones the visible rows first and reports the count. Deleting a space that does not exist succeeds with `deleted: false`.

Successful mutations return an acknowledgement that identifies the affected target, the mutation effect, and commit facts when the operation changed stored state.

## Examples

Delete a product space.

### CLI

```console
$ strata space create temp
$ strata space delete temp
$ strata space exists temp
```

### Wire

```json
{"space":"temp","type":"space_create"}
{"space":"temp","type":"space_delete"}
{"space":"temp","type":"space_exists"}
```

### Output

One response per step, in order:

```json
{"data":{"commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":3,"version":3},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"space":"temp"},"type":"space_create_result"}
{"data":{"commit":{"delete_count":0,"durable":false,"put_count":0,"timestamp":4,"version":4},"deleted_rows":0,"effect":{"affected_count":1,"applied":true,"kind":"deleted","matched":true},"force":false,"space":"temp"},"type":"space_delete_result"}
{"data":false,"type":"bool"}
```

## Parameters

| Name | Type | Required | Default | Description |
|---|---|---|---|---|
| `force` | `boolean` | no | — | Delete visible data in the space before dropping the catalog entry. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`MutationAck<SpaceDelete>`.

| Field | Type | Description |
|---|---|---|
| `deleted_rows` | `integer` | Number of visible space rows tombstoned, including primitive index/control rows. |
| `effect` | `MutationEffect` | Mutation effect facts. |
| `force` | `boolean` | True when visible space data was force-deleted. |
| `space` | `string` | Product space name. |
| `commit` | `CommitReceipt` | Commit receipt when a catalog mutation was applied. |

## Errors

| Code | Meaning |
|---|---|
| [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed) | The runtime is closed. |
| [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch) | The requested branch was not found. |
| [`invalid_argument.engine.product_space`](https://stratadb.org/e/invalid_argument.engine.product_space) | The requested space operation cannot be completed. |
| [`invalid_argument.engine.product_space_reserved`](https://stratadb.org/e/invalid_argument.engine.product_space_reserved) | The requested space operation cannot be completed. |
| [`invalid_argument.engine.space_delete_default`](https://stratadb.org/e/invalid_argument.engine.space_delete_default) | The requested space operation cannot be completed. |
| [`failed_precondition.engine.space_not_empty`](https://stratadb.org/e/failed_precondition.engine.space_not_empty) | The requested space operation cannot be completed. |
| [`invalid_argument.engine.space_delete_too_large`](https://stratadb.org/e/invalid_argument.engine.space_delete_too_large) | The requested space operation cannot be completed. |

## Invocation

```text
strata space delete [--force <boolean>] [--branch <branch>] [--space <space>]
```

- Wire type: `space_delete`

## Related

- [Create product space](/docs/space/create) — Create a product space on a branch.
- [Check product space existence](/docs/space/exists) — Check whether a product space exists on a branch.
- [All `space` commands](/docs/space/)
