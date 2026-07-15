---
title: "Check product space existence"
description: "Check whether a product space exists on a branch."
source: strata-core@1.0.0
section: space
---

Reports whether the named product space is cataloged on the target branch. A missing space is `false`, not an error; reserved names are rejected with `invalid_argument.engine.product_space_reserved`.

Status commands return a scalar or compact status payload and do not mutate database state.

## Examples

Check whether a product space exists.

### CLI

```console
$ strata space create app
$ strata space exists app
$ strata space exists nope
```

### Wire

```json
{"space":"app","type":"space_create"}
{"space":"app","type":"space_exists"}
{"space":"nope","type":"space_exists"}
```

### Output

One response per step, in order:

```json
{"data":{"commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":3,"version":3},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"space":"app"},"type":"space_create_result"}
{"data":true,"type":"bool"}
{"data":false,"type":"bool"}
```

## Parameters

_No parameters._

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`StatusValue<bool>`.

## Errors

| Code | Meaning |
|---|---|
| [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed) | The runtime is closed. |
| [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch) | The requested branch was not found. |
| [`invalid_argument.engine.product_space`](https://stratadb.org/e/invalid_argument.engine.product_space) | The requested space operation cannot be completed. |
| [`invalid_argument.engine.product_space_reserved`](https://stratadb.org/e/invalid_argument.engine.product_space_reserved) | The requested space operation cannot be completed. |

## Invocation

```text
strata space exists [--branch <branch>] [--space <space>]
```

- Wire type: `space_exists`

## Related

- [Create product space](/docs/space/create) — Create a product space on a branch.
- [All `space` commands](/docs/space/)
