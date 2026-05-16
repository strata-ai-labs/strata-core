# Strata AI Architecture

Status: V1 architecture draft

## Purpose

This document defines `strata-ai`, the agentic harness layer that sits at
the top of the Strata stack and delivers the intelligent-partner
capabilities the database exposes to users.

`strata-ai` is not a separate product running alongside Strata. It is the
top-level consumer of `intelligence-next` and `inference-next` that
orchestrates recursive AI analysis over the database, hosts the sandboxed
execution environment for AI-generated code, manages the system branch
lifecycle, and brokers multi-model inference across local and remote
providers. Without `strata-ai`, Strata is a fast multi-primitive database;
with `strata-ai`, Strata is an AI-native database that understands itself
and acts as a partner on top of that understanding.

## Related Documents

Architecture anchors:

1. [strata-v1-architecture.md](./strata-v1-architecture.md)
2. [intelligence-next-architecture.md](./intelligence-next-architecture.md)
3. [inference-next-architecture.md](./inference-next-architecture.md)
4. [engine-next-architecture.md](./engine-next-architecture.md)
5. [runtime-resource-profile-architecture.md](./runtime-resource-profile-architecture.md)
6. [v1-error-and-diagnostics-contract.md](./v1-error-and-diagnostics-contract.md)
7. [v1-engineering-standards.md](./v1-engineering-standards.md)

Intelligence layer documents:

1. [../intelligence/self-understanding-architecture.md](../intelligence/self-understanding-architecture.md)
2. [../intelligence/residual-capabilities.md](../intelligence/residual-capabilities.md)

Product direction:

1. [strata-v1-product-requirements.md](../product/strata-v1-product-requirements.md)
2. [pathways/retrieval-and-intelligence.md](../product/pathways/retrieval-and-intelligence.md)

## Requirement Language

1. Must means V1 strata-ai architecture is incomplete without it.
2. Should means expected for V1 unless a later architecture decision records
   a clear deferral.
3. May means allowed but not required for V1.

## Position In The V1 Stack

```text
core-next
  → storage-next
  → engine-next
  → intelligence-next → executor / CLI / SDK / strata-ai
  → inference-next
```

`strata-ai` is a consumer at the top of the stack. It must depend on
`intelligence-next` and `inference-next` and may depend on `engine-next`
for product surfaces that the intelligence layer does not need to
abstract. It must not depend on `storage-next` directly. It must not
expose its own primitive storage APIs; it consumes them through engine
surfaces.

## Product Role

`strata-ai` exists so Strata can deliver capabilities that no existing
database can structurally produce. These capabilities are catalogued in
the intelligence layer documents above and emerge as residual properties
of the substrate plus a recursive AI analysis loop driving it. The
harness owns the loop.

`strata-ai` is responsible for:

1. Driving Recursive Language Model (RLM) analysis loops over database
   contents.
2. Hosting the Pyodide-on-wasmtime sandboxed execution environment for
   AI-generated code.
3. Owning the system branch lifecycle: refresh cadence, contents shape,
   accept/reject feedback recording, decay of stale findings.
4. Brokering multi-model inference: routing each call to the right
   model and provider through `inference-next` based on per-task
   inference topology (local-only, smart-root + cheap-recursive,
   fully-API).
5. Enforcing the inference budget loop and recording every model call in
   provenance.
6. Hosting the user-facing findings stream and the accept/reject
   interaction surface.
7. Surfacing reactive AI subscriptions to applications that want to be
   notified when the analysis loop produces new findings.

## Boundary

`strata-ai` does not own:

1. **Database semantics.** Branches, commits, time travel, primitive
   operations, ontology storage, and deterministic retrieval are owned by
   `engine-next`.
2. **Model execution.** Loading models, invoking local inference through
   llama.cpp, calling remote provider HTTP APIs, and managing model
   artifacts are owned by `inference-next`.
3. **Model orchestration contracts.** The `QueryExpander`,
   `ResultReranker`, `RagGenerator`, and embedding traits remain in
   `engine-next`. `intelligence-next` installs implementations of these
   traits per database. `strata-ai` consumes the installed
   implementations through `intelligence-next` rather than reimplementing
   them.
4. **Storage primitives.** All reads and writes go through engine
   surfaces. The Pyodide sandbox sees a typed engine-shaped module, not
   storage internals.
5. **Provider HTTP.** Any call to OpenAI, Anthropic, Google, or any other
   provider goes through `inference-next`. `strata-ai` never speaks
   provider protocols directly.

