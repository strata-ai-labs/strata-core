# V1 Removed Surfaces

Status: V1 architecture draft

## Purpose

This document is the canonical list of old public or semi-public surfaces that
must not survive as normal V1 product workflows.

Milestone plans should reference this document instead of repeating their own
removed-surface list.

## Removed From The V1 Product Surface

1. Follower mode.
   Same-machine multi-process access is IPC-owned.

2. Public manual transaction sessions.
   Internal commit machinery remains. Users get normal write and batch
   semantics, not begin/commit/rollback sessions.

3. Disk-backed cache mode.
   Cache mode is explicit and non-durable. Durable local databases use standard
   or always durability policies.

4. Branch bundles.
   Dataset clone artifacts and StrataHub substrate replace this direction.

5. Branch tags, notes, and labels as first-class V1 features.
   These may return later if product workflows prove they are necessary.

6. Normal-user maintenance commands.
   Users should not run ordinary `flush`, `compact`, `checkpoint`, `gc`,
   `repair`, retention, or manual recovery commands during normal use.
   Internal maintenance remains allowed under engine/storage lifecycle control.
   Owner-local maintenance authority for `strata up` remains an operational
   capability, not a normal user data workflow.

7. Public subsystem instantiation.
   Product callers should not assemble graph/vector/search-style subsystems to
   open a database.

8. Raw engine or storage escape hatches in ordinary product APIs.
   Test-only or advanced internal escape hatches must be feature-gated and
   absent from the normal public surface.

## Guard Expectations

V1 cutover must include guards that check:

1. Public API exports.
2. CLI command help and command schema.
3. Executor/SDK command surfaces.
4. Docs and examples.
5. Dependency graph and direct imports.
6. Error messages that still reference removed modes.

Removed-surface tests should prove absence. They should not preserve old
behavior.
