# Llama FFI Unsafe Audit

Status: audit placeholder

## Purpose

This document is the required audit record for inference-next local runtime FFI
unsafe code.

M7E owns completing this audit before the local llama.cpp runtime can be treated
as V1-ready.

## Required Audit Scope

The completed audit must cover:

1. Every `unsafe` block in the local runtime boundary.
2. Pointer ownership and lifetime rules.
3. Thread-safety and send/sync assumptions.
4. Model artifact loading and unload ordering.
5. Buffer length and alignment assumptions.
6. Error propagation across the FFI boundary.
7. Panic behavior across FFI boundaries.
8. Cleanup behavior after timeout, cancellation surrogate, or provider failure.

## Acceptance Gate

The audit is complete when:

1. Every unsafe block has a nearby `SAFETY:` comment.
2. Each comment names the invariant that makes the block sound.
3. Local runtime tests cover normal load, failed load, failed generation,
   failed embedding, failed rerank, and teardown.
4. A second reviewer signs off before V1 readiness.
