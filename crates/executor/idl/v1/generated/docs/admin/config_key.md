---
title: "Read one config value"
description: "Read one sanitized configuration value by key."
source: strata-core@1.0.0
section: admin
---

Returns one sanitized configuration value from the allowlist by key, or a null value when the key is not recognized. Only allowlisted, non-sensitive keys are served; an empty key is rejected with `invalid_argument.engine.config_key`.

Optional reads distinguish present data from missing data. When version or timestamp facts exist on the executor output, SDK mappings should preserve them.

## Examples

Read one configuration value by key, or nothing if unknown.

### CLI

```console
$ strata config get-key missing
```

### Wire

```json
{"key":"missing","type":"configure_get_key"}
```

### Output

One response per step, in order:

```json
{"data":null,"type":"config_value"}
```

## Parameters

| Name | Type | Required | Default | Description |
|---|---|---|---|---|
| `key` | `string` | yes | — | Config key. |

## Returns

`Maybe<String>` — a miss returns nothing rather than raising.

## Errors

| Code | Meaning |
|---|---|
| [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed) | The runtime is closed. |
| [`invalid_argument.engine.config_key`](https://stratadb.org/e/invalid_argument.engine.config_key) | The request contains invalid input. |

## Invocation

```text
strata config get-key <key>
```

- Wire type: `configure_get_key`

## Related

- [All `admin` commands](/docs/admin/)