The boundary test: anything that requires AI reasoning, multi-step
decomposition, or orchestration across model calls is `strata-ai`'s
responsibility. Anything an embedded application could call through
engine-next/intelligence-next surfaces without an AI brain in the loop is
not.

## Deployment Modes

Strata ships in two deployment modes, both produced from the same
codebase through compile-time feature gating.

### Lite

Lite mode strips out `strata-ai`, `inference-next`, the
intelligence-driven parts of `intelligence-next`, the Pyodide-on-wasmtime
sandbox, and any model-dependent code paths. What remains is the full
substrate: five primitives, branches, time travel, ontology storage,
deterministic retrieval, durable services, and the engine surfaces that
sit above them.

Lite mode must:

1. Compile and link without any model code, inference dependencies, or
   the Pyodide runtime present in the final binary.
2. Run on resource-constrained devices including RPi Zero class
   hardware.
3. Open a database that was previously used in Full mode without
   migration. System-branch contents become inert and may be ignored or
   exposed read-only as historical findings; no new findings are
   produced.
4. Provide the vector primitive with caller-supplied embeddings. Auto-
   embedding is a Full-mode capability.

Lite mode is the answer for regulated industries that require provable
absence of AI code in the deployed binary, for IoT and edge devices that
cannot host a full inference stack, and for read-only or
ingestion-focused replicas in a larger fleet.

### Full

Full mode includes `strata-ai`, `inference-next`, the complete
`intelligence-next` surface, the Pyodide-on-wasmtime sandbox, and any
bundled models the deployment chooses to ship with.

Full mode must:

1. Open a Lite database cleanly and begin producing system-branch
   findings on the configured refresh cadence.
2. Support the same deployment shapes as Lite (embedded, server, cloud).
   Anything that ties Full mode to a specific deployment shape, such as
   cloud-only or server-only, breaks the embedded-full-intelligence
   property and is not acceptable.
3. Honor the inference budget loop and the inference topology choice
   recorded with each task.

Mode selection must be a compile-time feature gate, not a runtime flag.
A runtime toggle cannot satisfy compliance requirements that need the
absence of AI code to be physically demonstrable.

## Multi-Model Broker

`strata-ai` is a model-agnostic broker over `inference-next`. It does
not pick a model at compile time and does not assume a specific provider
is available at runtime.

Each analysis task declares its inference topology:

1. **Local-only**: every call routes to a local model via the llama.cpp
   path in `inference-next`. Required for sensitive data, regulated
   workloads, air-gapped deployments, and any database policy that
   forbids egress.
2. **Smart-root + cheap-recursive**: the root model of an RLM call uses
   an API-grade model or a large local model; recursive sub-calls route
   to a small local model. This is the default cost-efficient topology
   for long-running analyses.
3. **Fully-API**: every call routes to a remote provider. Appropriate
   for novel one-off analyses where model quality dominates cost.

The chosen topology is recorded in provenance for every fact written to
the system branch, so users can audit which model at which locality
produced which finding. Database administrators can configure policies
like *"all entity resolution must run on a local model"* and the system
branch must refuse to write a finding produced by a remote model. This
is what turns inference locality into a first-class data-governance
primitive rather than an implementation detail.

`strata-ai` is responsible for backpressure when budget is exhausted:
remote-API tasks may downgrade to local-model alternatives or be
deferred. Failures and downgrades are visible in the findings stream
rather than silently dropped.

## Deployment Shapes

`strata-ai` must ship in every deployment shape `strata-core` ships in.

1. **Embedded**: linked into the same binary as the application,
   including mobile, desktop, and IoT targets where compatible with the
   target's resource envelope.
2. **Server**: standalone process, optionally with a sidecar layout for
   the inference workload.
3. **Cloud / managed**: hosted Strata service, with `strata-ai`
   colocated in the same deployment as the database it serves.

Three packaging shapes, one codebase. If `strata-ai` becomes
cloud-or-server-only, the embedded-full-intelligence residual capability
collapses and Strata becomes another two-tier "AI database" product that
any competitor can reproduce. The architecturally distinctive choice is
treating the agent harness as something that runs wherever Strata runs.

## The Sandboxed Execution Environment

`strata-ai` hosts a Pyodide-on-wasmtime sandbox in the data plane. The
sandbox is the only path through which AI-generated code touches data.

Sandbox requirements:

1. Sandboxed by default: no filesystem access, no network access,
   bounded memory and CPU and wall-clock per execution.
