# M11 / M11T Implementation Plan: V1 Readiness Hardening

Status: draft implementation plan

## Goal

Move from functionally complete to release-grade.

## Inputs

1. `docs/architecture/strata-v1-architecture.md`
2. `docs/architecture/strata-v1-implementation-roadmap.md`
3. `docs/architecture/v1-testing-and-conformance-plan.md`
4. `docs/architecture/v1-engineering-standards.md`
5. `docs/product/strata-v1-product-requirements.md`
6. `docs/product/strata-v1-user-pathways.md`
7. `CLAUDE.md`
8. `docs/architecture/v1-cutover-pr-series.md`
9. `docs/architecture/v1-removed-surfaces.md`
10. `docs/architecture/implementation-plans/m9-m9t-implementation-plan.md`
11. `docs/architecture/implementation-plans/m10-m10t-implementation-plan.md`

All slices must follow the V1 engineering standards: permanent domain names,
concept-budget discipline, file/function thresholds, comment standards, and no
roadmap labels in production code vocabulary.

## Implementation Track

| Epic | Title | Scope | Exit gate |
|---|---|---|---|
| `M11A` | Durability hardening | Close storage crash, fault-injection, retention, repair, and recovery issues found by full-matrix testing. | Durable local committed data survives required crash windows. |
| `M11B` | Product hardening | Close engine product-path, IPC, cache, branch, time-travel, retrieval, clone, and relationship issues. | Required pathways have no known correctness gaps. |
| `M11C` | Model behavior hardening | Close inference and intelligence issues found by feature matrix and fake-provider tests. | Model-assisted behavior degrades predictably. |
| `M11D` | Performance hardening | Investigate benchmark regressions and resource-profile failures. | Regressions are fixed or explicitly accepted with rationale. |
| `M11E` | Release audit | Final API, docs, dependency, terminology, error-code, and security/redaction audit. | V1 is understandable without cleanup-era context. |
| `M11F` | Promotion to main | Execute the reviewed promotion path from `docs/architecture/v1-cutover-pr-series.md`, including branch protection, release tagging, and final merge/fast-forward policy. | `v1` is promoted to `main` only after readiness gates pass. |

## Test Track

| Test epic | Title | Scope | Exit gate |
|---|---|---|---|
| `M11TA` | Full storage matrix | Run fault-injection, crash, recovery, fuzz, golden, property, and stress tests. | No required durability or format failure remains unclassified. |
| `M11TB` | Full product conformance | Run product-pathway conformance across cache, durable local, read-only, IPC, branch/time, retrieval, graph, vector, clone, and model-assisted paths. | Every required pathway is tested and documented. |
| `M11TC` | Full inference/intelligence matrix | Run feature combinations, fake providers, redaction, model mismatch, and degradation tests. | Optional model-assisted paths fail closed or degrade as documented. |
| `M11TD` | Performance suite | Run required benchmarks and compare against threshold policy. | Regressions are within policy or explicitly waived. |
| `M11TE` | Release scans | Run dependency graph, public API, terminology, secret redaction, docs link, and engineering-standard scans. | No release-blocking scan failures remain. |

## Release Audit Slices

`M11E` should split into at least these slices:

1. `M11E1`: public API audit.
2. `M11E2`: docs and examples audit.
3. `M11E3`: dependency graph audit.
4. `M11E4`: terminology and cleanup-era vocabulary audit.
5. `M11E5`: error-code and retry-policy audit.
6. `M11E6`: security, secret-redaction, and prompt/data-leakage audit.

## Convergence Notes

1. `M11TA` through `M11TD` produce the findings that feed `M11A` through
   `M11D`.
2. `M11TE` lands with `M11E` and must be clean before `M11F`.
3. `M11F` is the final promotion slice, not a substitute for readiness gates.

## Slice Policy

M11 slices are bug-fix and hardening slices. Each slice should start with a
specific failing test, audit finding, benchmark regression, or release checklist
item.

## Non-Goals

1. No new architecture direction.
2. No new optional product features.
3. No broad refactors unless they fix a release-blocking issue.
4. No compatibility work for pre-V1 development databases unless separately
   approved.

## Milestone Exit Gate

M11 is complete when V1 is release-ready: product pathways are tested, durable
local recovery is proven, cache mode is explicit, removed surfaces are absent,
error semantics are stable, and the crate graph is understandable to a new
engineer. The roadmap Test Gate Summary remains the canonical milestone gate;
this plan explains how M11 reaches it.
