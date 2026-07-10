# Strata Self-Understanding Architecture

Status: vision and architecture sketch

## Purpose

This document defines Strata's stance on what makes the database AI-native,
and the architectural shape that follows from that stance.

The thesis is short: every database today is a passive store. You put bytes
in, you get bytes out, and the database has no idea what it holds. Strata
should be the first database whose default behavior is to *understand its own
contents* and *act as a partner* on top of that understanding, with that
behavior driven by recursive AI analysis rather than by hand-curated
analytical APIs.

This is not a query-language improvement and not a BI feature set. It is a
category step: the database stops being a tool and starts being a coworker
that reads everything you put in and proposes what to do about it.

## Related Documents

Architecture anchors:

1. [strata-v1-architecture.md](../architecture/strata-v1-architecture.md)
2. [intelligence-architecture.md](../architecture/intelligence-architecture.md)
3. [engine-architecture.md](../architecture/engine-architecture.md)
4. [inference-architecture.md](../architecture/inference-architecture.md)
5. [storage-architecture.md](../architecture/storage-architecture.md)

Product direction:

1. [strata-v1-graph-relationship-layer.md](../product/strata-v1-graph-relationship-layer.md)
2. [strata-v1-versioning-time-travel.md](../product/strata-v1-versioning-time-travel.md)

## The First-Principles Inversion

A dumb database returns only what you put in. An intelligent database
returns things that *do not exist as stored bytes* and are therefore
unreachable by any query language today: facts that follow from combining
records, gaps it can see against learned structure, hypotheses worth
testing, proposals for self-modification, narratives, patterns that have
no name yet.

The space of what's discoverable is not knowable in advance, and that is
the point. Strata's architecture should not prescribe what the AI will
find. Its job is to make discovery *possible* — to give a recursive AI
loop enough substrate (typed primitives, ontology, branches, time travel,
sandboxed execution, persistent intermediate state) that it can surface
things humans would otherwise spend intuition, expertise, and money to
construct manually, or never construct at all because the analyses are
too broad or too expensive to undertake.

What the AI surfaces is for the AI to decide. The user evaluates findings
through an accept/reject feedback loop. Our job is to make sure the
substrate does not ossify around today's assumptions about what queries
matter or what discoveries are valuable.

## The Architectural Stance

Strata's bet is that **as LLMs get better, our capabilities get better for
free**, but only if we build the right substrate. The substrate must avoid
ossifying around today's assumptions about what queries matter. That leads to
two architectural commitments:

1. **Expose an execution environment, not an analytical query API.** Every
   other database exposes a fixed query surface (SQL, MQL, Cypher, vector
   search). Strata exposes a sandboxed execution environment that AI models
   drive directly, with Strata's primitives as the typed data-access API
   inside the sandbox. Data access is stable and worth ossifying; analytical
   methodology is not.
2. **Curated prompts, not curated tools.** The intuition we encode lives at
   the prompt layer, which is cheap to rewrite as models improve and which
   reflects system goals rather than system behavior. Tool APIs that
   pre-decide what analyses matter become tomorrow's bottleneck.

The product is not "a database with an LLM bolted on." It is a database whose
core loop is recursive AI analysis over its own contents, with the user's
query surface sitting on top of what that loop discovers.

## The Substrate

Strata is uniquely positioned to host this loop because it already has the
raw materials no other database has together:

1. **Five primitives** (KV, JSON, events, vectors, graph) covering the
   storage shapes that real-world data lives in. The recursion has typed
   tools to navigate the corpus.
2. **Ontology**: typed graph relationships. The AI navigates structured edges
   instead of inventing structure from unstructured text on every call.
3. **Branches and time travel**: a place to do analysis without polluting
   production data, and the ability to compare states across time.
4. **Native inference** (`inference-next`) with both local model execution
   (llama.cpp, any HF model) and remote provider APIs (OpenAI, Anthropic,
   Google). The same product runs in regulated air-gapped environments and
   in environments that want frontier-model quality.
5. **System branch**: a durable home for the database's understanding of
   itself, separate from user-owned branches.

## Recursive Language Models as the Analysis Substrate

The analysis loop uses the Recursive Language Model (RLM) pattern: the root
model receives only the query, the corpus exists as data in an external
environment, and the root model programmatically navigates and recursively
delegates over slices of the corpus. RLMs are the natural fit for self-
understanding because:

1. **Long-running analysis is possible.** The reason existing databases
   cannot host hours-long analyses is not compute — it is that they have no
   place to *put* intermediate state. Strata's branches are exactly that
   place. The AI can fork a working branch, persist intermediate
   computations as typed objects (cluster centroids, entity-resolution
   candidates, hypothesis test results), iterate over multiple LLM rounds
   with each round reading the previous round's persisted state, and either
   merge or discard the branch when done.
2. **No single call needs to see the whole corpus.** Each recursive call
   operates on a slice, so context-window limits stop being a ceiling on
   problem size.
3. **The substrate scales with model quality.** As models get better at
   decomposition, the same Strata substrate produces better analysis without
   any change to the engine.

