# Residual Capabilities

Status: architectural-consequences catalog

## Purpose

This document catalogs capabilities that *become possible* as a consequence
of Strata's architecture. They are not features we work toward consciously
and they are not part of any product roadmap. They are observations about
what a carefully constructed substrate makes available for free.

The framing matters. If we treat these as features to build, we ossify
predictions about how users will benefit from the architecture, and we
optimize the substrate toward today's understanding of what's valuable.
Instead, we treat them as residue: the architecture is the work, these
fall out, and the actual product surfaces emerge from users discovering
they can finally do things that were structurally impossible before.

The intelligence-loop surfaces (entity resolution, relationship detection,
findings stream, visualization suggestions, schema proposals, and so on)
are catalogued separately in
[self-understanding-architecture.md](./self-understanding-architecture.md).
This document covers the *other* residual capabilities — the ones that
enable new user workflows, new things to build on Strata, and new
operational properties — that follow from the substrate's shape rather
than from the intelligence loop's outputs.

## Related Documents

1. [self-understanding-architecture.md](./self-understanding-architecture.md)
2. [strata-v1-architecture.md](../architecture/strata-v1-architecture.md)
3. [intelligence-architecture.md](../architecture/intelligence-architecture.md)
4. [inference-architecture.md](../architecture/inference-architecture.md)

## The Substrate, Briefly

The capabilities below come from one fact: Strata co-locates six
AI-relevant properties in a single engine. Every existing database has at
most two of them.

1. **Five primitives**: KV, JSON, events, vectors, graph.
2. **Branches**: Git-style, isolated, mergeable.
3. **Time travel**: every state is addressable as of any point in history.
4. **Native inference**: local via llama.cpp (any HF model) and remote via
   provider APIs, both in the data plane.
5. **Ontology**: typed graph relationships, evolvable.
6. **System branch + provenance**: durable, attributable, auditable belief
   state separate from user data.

Plus the architectural decision to **expose an execution environment
(Pyodide on wasmtime) rather than a curated analytical API**, which lets
analyses span all six properties without engine changes.

What follows is what becomes possible when these are present together.

## Workflow Consequences

### Hypothesis Branches

Branches exist primarily for dev/staging workflows in databases that have
them (Dolt, Neon, PlanetScale). Once branches sit next to native inference
and persistent intermediate state, they become the natural medium for
*experimentation*: each hypothesis the AI or the user wants to test gets
its own branch, intermediate computations are persisted as typed objects,
multiple rounds of analysis read previous rounds' results, and the user
either merges what worked or discards the branch.

Today, teams do this in scratch notebooks that drift from production data
or in ad-hoc dev environments that lose state between sessions. Strata
makes hypothesis tracking a first-class data workflow with the same
durability and audit properties as any other branch operation.

Required: branches + persistent intermediate state + native inference.

### AI-Mediated Semantic Merge

Git solved structural merge for code: line-level diffs, conflict markers,
three-way merge. Git cannot solve *semantic* merge — it has no model of
what the content means. Strata can: when two branches diverge, the
intelligence loop can describe what each branch implies semantically, what
data each preserved, what each change would do downstream, and propose a
resolution with provenance.

The capability is open-ended: AI-assisted merge could surface
"these two edits express the same intent in different shapes," "this
branch implies a schema migration the other branch doesn't,"
"these two entity-resolution decisions contradict — here's the evidence
on each side." Multi-user workflows on databases have always been hard;
this is the first substrate where they can be merged with understanding
instead of by hand.

Required: branches + ontology + native inference + system branch.

### Reactive AI Subscriptions

Change data capture (CDC) emits "this row changed." Strata can emit
something richer: "the intelligence loop just surfaced a new finding about
this entity," "a new hypothesis crossed 0.9 confidence in the system
branch," "drift detected in this cohort's behavior compared to last week."

Streams of *inference events* — not just data events — become a primitive
that applications can subscribe to. The AI's analysis loop has its own
clock and produces its own outputs; subscribing to those outputs is no
different in shape from subscribing to row changes, but the content is
qualitatively new.

Required: native inference + system branch + event primitive.

## Substrate Consequences

### Agent State Substrate

AI agents today fake persistent state with hacks: a vector store labeled
"memory," ad-hoc JSON for "world model," random event logs for action
history, none of which are linked or queryable together. Strata happens
to be purpose-shaped for agent state:

1. KV for working memory.
2. Events for action history.
3. Vectors for semantic memory.
4. Graph for the agent's world model.
5. Branches for "what did the agent know at time T."
6. Time travel for replay of agent decisions.
7. Provenance for audit of agent behavior.
8. Sandboxed execution for agent tools that read and write the agent's
   own state.

No database in production today is designed around agent state. Most
agent frameworks (LangChain, LlamaIndex, AutoGen) stitch together three or
four external systems to approximate the shape Strata has natively. The
capability falls out — we did not design for it, but every property an
agent needs is already there.

Required: all five primitives + branches + time travel + provenance.

### Embedded Full Intelligence

