# V1 Cutover PR Series

Status: draft planning placeholder

## Purpose

This document is the owner for the exact PR sequence that promotes the V1 stack
from build-branch crates to the canonical Strata crate graph.

The detailed sequence is produced during `M9G`. It is intentionally not fully
specified before core-next, storage-next, engine-next, inference-next, and
intelligence-next have real crate shapes.

## Required Content Before M9 Cutover

The completed cutover PR series must list:

1. Workspace member additions and removals.
2. Package rename order.
3. Dependency edge changes.
4. Public crate re-export changes.
5. Executor, CLI, SDK, benchmark, and docs cutover order.
6. Retired crate deletion order.
7. Guard tests that prevent retired crates or forbidden edges from returning.
8. Pre-V1 database rejection behavior.
9. Branch protection and final promotion steps for `v1` to `main`.

## Baseline Sequence Shape

The expected sequence is:

1. Verify M6 and M8 product surfaces are stable.
2. Cut product crates to V1 engine/intelligence APIs on the `v1` branch.
3. Rename build-phase crates to canonical package names.
4. Delete retired crate implementations and stale compatibility paths.
5. Update workspace manifests, lockfile, benches, docs, and examples.
6. Add dependency and removed-surface guards.
7. Run M9 product-path, CLI, IPC, dependency, docs, and benchmark gates.
8. Run M10 readiness gates.
9. Promote `v1` to `main` only after readiness gates pass.

## Non-Goals

1. No old/new permanent compatibility layer.
2. No pre-V1 database migration tool unless a later approved plan changes the
   cutover policy.
3. No hidden sync, upload, or network server behavior.