The RLM pattern in Strata replaces the original paper's Python-REPL
environment with a typed Strata query plane (KV scan, vector search, graph
walk, event aggregate, JSON traverse), all branch-aware and time-travel-
capable. Each recursive step is information-dense instead of
text-pattern-fuzzy.

## Execution Environment

For analyses that require general-purpose computation (statistical tests,
ML, simulation, arbitrary transformation), the AI needs an execution
environment, not a fixed function library.

**Pyodide running inside a wasmtime sandbox** is the right execution
environment for Strata:

1. Sandboxed by default. No filesystem, no network, bounded memory and CPU
   and wall-clock timeouts, enforced by wasmtime.
2. Portable across macOS, Linux, Windows, and browser/cache mode without
   per-platform work.
3. Ships the Python data-science stack (numpy, scipy, scikit-learn) as
   pre-built WASM packages, so the AI can use the libraries it already
   knows.
4. Single-binary deployment. Pyodide adds ~30-50 MB to the Strata binary
   but keeps the "one artifact" property a database needs.

Strata's primitives are exposed inside the sandbox as a typed Python module.
The AI reads and writes Strata via that module; it does not see the host
filesystem, host network, or host process. The sandbox is the only surface
through which AI-generated code can touch real data.

We deliberately do not curate the analytical API. The AI brings its own
methodology and writes code; Strata provides data access and execution.

## Inference Topology: Smart Root, Cheap Recursive

The RLM cost model has a natural fit with Strata's dual inference stack:

1. **Root model**: needs strong decomposition and synthesis. API-grade or a
   large local model.
2. **Recursive sub-calls**: operate on small slices, do focused extraction
   or summarization or scoring. A small local model (Qwen3-8B-class via
   llama.cpp) handles these well — the original RLM paper showed that
   RLM-Qwen3-8B beat vanilla Qwen3-8B by 28.3% and approached GPT-5 on three
   of four long-context tasks because most of the cognitive lift happens in
   *how the context is decomposed*, not in raw reasoning per call.

Most tokens in a long analysis flow through the recursive layer. If that
layer is local, total API spend is bounded by the much smaller root-call
volume. Hours-long analyses become economically viable.

The inference topology is configurable per task and per database:

1. Fully local (regulated data, air-gapped, user policy).
2. Smart-root + cheap-recursive (default for cost-sensitive workloads).
3. Fully API (one-off novel analyses where model quality matters).

The choice is recorded in provenance, not hidden as an implementation
detail.

## The System Branch as the Durable Home

Everything the analysis loop discovers, learns, or proposes lives on the
system branch. The system branch is a first-class storage primitive separate
from user-owned branches.

Contents:

1. **Typed edges**: relationships between entities, each with confidence,
   provenance, recency, and evidence pointers.
2. **Materialized summaries**: distributions, cluster topology, anomaly
   priors, schema-shape facts, ontology coverage estimates.
3. **Proposals**: draft branches representing changes the system would make
   to user data (schema migrations, deduplications, ontology refinements,
   index additions). Each proposal is reviewable, accept/reject-able, and
   carries its own justification.
4. **Generated findings**: whatever shape the AI chose — questions,
   narratives, anomalies, hypotheses, gaps, patterns, visualizations — with
   supporting evidence and rendering hints. The schema is open-ended; new
   finding shapes do not require an engine change.
5. **Feedback log**: user accept/reject signals on past proposals and
   insights, used to slide confidence thresholds and refine prompts.

The system branch is refreshed by background RLM passes on a schedule
constrained by the budget loop (see Cost Model). It is never authoritative
about user data — user branches remain the source of truth — but it is
authoritative about the database's beliefs and proposals.

## Provenance as a Data-Governance Primitive

Every fact written to the system branch carries:

1. **Origin**: LLM-derived, rule-derived, user-confirmed, or
   externally-supplied.
2. **Confidence**: calibrated probability where applicable.
3. **Evidence**: pointers to the source records that produced the fact,
   the recursion path that arrived at it, and the inputs to any sandboxed
   computation.
4. **Inference path**: which model at which inference locality (local model
   X via llama.cpp, or remote provider Y) produced this fact, including
   model version.
5. **Recency**: timestamp of derivation and a decay model so stale facts
   age out as the underlying data changes.
6. **Reproducibility hash**: a hash of (inputs, prompt version, model
   version) sufficient to detect drift when re-derived.

Provenance is not a logging concern. It is a **data contract**. Enterprise
customers must be able to tell stored facts from beliefs. Users must be able
to configure policies like "all entity resolution must run on a local model"
and have the system branch *refuse to write* a finding that violated the
policy. Researchers must be able to re-derive findings against a newer model
and compare results meaningfully.

This is the AI-native equivalent of column-level encryption or row-level
security in a traditional database — once "which model produced this" lives
in the data contract, the inference-locality choice becomes a first-class
governance primitive instead of an implementation detail.

## The User-Facing Surface

The product surface is not a curated list of analytical capabilities. It
is a continuously-regenerated stream of findings on the system branch,
each one a thing the AI deemed worth surfacing. Each entry carries:

