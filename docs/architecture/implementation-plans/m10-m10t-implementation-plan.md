# M10 / M10T Implementation Plan: Executor, CLI, SDK, Tests, Benches, And Docs Cutover

Status: draft implementation plan

## Goal

Make the V1 integration line ready for promotion without exposing `*-next`
architecture to users.

## Inputs

1. `docs/architecture/strata-v1-implementation-roadmap.md`
2. `docs/architecture/engine/public-api-and-cli-surface-cleanup-checklist.md`
3. `docs/architecture/engine/ipc-and-command-boundary-contract.md`
4. `docs/architecture/engine/dataset-clone-artifact-contract.md`
5. `docs/architecture/v1-existing-test-inventory-and-porting-plan.md`
6. `docs/product/strata-v1-product-requirements.md`
7. `docs/architecture/v1-cutover-pr-series.md`
8. `docs/architecture/v1-removed-surfaces.md`
9. `docs/architecture/implementation-plans/m9-m9t-implementation-plan.md`

All slices must follow the V1 engineering standards: permanent domain names,
concept-budget discipline, file/function thresholds, comment standards, and no
roadmap labels in production code vocabulary.

## Implementation Track

| Epic | Title | Scope | Exit gate |
|---|---|---|---|
| `M10A` | Product crate cutover | Route executor, CLI, SDK, and product entry points to engine-next and intelligence-next APIs. | Product crates do not call old engine/storage internals. |
| `M10B` | Canonical crate rename | Replace old canonical crate implementations with the V1 stack and shed `next` suffixes. | Public package names are normal and old crates are retired. |
| `M10C` | CLI and IPC surface | Update CLI commands, IPC daemon protocol access, same-machine sharing UX for multiple local processes, and clone commands to V1 semantics. | CLI reflects required V1 product pathways. |
| `M10D` | Public API cleanup | Remove or hide old public surfaces, internal escape hatches, and stale compatibility names. | Public API audit matches the V1 surface checklist. |
| `M10E` | Docs and examples | Update user docs, examples, architecture links, and stale terminology. | New users do not need old cleanup documents to understand V1. |
| `M10F` | Bench and dependency cutover | Update benchmarks, guard tests, and workspace dependency audits. | Performance and crate-graph checks run against the V1 stack. |
| `M10G` | Cutover PR series plan | Complete `docs/architecture/v1-cutover-pr-series.md` with exact PR order, dependency cuts, package renames, promotion steps, and retirement guards. | Cutover execution has a reviewed checklist before crate renames start. |

## Test Track

| Test epic | Title | Scope | Exit gate |
|---|---|---|---|
| `M10TA` | Product-path end-to-end tests | Run required product pathways through public APIs, executor, and CLI where applicable. | End-to-end behavior matches product docs. |
| `M10TB` | IPC tests | Validate local IPC ownership, read-only clients, maintenance authority, command serialization, and cache-mode rejection. | Same-machine sharing works without server-mode semantics. |
| `M10TC` | Removed-surface scans | Scan source, docs, public API, and CLI help for `docs/architecture/v1-removed-surfaces.md`. | Removed surfaces stay removed. |
| `M10TD` | Benchmark harness | Run required performance benchmarks and record baseline changes. | Regressions are classified before V1 readiness. |
| `M10TE` | Dependency graph audit | Check retired crates and forbidden edges. | Workspace graph matches the target architecture. |
| `M10TF` | Pre-V1 rejection tests | Attempt to open pre-V1 development databases and malformed clone artifacts. | Failures are structured and user-actionable. |

## Convergence Notes

1. `M10G` closes before `M10B` begins crate rename work.
2. `M10TA` grows as `M10A`, `M10C`, and `M10D` cut over product paths.
3. `M10TB` lands with `M10C`.
4. `M10TC` lands with `M10D` and references the canonical removed-surface list.
5. `M10TD` and `M10TE` close before M11 readiness hardening.
6. `M10TF` lands before public docs claim V1 format behavior.

## Slice Policy

Cutover slices may be larger than normal because crate renames and public API
routes are coupled. Do not preserve old and new product paths indefinitely just
to keep intermediate compatibility.

## Non-Goals

1. No new product features beyond the V1 surface.
2. No migration tool for pre-V1 development databases.
3. No network server mode.
4. No hidden sync or upload behavior.

## Milestone Exit Gate

M10 is complete when product crates, CLI, SDK surfaces, docs, tests, benches, and
dependency guards all point at the canonical V1 stack. The roadmap Test Gate
Summary remains the canonical milestone gate; this plan explains how M10 reaches
it.
