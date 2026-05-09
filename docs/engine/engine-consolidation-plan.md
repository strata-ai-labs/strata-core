# Engine Consolidation Closeout

## Purpose

This is the active, milestone-free summary of the completed engine
consolidation work.

The historical implementation ledgers live under
[archive/](./archive/). They are audit records, not required reading for the
current architecture.

## Current State

`strata-engine` is the workspace authority layer for database semantics,
runtime composition, primitive behavior, product open/bootstrap, and storage
consumption above `strata-storage`.

The normal production stack is:

```text
core -> storage -> engine -> intelligence -> executor -> cli
```

`stratadb` reaches engine through executor. `strata-intelligence` may reach
`strata-inference` through optional model/provider features, but engine does not
depend on intelligence or inference.

## Consolidated Ownership

Engine owns:

- open options, access mode, and sensitive configuration types
- product open/cache/follower bootstrap
- graph implementation, branch DAG behavior, and graph transaction extensions
- vector implementation, vector indexes, sidecar policy, and vector recovery
- search runtime, search manifests, and retrieval substrate integration
- storage-backed lifecycle, recovery, checkpoint, snapshot, retention, and
  metrics orchestration

The retired peer crates for security, graph, vector, search, and legacy
executor bootstrap are no longer workspace packages.

## Storage Boundary

Only engine may consume `strata-storage` in normal production code above the
storage layer. Executor, intelligence, CLI, root product code, and optional
model/provider code must request storage-backed behavior through engine-owned
APIs.

Allowed direct storage imports outside engine are limited to storage itself,
tests, benches, fuzz targets, diagnostics, and explicit migration or
verification tooling.

The detailed allowlist for engine's storage consumption is
[v1-storage-consumption-contract.md](../storage/v1-storage-consumption-contract.md).

## Guards

The repository keeps guard tests for:

- retired crate reintroduction
- direct storage dependencies above engine
- direct subsystem assembly outside engine-owned runtime composition
- inference/provider dependency direction
- legacy compatibility aliases and compatibility shells

These guards are current architecture checks, not cleanup-phase checks. When one
fails, update the architecture intentionally or fix the regression.
