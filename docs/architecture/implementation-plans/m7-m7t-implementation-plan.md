# M7 / M7T Implementation Plan: Inference-Next Hardening

Status: draft implementation plan

## Goal

Stabilize provider and local model execution before intelligence-next depends
on it.

## Inputs

1. `docs/architecture/inference-architecture.md`
2. `docs/architecture/v1-error-and-diagnostics-contract.md`
3. `docs/architecture/runtime-resource-profile-architecture.md`
4. `docs/architecture/v1-engineering-standards.md`
5. `docs/audits/llama-ffi-unsafe-audit.md`

All slices must follow the V1 engineering standards: permanent domain names,
concept-budget discipline, file/function thresholds, comment standards, and no
roadmap labels in production code vocabulary.

## Implementation Track

| Epic | Title | Scope | Exit gate |
|---|---|---|---|
| `M7A` | Task traits | Implement task-specific `Generator`, `Embedder`, and `Reranker` traits. | Intelligence can request only the task it needs. |
| `M7B` | Request and response DTOs | Implement generation, embedding, and rerank DTOs with item-level outcomes where required. | Partial failures are representable without provider-specific types. |
| `M7C` | Model parser and registry | Implement `parse_model_spec`, deterministic model-spec parsing, model registry lookup, `InferenceCapability`, and machine-local model metadata. | Bare names, provider prefixes, casing, opaque model IDs, and capability reports behave as specified. |
| `M7D` | Provider policy and errors | Enforce network-disabled policy, auth redaction, timeout behavior, retry classes, and `inference.*` codes. | Provider failures classify through the global diagnostics contract. |
| `M7E` | Local runtime safety | Isolate local llama.cpp runtime and finish `docs/audits/llama-ffi-unsafe-audit.md`. | Unsafe code is limited to the approved boundary with `SAFETY:` comments. |
| `M7F` | Fake-provider testkit | Add deterministic fake providers for generation, embedding, rerank, failures, and latency. | Intelligence tests can run without real models or network. |

## Test Track

| Test epic | Title | Scope | Exit gate |
|---|---|---|---|
| `M7TA` | Feature matrix | Build no-default, local, cloud-provider, fake-provider, and max feature combinations using `cargo hack check --workspace --feature-powerset --depth 2` where practical. | Supported combinations compile and unsupported combinations fail clearly. |
| `M7TB` | Parser and registry tests | Property-test model specs and registry behavior. | Parser behavior is deterministic and documented. |
| `M7TC` | Provider fake tests | Exercise fake success, auth failure, timeout, rate limit, missing model, unsupported parameter, and malformed response. | Error codes and retry policies are stable. |
| `M7TD` | Redaction tests | Verify secrets, prompts, and provider payloads are not leaked by default. | Diagnostics are useful without exposing sensitive data. |
| `M7TE` | Unsafe audit tests | Compile and test local runtime boundaries where enabled. | Audit checklist is complete before V1 readiness. |

## Convergence Notes

1. `M7TA` runs after each feature-affecting epic.
2. `M7TB` lands with `M7C`.
3. `M7TC` and `M7TD` land with `M7D` and `M7F`.
4. `M7TE` lands with `M7E`.
5. M7 may run in parallel with storage-next and engine-next after M1, but M8
   cannot close until M7 task traits and fake providers are stable.

## Slice Policy

Inference-next must not import storage, engine, intelligence, executor, or CLI.
Slices should be organized around provider execution contracts, not Strata
product behavior.

## Non-Goals

1. No Strata database concepts.
2. No autoembedding queue.
3. No retrieval recipes.
4. No provider HTTP inside intelligence-next.
5. No post-V1 provider families unless explicitly feature-gated and documented.

## Milestone Exit Gate

M7 is complete when inference-next exposes stable generation, embedding, and
rerank task boundaries with deterministic errors and fake-provider coverage.
The roadmap Test Gate Summary remains the canonical milestone gate; this plan
explains how M7 reaches it.
