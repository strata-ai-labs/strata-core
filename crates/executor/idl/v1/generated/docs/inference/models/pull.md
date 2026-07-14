---
title: "Download a model"
description: "Download an inference model locally."
source: strata-core@1.0.0
section: inference
---

Resolves a catalog name or model spec and downloads the model artifact into the local model directory, returning the resolved local path. Honors `STRATA_MODELS_DIR` for the destination and `STRATA_HF_ENDPOINT` and `STRATA_HF_TOKEN` (or `HF_TOKEN`) for gated HuggingFace repositories. Downloading requires network access and a build with the local execution feature; cloud-only builds return `inference.unsupported_operation`.

## Parameters

| Name | Type | Required | Description |
|---|---|---|---|
| `model` | `string` | yes | Model spec or catalog name. |

## Returns

`PullModelOutput`.

## Errors

- [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed)
- [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch)
- [`inference.download_disabled`](https://stratadb.org/e/inference.download_disabled)
- [`inference.missing_model`](https://stratadb.org/e/inference.missing_model)
- [`inference.download_failed`](https://stratadb.org/e/inference.download_failed)
- [`inference.download_verification_failed`](https://stratadb.org/e/inference.download_verification_failed)
- [`inference.io_failure`](https://stratadb.org/e/inference.io_failure)
- [`inference.registry_corrupt`](https://stratadb.org/e/inference.registry_corrupt)

## Invocation

- CLI: `strata inference models pull`
- Wire type: `inference_models_pull`
