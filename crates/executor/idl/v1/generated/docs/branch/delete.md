---
title: "Delete branch"
description: "Delete an active branch and release its storage claims."
source: strata-core@1.0.0
section: branch
---

Deletes an active branch and reports the deleted branch summary, generation facts, and storage cleanup counts. The `default` branch refuses deletion with `invalid_argument.engine.branch_delete`. There is no merge in V1: work on a fork is either kept by continuing on that branch or discarded by deleting it.

Successful mutations return an acknowledgement that identifies the affected target, the mutation effect, and commit facts when the operation changed stored state.

## Examples

Delete a branch.

### CLI

```console
$ strata branch create temp
$ strata branch delete temp
$ strata branch list
```

### Wire

```json
{"branch":"temp","type":"branch_create"}
{"branch":"temp","type":"branch_delete"}
{"type":"branch_list"}
```

## Parameters

_No parameters._

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`MutationAck<BranchDelete>`.

## Errors

- [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed)
- [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch)
- [`invalid_argument.engine.branch_name`](https://stratadb.org/e/invalid_argument.engine.branch_name)
- [`invalid_argument.engine.branch_name_reserved`](https://stratadb.org/e/invalid_argument.engine.branch_name_reserved)
- [`invalid_argument.engine.branch_delete`](https://stratadb.org/e/invalid_argument.engine.branch_delete)

## Invocation

- CLI: `strata branch delete`
- Wire type: `branch_delete`