1. The finding itself, in whatever shape the AI chose — a narrative, a
   question, a proposal, a visualization, a relationship, a hypothesis, a
   gap, a pattern, or a shape we have not named yet.
2. Supporting evidence and provenance.
3. Calibrated confidence.
4. Interaction affordances: accept, reject, ask for variants, suppress
   future findings of the same shape. All of these flow back into the
   feedback loop.

The user is the reviewer. The AI is the analyst. No category of finding
is prescribed by the architecture, no product surface is reserved for a
specific kind of output, and no use case is hardcoded. Surfaces emerge
from the substrate; they are not designed by us in advance.

Some shapes will likely appear early simply because they are foundational
to other findings — entity resolution and relationship detection both
have this property, because downstream findings are unstable without
them. This is an *expected emergent property* of the substrate, not a
product roadmap. The AI may surface other things first, and as models
improve the kinds of findings it produces will shift in ways we cannot
predict today. The architecture is built to accommodate that shift
without engine changes.

## The Feedback Loop

The system is not "fire-and-forget AI." Without a feedback loop, the
analysis layer becomes a firehose of variable-quality output that users
learn to ignore.

The loop is intentionally minimal:

1. The system surfaces a finding or proposal.
2. The user marks it useful, irrelevant, wrong, or asks for variants.
3. The signal flows back into the system branch as a feedback entry,
   sliding per-database confidence thresholds and refining prompt context
   on subsequent passes.

Over time, the system learns the user's taste in addition to learning the
data. The accept/reject record is also the substrate for evaluating newer
models against older ones on the same database.

## Cost Model

Long-running recursive analysis must be bounded. Without an explicit budget
loop, an enthusiastic system branch could quietly run up a large bill
overnight.

The model is per-database and per-task:

1. Each database has a configurable inference budget (cost units per day or
   per analysis).
2. Each analysis task declares its expected budget and its inference
   topology (local-only, smart-root + cheap-recursive, fully-API).
3. Background passes are scheduled within budget. Foreground user-driven
   analyses consume budget but can be prioritized.
4. Budget exhaustion downgrades remote-API tasks to local-model alternatives
   or defers them, rather than failing silently.
5. Provenance records the inference path that was actually used, so cost is
   auditable.

## Non-Goals

1. No replacement for the user's query API. Existing engine retrieval,
   commands, and primitive access remain the source of truth. The
   intelligence layer adds capabilities; it does not subtract them.
2. No autonomous mutation of user-owned branches. All system-proposed
   changes land as branches the user explicitly accepts.
3. No hidden inference. Every model call is recorded in provenance and
   bounded by the budget loop.
4. No curated analytical API. The execution environment is general-purpose;
   the only curated surface is the data-access API exposed inside the
   sandbox.
5. No proprietary scripting language. Pyodide ships standard Python; users
   and the AI both target the same ecosystem.
6. **No prescription of what the AI must discover, surface, or prioritize.**
   The architecture is open-ended by design. Encoded predictions about
   what the AI will produce become tomorrow's bottleneck as models improve.
   The substrate is ossified; the analytical methodology and the surfaced
   findings are not.

## Open Questions

The shape of the substrate is clear, but several decisions remain open:

1. **Refresh cadence and triggering**. Time-based, write-volume-based,
   user-activity-based, or some combination. Affects budget consumption and
   freshness of the understanding layer.
2. **System branch schema**. The exact typed shapes for findings, proposals,
   relationship edges, and feedback entries are still to be specified. They
   should be extensible without breaking older entries.
3. **Confidence calibration approach**. Per-edge-type? Global? Learned
   from accept/reject history? The threshold question (narrow-conservative
   vs broad-noisy) needs a per-database default and a per-user override.
4. **Cross-database understanding**. StrataHub's role in sharing
   understanding across databases (without leaking data) is out of scope
   for V1 but worth keeping in mind so the system branch shape does not
   preclude it.
5. **Sandboxed data-access API shape**. The Python module exposed inside
   Pyodide should be ergonomic but should not leak internals that would
   ossify the engine. Designing this API is the first concrete piece of
   work after the architecture is accepted.

## Summary

A traditional database is a glorified drawer. Strata's bet is that the
database should be a partner: it reads what you put in, understands it,
proposes what to do about it, and answers questions you did not know to
ask. The architecture is:

1. Strata's primitives, ontology, branches, and time travel as the
   substrate.
2. Recursive Language Models as the analysis loop, running in branches with
   persistent intermediate state.
3. Pyodide on wasmtime as the sandboxed execution environment.
4. Dual inference (local via llama.cpp, remote via API) with smart-root +
   cheap-recursive as the default cost-efficient topology.
5. System branch as the durable home for findings, proposals, and feedback.
6. Provenance as a first-class data contract.

The capabilities improve automatically as models improve, because Strata
ossifies the substrate (data access, execution environment, persistence,
provenance, feedback) and leaves *what* to analyze, *how* to analyze it,
and *what to surface* entirely to the model. We do not predict what the
RLM will discover. The whole point is that it surfaces things humans
would otherwise spend intuition, expertise, and money to construct
manually, or never construct at all.