2. Strata's primitives exposed inside the sandbox as a typed Python
   module that calls through engine surfaces. The AI sees engine-shaped
   data access; it does not see storage internals.
3. Bundled Python data-science stack (numpy, scipy, scikit-learn) as
   pre-built WASM packages, so the AI can use libraries it already
   knows.
4. Portable across macOS, Linux, Windows, and browser/cache mode without
   per-platform sandbox work.

`strata-ai` does not curate the analytical methodology available inside
the sandbox. The substrate is general-purpose; what runs over it is for
the AI to decide.

## System Branch Lifecycle

`strata-ai` owns the system branch. The system branch holds the
database's beliefs about itself: relationship edges, materialized
summaries, proposals, generated findings, and the feedback log.

Lifecycle responsibilities:

1. Scheduling background analysis passes within budget.
2. Writing findings with full provenance (origin, confidence, evidence,
   inference path, reproducibility hash, recency).
3. Recording user feedback signals and feeding them back into prompt
   context and confidence calibration.
4. Decaying stale findings as the underlying data changes.
5. Refusing to write findings that violate the configured inference-
   locality or budget policies.

The system branch is durable storage handled by `engine-next` and
`storage-next`. `strata-ai` writes through engine surfaces; it does not
have its own storage path.

## Curated Prompts, Not Curated Tools

The intuition `strata-ai` encodes lives at the prompt layer, not the
tool API layer. Prompts express system goals ("look for entity
duplicates," "flag distributional drift," "propose visualizations") and
are cheap to revise as model capabilities change. The tool surface
exposed inside the Pyodide sandbox is a typed *data-access* API, which
is stable and worth ossifying.

This is the architectural choice that lets `strata-ai`'s capabilities
improve automatically as models improve. Tool APIs that pre-decide what
analyses matter become tomorrow's bottleneck.

## Non-Goals

1. **No replacement of engine-next surfaces.** Deterministic retrieval,
   primitive commands, branch operations, and the time-travel API remain
   the source of truth. `strata-ai` adds capabilities; it does not
   replace existing surfaces.
2. **No direct provider HTTP.** All model calls go through
   `inference-next`. `strata-ai` does not implement Anthropic, OpenAI,
   Google, or local model protocols directly.
3. **No autonomous mutation of user-owned branches.** All system
   proposals land as branches the user explicitly accepts or rejects.
4. **No proprietary scripting language.** Pyodide ships standard Python.
5. **No prescription of what the AI must discover.** `strata-ai`'s job
   is to run the loop, not to define the outputs.
6. **No cloud-only or server-only path.** Every capability `strata-ai`
   exposes must work in embedded deployments.
7. **No hidden inference.** Every model call is bounded by the budget
   loop and recorded in provenance.

## Open Questions

1. **Crate boundary.** Does `strata-ai` ship as a single crate, a small
   crate family, or a thin top-level binary that composes several
   internal crates? The right answer depends on whether the Pyodide
   sandbox, the agent loop, and the multi-model broker have natural
   separation as independent units.
2. **Embedded sandbox cost.** The Pyodide runtime adds 30-50 MB to the
   binary. Lite mode excludes this entirely; Full mode must include it.
   Whether intermediate "Full minus sandbox" deployments are useful is
   an open product question.
3. **Refresh cadence policy.** Whether the system-branch refresh
   schedule is time-based, write-volume-based, user-activity-based, or
   some combination has not been decided.
4. **Feedback loop persistence.** The exact shape of the accept/reject
   record and how it feeds back into prompt context and confidence
   thresholds needs to be specified before the first user-facing
   surface ships.
5. **Cross-database learning through StrataHub.** Whether `strata-ai`
   instances in a fleet share learnings (without sharing data) through
   StrataHub is a substrate decision that must not preclude the
   architecture even though it is out of scope for V1.
6. **Cargo feature naming.** The Rust feature flags for Lite vs Full
   need to be settled and applied consistently across the workspace.

## Summary

`strata-ai` is the agentic harness layer that delivers Strata's
intelligent-partner capabilities. It drives recursive AI analysis loops
over the database, hosts a sandboxed execution environment for
AI-generated code, brokers multi-model inference across local and remote
providers, manages the system branch lifecycle, and enforces provenance
and budget invariants. It ships in two compile-time modes — Lite
(absent) and Full (present) — across all deployment shapes that Strata
itself ships in. Capabilities improve automatically as models improve
because `strata-ai` ossifies the substrate and orchestration shape and
leaves analytical methodology and discovery to the model.