Every "AI database" today is two-tier: the database stores rows, a
separate inference API runs the model, data leaks across the boundary.
Strata runs full RLM-driven analysis locally through llama.cpp, no
network egress required. The same capabilities that work for a cloud
deployment also work for:

1. Regulated industries (healthcare, finance, defense) where data cannot
   leave the boundary.
2. Air-gapped deployments where there is no boundary to cross.
3. Edge and mobile applications where round-trip latency to a cloud API
   is incompatible with the UX.
4. Consumer privacy-sensitive products where the user controls their data.

This is a category of customer that most "AI-native" databases
structurally cannot serve. The capability follows directly from the
inference-next design choice to support both local and remote inference
in the same data plane.

Required: native inference (specifically the local-model path).

### Cross-Primitive Single-Engine Analysis

"Find users (KV) with high-engagement events (events) whose support
tickets (JSON) cluster (vectors) with the churn-risk cohort (graph)"
today requires four separate systems and a glue layer that has to manage
consistency between them. Strata runs the entire analysis in one query
plane, and the recursive AI loop can compose primitives freely on each
sub-call.

The capability is not just convenience. It is *latency* (no
cross-service hops), *consistency* (one transactional substrate), and
*expressiveness* (the AI can pick the right primitive for each step of an
analysis without coordinating across product boundaries). Every step in
an RLM recursion becomes information-dense, with the right tool for the
right slice.

Required: all five primitives in one engine.

### Self-Tuning Everything

Databases have always had partial autotuning: index advisors, query
plan caches, vacuum schedules. These are rudimentary because the database
does not understand the semantic structure of the data it stores. Strata
does — through the intelligence loop and the system branch — which means:

1. Indexes (including vector and graph indexes) get proposed based on
   *what queries semantically mean*, not just what queries run frequently.
2. Partitioning, compaction, and caching tune themselves against the
   actual workload shape with awareness of which data is hot, cold,
   reference, or transient.
3. Retention and archival policies become inferable: the database can
   observe that certain event types are queried only within 30 days of
   creation and propose archival rules accordingly.

The capability is not a tuning *feature*; it is the natural consequence
of having self-understanding co-located with execution. Users can accept,
reject, or override any proposal — the system never reorganizes silently.

Required: system branch + intelligence loop + execution environment.

## Time and Audit Consequences

### Temporal Model Evaluation

Vector databases have no time travel. Time-travel databases have no native
inference. Strata has both, which makes a category of questions answerable
for the first time:

1. "How was this customer understood six months ago vs now?"
2. "Did our retrieval quality drift because the data changed or because
   the embedding model changed?"
3. "Did this entity-resolution edge become wrong after we deployed the
   new embedding model?"
4. "Re-run yesterday's analyses with today's model and compare findings."
5. "Show me the entities the previous model thought were related but the
   current model does not."

This is a production pain point for every team running embeddings or
deployed models. Provenance records which model produced which fact at
which time, so the comparisons are reproducible.

Required: time travel + native inference + provenance.

### Reproducibility For AI Work

ML reproducibility is a known nightmare: data versions live in DVC, model
versions in MLflow, code versions in Git, evaluation runs in spreadsheets,
none of them aligned. Strata happens to have time travel + provenance +
branches in one substrate, which means:

1. "Re-run this analysis on the exact data and model state I had on
   April 3rd" is a one-line operation.
2. Every fact in the system branch can be re-derived against the same
   inputs and the comparison surfaced as a finding.
3. Branches representing experiments are durable and addressable
   indefinitely — no scratch-notebook decay.
4. The reproducibility hash on each provenance entry detects whether a
   re-derivation matches the original or has drifted.

Every ML team in the world fights this. Nobody has solved it because
nobody has all three layers (time travel, branches, provenance) in one
system. Strata does, by accident of architecture.

Required: time travel + branches + provenance.

## What This List Is Not

1. **Not a roadmap.** None of these capabilities have planned
   implementation slices. They surface as user-facing capabilities when
   users discover they can do them, not because we decided to ship them
   on a date.
2. **Not exhaustive.** The list will grow as the substrate matures and as
   users find combinations we did not anticipate. Treat this document as
   a snapshot of obvious consequences, not a complete inventory.
3. **Not prescriptive about prioritization.** A reader might be tempted
   to read this as "what to focus on next." That is the wrong reading.
   The focus is the substrate; these capabilities are signals that the
   substrate is well-shaped.
4. **Not commitments.** If any of these turn out to be unimportant or to
   require substrate changes that conflict with more important
   properties, we do not chase them.

## Open-Ended By Design

The point of cataloging residual capabilities is to validate that the
substrate is doing structural work, not to enumerate features to build.
If the substrate is right, this list grows on its own as users and the
intelligence loop discover combinations we have not thought of.

When evaluating future architectural decisions, the test is not "does
this make residual capability X better?" The test is "does this preserve
the substrate properties that produce capabilities like these?" Architecture
choices that *narrow* what's possible in exchange for making one capability
sharper are usually wrong — they trade open-endedness for specificity, and
specificity ossifies.
