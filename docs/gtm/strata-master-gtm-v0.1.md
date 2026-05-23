# Strata Master GTM Document

**Version:** 0.1
**Status:** Living document — parent for every external Strata asset
**Audience:** Anyone writing, designing, demoing, or talking about Strata externally

---

## Vision

> **A world where every person uses the abundant intelligence at their disposal to uplift their lives.**

A teacher in rural Africa builds a flashcard app that teaches math in the native language. An electrician in China builds a website for her regional business. A schoolchild in India builds an Android app to track her parents' finances. Each of them, using the abundant intelligence around them to make life materially better.

That is the world Strata exists for. Software building is being democratized faster than any wave before it, and the data layer is one of the most important layers in every app. Strata makes it possible for that layer to keep up — so the teacher, the electrician, and the schoolchild can build what they imagine without an infrastructure tax standing in the way.

The vision is not about Strata. It is about the world Strata makes more feasible.

---

## 0. How to use this document

This is the parent GTM document for Strata. Every external asset — homepage copy, launch posts, blog posts, social posts, demo scripts, pitch decks, sales conversations, technical content, ads, hiring pages, FAQs — pulls from here.

Three rules:

1. **When a derivative contradicts this doc, fix the doc first.** This document is the single source of truth for external messaging. If a new insight or correction emerges, update here, then propagate downstream. Don't let derivatives drift from the master.
2. **Don't add anything to a derivative that isn't grounded here.** If a piece of copy needs a claim, a feature description, an audience framing, or a comparison that isn't in this document, add it here first.
3. **Read sections 1–4 before writing anything external.** Mission, thesis, tagline, positioning. Everything else assumes those are internalized.

Related artifacts:
- **Internal vision (deeper, more architectural):** `docs/gtm/strata-vision-v0.2.md`
- **GTM execution playbook (surface-specific patterns):** `docs/gtm/strata-gtm-playbook-v0.1.md`
- **Founder essay:** `docs/gtm/why-we-started-strata.md`
- **Self-understanding architecture (hidden depth):** `docs/intelligence/self-understanding-architecture.md`

---

## 1. Mission

Strata exists to help democratize software building. AI is the latest cycle in a long arc — printing press, electricity, computers — where transformative technology becomes accessible to far more people than the experts who first wielded it. Each cycle needs new infrastructure to actually reach people. AI needs a data layer designed for the way people are going to build now, not for the way enterprise teams built before.

Strata is the founder's contribution to that work, focused specifically on the data layer.

The mission is not "build a better database." The mission is to make sure the next wave of builders — many of them non-traditional, AI-augmented, indie, small-team — has data infrastructure worthy of what they're trying to do.

### Vision & Impact

Strata isn't just another database — it's infrastructure for the democratization of software creation. By slashing the "infrastructure tax" and giving AI agents safe, powerful state management, it enables orders of magnitude more people to solve real problems.

### End State

Strata becomes as ubiquitous for the next generation of builders as SQLite was for the last. A high-margin, enduring company that powers millions of applications while staying true to the mission: more people building real things, without drag, so more real problems get solved.

### Company Scope (the wedge is the database; the company is broader)

> **Strata exists to help anyone turn abundant intelligence into useful software, without becoming an infrastructure expert first. The database is where we start. The work is broader.**

This is the company-level framing. It sits between the vision (the world we want) and the product tagline (*"Database for the next 100 million builders"*).

The database is the wedge because every real app needs state, memory, recovery, structure, search, and control. Solving the data layer is where Strata starts. But Strata is not a database company. Over time, the company can expand into other layers that today block ordinary people from turning intelligence into working software: deployment, authentication, synchronization, observability, app generation, local agents, workflows, collaboration. Each new product would be downstream of the same vision and purpose. **Strata expands wherever the friction lives — wherever a hard technical layer stands between human intent and working software.**

The strategic implication: do not let positioning, pitches, hiring conversations, investor materials, or external copy box Strata as "a database company." The database is the first product. The company is the larger work — making software creation radically more accessible by collapsing the infrastructure burden between human intent and working applications.

This is the Stripe pattern. *"Payments for developers"* was Stripe's first tagline; *"the economic infrastructure for the internet"* was always the company. That framing is what made every product after the first one possible without identity crisis. Strata follows the same structure: the database is the wedge, the broader company is the destination.

---

## 2. Core Thesis

**AI agents fundamentally need a ground-up rethinking of what databases mean.**

The category "database" was defined fifty years ago for a specific operator: human teams at corporate scale, making careful, deliberate writes against a linear timeline. Every primitive in the category — schemas as contracts, linear write logs, ACID semantics, SQL as the interface language, indexes tuned for human-paced workloads, branches as deploy environments, backups as the recovery story — was shaped by what that operator needed.

The operator has changed. AI agents generate schemas, mutate state at machine speed, retrieve memory, ingest documents, test hypotheses, run migrations, coordinate tools, produce application logic. They are fast, parallel, willing to fail, capable of remembering everything, uninterested in human-readable errors. None of the category's defining primitives were designed for that operator.

Strata is built from the other direction: starting from what an AI agent (and the people building with one) actually needs from a data layer, and reshaping every primitive accordingly.

---

## 3. Tagline & Wording

### Primary tagline (positioning — who it's for)

> **Database for the next 100 million builders.**

This is the committed external tagline. Use it as the headline on the homepage, the opening line of launch posts, the first slide of pitch decks, the social bio.

### Secondary tagline (benefit — what it does)

> **Slash the infrastructure tax.**

This is the benefit-framed companion to the positioning tagline. Use it as a featured callout on the homepage, a section header in the pitch deck, a recurring campaign frame in blog posts and social posts, and the imperative phrase for swag and community surfaces (stickers, t-shirts, hackathon prizes). Pairs cleanly with the primary tagline — the primary says *who*, the secondary says *what for them*.

The phrase *"infrastructure tax"* itself is a recurring vocabulary anchor — use it consistently across long-form writing to give the GTM stack a thematic spine.

### One-line supporting descriptions

After the tagline, the next line must break the BaaS/cloud-Postgres expectation. Approved patterns:

- *"A complete database that runs anywhere — from a Raspberry Pi to a Xeon, with no signup, no connection string, no cloud overhead."*
- *"Not a cloud backend. A complete data layer that ships with your app — from a solo project to a small SaaS, all on the same binary."*
- *"Most apps don't need a cloud database. They need a complete one that lives with them."*
- *"A database that ships with your app — not behind a connection string to someone else's cloud."*

### Short product descriptions

**Twitter / X bio (≤120 chars):**
> Database for the next 100 million builders. Complete, embedded, runs anywhere.

**One sentence:**
> Strata is a complete database for the next 100 million builders — one binary that runs from a Raspberry Pi to a Xeon, holds every kind of state your app needs, branches like git, and runs AI inference locally.

**One paragraph:**
> Strata is the database for the next 100 million builders. It runs as a single binary on anything from a Raspberry Pi to a Xeon server. Inside that binary: structured records, documents, events, vectors, and graph relationships, all in one query layer. It branches like git, time-travels in milliseconds, and runs AI inference locally — so your data stays with your app, your agents can experiment without breaking what works, and you ship without operating a stack.

**Founder casual answer ("what are you working on?"):**
> "I'm building a database for the next 100 million builders. AI is going to let way more people build things — and most of them don't need a cloud database, they need a complete one that lives with their app. I know databases, so I'm building it."

---

## 4. Positioning

### Category claim

Strata is in the **substrate** category — the foundational data engine layer. Same shelf as SQLite, DuckDB, Postgres. Not the same shelf as managed BaaS products that get built *on* substrate engines.

The application-layer products that serve AI builders — memory services, retrieval-as-a-service, agent infrastructure, vector databases, RAG platforms — are potential customers of Strata, not competitors. The relationship is analogous to how Postgres relates to Supabase, or how SQLite relates to thousands of apps that embed it.

### What Strata IS

- A complete database (multi-form, versioned, branchable, retrievable)
- Embedded (single binary, ships with apps, no signup)
- Designed for AI agents as primary operators (machine-actionable errors, schema-as-context, MCP-native)
- Capable of running entirely offline (native local inference)
- A substrate that other data products and applications get built on

### What Strata is NOT

- A cloud backend or BaaS product
- A managed Postgres alternative
- A vector database
- An "AI assistant for your existing database"
- A bolt-on AI feature on a traditional database
- Only for developers — it's for the wider builder population

### The disruption window

Today's mental model of "database" routes listeners to cloud BaaS (Supabase, Firebase, Neon). The three-AI test (Grok, ChatGPT, Claude) confirms this. The mis-routing is not a positioning failure — it is proof of disruption. Strata exists in the gap that current category understanding cannot see.

Customer education is therefore part of the GTM, not external to it. Every external surface must do category-correction work alongside the tagline.

---

## 5. Target Audience

### Primary ICP: agent-infrastructure and data-product builders

Founders and teams building AI-native data products: memory layers, retrieval engines, agent persistence tools, RAG-as-a-service, versioned dataset registries, agent observability platforms, vertical AI tools with rich persistence needs. Anyone currently stitching together a relational database, a vector index, a key-value store, an event log, and custom glue code.

Their pain: they're doing substrate work that should already be done. They want to ship the memory algorithm, the retrieval model, the agent ergonomics — not maintain a multi-database stack.

### Secondary wedge: AI-assisted application builders

Indie hackers, AI-native founders, small teams shipping fast. They use coding agents to create applications, schemas, seed data, migrations. Their pain: the forced choice between under-equipped (Postgres-with-pgvector alone) and over-complex (stitching together cloud services). They need a complete data layer that ships with their app.

### Expansion: enterprise agent platform teams

Organizations deploying agents into real workflows — operations, support, data cleanup, internal copilots, customer-facing automation. They need stateful infrastructure for memory, retrieval, audit, supervision, and recovery, with governance and policy support.

### Downstream beneficiaries: the wider builder population

Solo creators, citizen developers, AI-augmented non-engineers. Strata reaches them through the products built on it (and, in some cases, directly when they want to use the substrate). They are not the buyer; they are the user of products built on Strata.

### Use-case anchors (canonical demo personas)

These four are the proof cases — each represents a real builder current data infrastructure mis-serves, and each breaks the BaaS expectation:

- **The AI agent startup at 50–100 users.** Can't justify a Pinecone + Postgres + Neo4j stack at that scale.
- **The internal analytics team.** Pulling from Salesforce, Stripe, GitHub, Slack into a unified queryable surface across different data shapes.
- **The vibecoder building a calculator for an Excel sheet.** Solo, replacing spreadsheet workflows with a real tool, AI-assisted construction.
- **The CEO with a private mental model on her laptop.** Solo, explicitly local-first / air-gapped, graph + notes + simulation, never leaves the device.

---

## 6. Tesla Bar & Reaction Moments

### The bar

> **"Why doesn't every database work like this?"**

This is the bar every Strata feature, demo, and first-60-seconds experience must clear. People will not have seen a database like this before. The reaction is the category-bending moment. Without it, Strata is just another product; with it, Strata is the start of a new category understanding.

Demos must be designed to produce this reaction in 60 seconds, ideally 10. If a demo doesn't produce a reaction, redesign the demo. If a feature doesn't have a reaction-moment behind it, it might not be a V1 headline feature.

### The seven reaction-moments

Each Strata demo, surface, or first interaction should produce at least one of these reactions:

1. **Branching:** *"I made changes on a branch, tested them, merged the good ones, threw away the bad ones. Why doesn't every database let me do this?"*
2. **Time travel:** *"I rolled the database back to ten minutes ago with one command. Why doesn't every database do this?"*
3. **Multi-form unified:** *"I stored a KV cache, documents, events, vectors, and graph relationships in one database, in one query plan, in one binary. Why doesn't every database do this?"*
4. **Embedded everywhere:** *"It runs on my laptop. Same binary runs on a Pi and on a Xeon. No signup. No connection string. Why doesn't every database do this?"*
5. **Native local inference:** *"I ran semantic search against my data with a model I downloaded from Hugging Face, completely offline. Why doesn't every database do this?"*
6. **Self-understanding:** *"The database told me it found three patterns I hadn't noticed in my data. Why doesn't every database do this?"*
7. **StrataHub clone:** *"I cloned a real dataset, branched it, ran my analysis on the branch, and the original is still pristine. Why doesn't every database do this?"*

---

## 7. Features & Capabilities

Strata's capabilities are organized below by primitive group. Each section gives a one-paragraph description, the customer-value framing, the reaction-moment it can produce, and example use cases.

### 7.1 The Five Storage Primitives

Strata holds five forms of state in one substrate: **key-value, documents, events, vectors, and graph relationships.** These are not separate products. They are different views over one branch-aware, versioned state engine.

**Why this matters.** Today's builders stitch together a relational database for application data, a key-value store for cache and scratch, a vector index for semantic memory, an event log for audit, and sometimes a graph database for relationships. Five products, five APIs, five failure modes, five sets of credentials, five bills. Strata replaces the stitch with one substrate. Same versioning, branching, retrieval, and recovery semantics apply uniformly across all five.

**Reaction-moment:** *"I stored events, KV, vectors, and graph relationships in one database, in one query plan, with one binary."*

**Use cases:**
- AI agent startups consolidating their full state into one engine
- Internal analytics teams unifying multi-source, multi-shape data
- Agent memory products built without DIY storage
- Any app that has outgrown SQLite but doesn't want six cloud services

### 7.2 Branching (Git-Style Operations)

Every state in Strata can be forked into an isolated branch. Branches support **fork, merge, cherry-pick, diff, rollback, and conflict handling.** Branching is cheap by construction — copy-on-write semantics mean a branch creates almost no storage overhead until it diverges.

**Why this matters.** Code has git. Deployment has rollback. Infrastructure has Terraform. Until Strata, the data layer had no equivalent. AI agents working against a branched database produce *proposals*, not direct mutations. A failed agent run discards its branch. A successful one merges. The developer can diff between branches to inspect exactly what changed.

This is the safety model agents actually need. Without branching, every agent action is a direct mutation of trusted state. With branching, every agent action is provisional until accepted.

**Reaction-moment:** *"I made changes on a branch, tested them, merged the good ones, threw away the bad ones."*

**Use cases:**
- Agent runs that need isolated scratch state
- Migration testing without affecting production
- Data ingestion under uncertainty (messy inputs land on a branch, get cleaned, get promoted)
- Multi-agent coordination (each agent works on its own branch, branches merge under policy)
- Preview environments for collaborative work

### 7.3 Time Travel

Any past state of any branch is recoverable in milliseconds. **State diff between two points in time is a primitive operation,** not a backup-and-restore procedure. Recovery is part of the substrate, not an operational discipline.

**Why this matters.** Traditional databases treat history as a backup problem. You take backups; if something goes wrong, you restore from a backup that's hours or days old. Strata treats history as a first-class read primitive. You can query the database *as of* any point in time. You can diff between two points. You can roll back to any prior state without operational intervention.

For AI workloads, this is essential. Agents make many changes per session; users need to inspect what happened, recover from mistakes, and replay decisions against known prior state.

**Reaction-moment:** *"I rolled the database back to ten minutes ago with one command."*

**Use cases:**
- Debugging agent behavior by replaying against the state the agent saw
- Auditing what changed during an agent session
- Recovering from human mistakes without restoring backups
- Reproducible retrieval (querying the same state that produced a prior decision)

### 7.4 Multi-Modal Search

Strata's search substrate handles **keyword search (BM25), vector similarity search, graph traversal, and hybrid search** that combines all three — all over the same data, in one query layer.

**Why this matters.** AI applications routinely need text search, semantic search, and relationship traversal in the same query: *"find documents about X that are similar to this one, owned by user Y, related to entity Z."* Today, this requires three separate systems and a query orchestrator. Strata makes it one query against one substrate.

**Reaction-moment:** *"I searched with keyword and embeddings and relationships in one query — and the results were ranked together."*

**Use cases:**
- RAG over heterogeneous data (documents + structured records + entity relationships)
- Agent memory retrieval that needs to combine semantic recall with structured filters
- Search products that previously required combining Elasticsearch + Pinecone + Neo4j
- Knowledge graph applications with semantic retrieval

### 7.5 Embedded Deployment

Strata ships as a single binary that runs on **anything from a Raspberry Pi Zero to a Xeon server** — same binary, same primitives, same code. No signup, no connection string, no cloud round-trip. In-process; runs alongside the application.

**Why this matters.** The default mental model of "database" assumes cloud — Postgres-as-a-service, managed BaaS, connection strings to someone else's infrastructure. That model is wildly over-applied. Most apps don't need cloud-scale, multi-region, multi-tenant operational infrastructure. They need a complete database that ships with them.

Strata's embedded model means: no provisioning, no operational complexity, no infrastructure tax, no cloud round-trip in the development loop. It also means privacy by default — your data is on your machine unless you explicitly send it elsewhere.

**Reaction-moment:** *"It runs on my laptop. Same binary runs on a Pi and on a Xeon. No signup. No connection string."*

**Use cases:**
- Solo / indie projects where cloud overhead is the deal-breaker
- Edge deployments (IoT, on-device, regulated environments)
- Local development that mirrors production exactly (no separate dev/prod stack)
- Apps shipping as installable software that includes their own database
- Air-gapped environments (regulated industries, defense, healthcare)

### 7.6 Native Inference

Strata includes a built-in inference engine that can **download and run any Hugging Face model locally** via llama.cpp, and also call out to remote provider APIs (OpenAI, Anthropic, Google) where desired. The same product runs in fully air-gapped environments and in environments that want frontier-model quality.

The inference topology is configurable per task: **fully local** (regulated data, air-gapped, user policy), **smart-root + cheap-recursive** (default for cost-sensitive workloads — strong model decomposes, small local model handles recursive sub-calls), or **fully API** (one-off novel analyses where model quality matters most).

**Why this matters.** Most "AI database" pitches assume cloud-based inference. That excludes a huge category of real users — anyone in regulated industries, anyone with privacy concerns, anyone running embedded products, anyone whose data can't leave the device. Strata's native local inference makes the air-gapped case a first-class deployment mode, not an afterthought.

It also makes long-running analyses economically viable. The smart-root + cheap-recursive topology means most tokens in a long analysis flow through cheap local models; only decomposition uses expensive remote models. Hours-long agent analyses become affordable.

**Reaction-moment:** *"I ran semantic search against my data with a model I downloaded from Hugging Face, completely offline."*

**Use cases:**
- Private intelligence on a laptop (the CEO mental model demo)
- Regulated industries that cannot send data to cloud APIs
- Cost-sensitive long-running agent analyses
- Embedded products that need on-device AI

### 7.7 Self-Understanding Architecture (Strata AI)

Strata includes a recursive AI loop that **runs over the database's own contents** — analyzing structure, finding patterns, proposing improvements, surfacing insights. This is not "an AI assistant for your database." It is the database actively understanding and proposing what's in itself.

The loop uses the Recursive Language Model (RLM) pattern: a strong root model receives queries and programmatically navigates the corpus via Strata's typed primitives (KV scan, vector search, graph walk, event aggregate, JSON traverse). Each recursive step is information-dense. Intermediate state persists on Strata's branches — the substrate that makes hours-long agent analysis feasible.

For analyses requiring general-purpose computation (statistical tests, ML, simulation), the AI runs Python code in a **Pyodide sandbox inside wasmtime** — bounded by sandbox controls, with access to numpy, scipy, scikit-learn pre-built.

Everything the loop discovers lives on a **system branch** — a first-class storage primitive separate from user-owned branches. Contents include typed edges between entities, materialized summaries, draft proposals (schema migrations, deduplications, ontology refinements), and generated findings. Every finding carries **provenance**: origin, confidence, evidence pointers, inference path (which model at which locality), recency, reproducibility hash.

**Why this matters.** Every database today is a passive store. You put bytes in, you get bytes out, the database has no idea what it holds. Strata is built to actively understand its own contents and partner with the user — surfacing facts, hypotheses, proposals, and gaps that no query language could reach.

**Reaction-moment:** *"The database told me it found three patterns I hadn't noticed in my data."*

**Use cases:**
- Data teams getting automated anomaly and pattern surfacing
- Research analysis where the AI proposes hypotheses worth testing
- Agent observability that explains agent behavior in terms of state evolution
- Schema and indexing improvements proposed by the system itself

### 7.8 Agent-Native Interfaces

Strata is designed for an AI agent to use directly, not via a human translator. This means:

- **Agent-readable schema.** The schema is a typed artifact the agent reads in one prompt (target: fits in <2,000 tokens for the modal project), with entities, relationships, indexes, branch metadata, permissions, and examples.
- **Machine-actionable errors.** Errors return structured fields: error type, relevant object, failed operation, cause, suggested recovery, safety level. The agent can act on the error directly.
- **MCP-native operations.** Tool-native interfaces for `create_branch`, `query_state`, `semantic_search`, `inspect_schema`, `propose_change`, `diff_branch`, `rollback_branch`, `merge_branch`, `explain_change`, `replay_run`.

**Why this matters.** When the operator is an AI agent, traditional database interfaces lose information. Cryptic error messages become opaque exceptions. Schema introspection requires multiple calls. Operations that are obvious in a human GUI are tedious in a tool-call sequence. Strata flips the contract: the agent gets the same dignity of interface that a human DBA would, plus structured machine-readable affordances that humans don't need.

**Reaction-moment:** *"The agent invokes the database directly through MCP — and when something fails, the error tells the agent exactly what to do next."*

**Use cases:**
- Any project where an agent writes to a database (which, increasingly, is most projects)
- Autonomous workflow systems
- Agent memory products built on Strata
- IDE/coding-agent integrations

### 7.9 StrataHub

StrataHub is the cloud registry layered on top of Strata, modeled on the GitHub pattern. Developers can **publish, clone, branch, fork, and pull-request datasets** the way they do code repositories. V1 ships with curated datasets; V2 opens to user-published datasets.

**Why this matters.** Data is currently the most-friction artifact in software development. You can clone a code repo in seconds; cloning a real dataset means CSV exports, manual ETL, schema reconciliation, and weeks of work. StrataHub treats datasets the way Git treats code — as branchable, versioned, mergeable artifacts that propagate by clone and pull request.

This is also how Strata reaches a new audience: builders who clone interesting datasets from StrataHub adopt Strata as the runtime that operates them.

**Reaction-moment:** *"I cloned a real dataset, branched it, ran my analysis on the branch, and the original is still pristine."*

**Use cases:**
- Sharing realistic test data across teams
- Distributing curated public datasets (open data, benchmarks, training corpora)
- Cloning agent-generated state across projects
- Building reproducible agent benchmarks
- Forking and customizing other people's datasets without losing provenance

### 7.10 Provenance & Audit

Every fact written to Strata (especially on the system branch) carries **origin, confidence, evidence pointers, inference path (model + locality + version), recency, and a reproducibility hash.** Provenance is not a logging concern — it is a data contract.

**Why this matters.** When AI is the operator, "where did this fact come from" becomes a first-class governance question. Was this entity-resolution finding produced by a local model or a remote one? At what model version? With what confidence? Strata makes the answer queryable. Users can set policies like *"all entity resolution must run on a local model"* and have the system *refuse to write* a finding that violated policy.

This is the AI-era equivalent of column-level encryption or row-level security — a new governance primitive made necessary by the new operator.

**Reaction-moment:** *"I can tell exactly which model produced this fact, at which locality, with what confidence — and I can re-derive it against a newer model to detect drift."*

**Use cases:**
- Regulated industries requiring audit of AI-derived data
- Research that needs reproducible AI-driven analysis
- Multi-agent systems where attribution matters
- Privacy-sensitive deployments where inference locality is a policy

### 7.11 Pyodide Sandbox

For analyses requiring general-purpose computation — statistical tests, machine learning, simulation, arbitrary transformation — Strata exposes a **Pyodide-on-wasmtime sandbox**. The AI brings its own methodology and writes Python code; Strata provides typed data access and bounded execution.

The sandbox is **sandboxed by default**: no filesystem, no network, bounded memory and CPU and wall-clock timeouts, enforced by wasmtime. The Python data-science stack (numpy, scipy, scikit-learn) ships as pre-built WASM packages. The whole thing adds ~30–50 MB to the Strata binary but keeps the single-binary deployment property.

**Why this matters.** Strata deliberately does not curate an analytical API. As models improve, the curated API would become tomorrow's bottleneck. Instead, Strata provides the data access (stable, worth ossifying) and the execution environment (general-purpose), and lets the AI bring its own analytical methodology. Capabilities improve as models improve.

**Use cases:**
- Custom statistical analyses (regressions, hypothesis tests, simulations)
- ML model evaluation against database contents
- Data transformations beyond what query languages express
- The execution substrate for the self-understanding loop

---

## 8. Value Propositions by Audience

### For data-product founders (primary ICP)

**The promise:** Stop doing substrate work. Build the domain layer.

You need a versioned KV store, a branch-aware vector index, an event log with point-in-time recovery, and a graph layer with the same consistency model as the rest. Today that's a custom stack and months of engineering. With Strata, those primitives are already coherent. Build the memory algorithm, the retrieval model, the agent ergonomics that are actually your product. Ship in months what would have taken years on a stitched stack.

### For AI-assisted application builders (secondary wedge)

**The promise:** A complete database your AI can work with safely, that ships with your app.

Your agent can branch, experiment, fail, and recover — without breaking what works. Your data stays with your app. You ship without operating a stack of cloud services. The forced choice between "Postgres with pgvector and a prayer" and "six cloud services your project never asked for" stops being a choice.

### For enterprise agent platform teams (expansion)

**The promise:** Substrate-level governance and inference locality for the agentic AI era.

Provenance, audit, supervised state transitions, branch-based experimentation, and inference-locality policies are primitives of the system, not bolted-on features. Configure local-only inference for regulated data, smart-root + cheap-recursive for cost-sensitive workloads. Every fact carries the inference path that produced it.

### For the wider builder population (downstream)

**The promise:** A database that just runs, wherever you are, with everything you need.

The Excel-replacer builds a real tool. The CEO keeps her mental model on her laptop. The indie hacker ships from a Pi. No signup, no connection string, no infrastructure tax. Same binary everywhere.

---

## 9. Vocabulary & Voice

### Words we use repeatedly

- **Complete** (replaces a stack of products)
- **Embedded** (ships with the app, not in someone's cloud)
- **Ships with your app** (concrete deployment model)
- **Same binary** (Pi to Xeon, no version differences)
- **Anywhere** (deployment portability)
- **Lives with your app** (intimate possessive framing)
- **Not a cloud backend** (active anti-positioning)
- **Most apps don't need...** (challenges the BaaS-default assumption)
- **One binary, every data shape** (unique value proposition)
- **Operator** (when distinguishing humans from agents)
- **Substrate** (in internal/strategic contexts, less in homepage copy)
- **Branchable / branch-aware / git-style** (the safety primitive vocabulary)
- **Time-traveled / state-diff / replayable** (the history primitive vocabulary)
- **Machine-actionable** (errors and contracts agents can use)

### Words we avoid

- "AI database" — overloaded, routes to BaaS
- "Postgres for AI" / "AI-native Postgres" — wrong category
- "Backend-as-a-Service" / "BaaS" — Strata is not this
- "Cloud-native" — actively mis-positions
- "Serverless database" — routes to Neon/Aurora territory
- "Database platform" — BaaS positioning
- "AI memory" as Strata's primary identity — that's an application built on Strata, not Strata
- "For agents" alone (saturated)
- "For vibecoders" alone (narrow, BaaS-routing)
- "100 million [anything beyond builders]" — reads as VC math
- Competitor names anywhere in external copy (discuss them in strategy only, never name them in copy)

### Voice principles

- Direct over hedged. *"We built X to do Y"* not *"We're working on solutions that aim to..."*
- Personal stakes when appropriate. Founder-voice "I" lands harder than "we" in essays.
- Concrete verbs over abstract noun phrases. *"I built it"* not *"my foot forward in this effort."*
- Specific over generic. *"Banks, regulated industries, DBAs running mission-critical workloads"* not *"enterprise customers."*
- No deck-language. Avoid *"democratization cycle," "foundation of the era," "next wave of innovation,"* and similar stacked abstractions.
- Humanist warmth in closings. End on what the work does for people, not on a thesis claim.

---

## 10. FAQ

### Is Strata cloud or local?

Local-first, embedded. Strata runs as a single binary, in-process, alongside your application. There's an optional cloud component (StrataHub) for sharing and distributing datasets, but the core database does not require any cloud connection. You can run Strata entirely offline.

### How is Strata different from SQLite, DuckDB, or Postgres?

SQLite is great for embedded single-table workloads — it does one thing (relational SQL) very well. DuckDB is great for analytics on local files. Postgres is the gold standard for cloud-hosted relational workloads. All three were designed for human operators making careful changes against a linear timeline. Strata is built for a different operator (AI agents) and a different shape of work (multi-form state, branchable, replayable, with native AI inference). It runs in the same "embedded" category as SQLite and DuckDB, but with the breadth of capabilities a modern AI-driven application needs.

### Is Strata open source?

[OPEN — pricing and licensing model to be finalized.]

### What languages does Strata support?

[OPEN — language SDK roadmap to be specified. Initial target: Rust, Python, TypeScript, with MCP-based integrations covering most agent runtimes.]

### How do I deploy Strata?

Drop the single binary into your project. No installation server, no daemon, no signup. Strata runs in-process. If you want to share state across machines, you can sync with StrataHub or other replication targets — but that's optional, not required.

### Can I use Strata in production?

[OPEN — V1 production readiness criteria. Currently in active development.]

### Does Strata work offline?

Yes, completely. Strata is embedded, runs locally, includes its own inference engine, and requires no network access. Air-gapped deployments are a first-class use case.

### Can I use my own AI models?

Yes. Strata's inference layer can download and run any Hugging Face model locally via llama.cpp. It also supports remote provider APIs (OpenAI, Anthropic, Google) for tasks where frontier-model quality matters. You can configure inference locality per task.

### What about ACID, consistency, durability?

Strata maintains the durability and consistency guarantees you'd expect from a database, with the additional property that branches and time-travel are first-class. ACID semantics extend across branches; transactions hold across branched state; merges follow well-defined conflict semantics.

### How does branching scale?

Branches are copy-on-write. Creating a branch costs almost nothing. The branch only consumes additional storage as it diverges from its parent. You can have many concurrent branches without storage explosion.

### Can multiple agents work on the same database?

Yes. Each agent can work on its own branch in isolation. Branches can merge under policy or under human review. Strata records actions, dependencies, and conflicts for multi-agent coordination.

### What about security?

Strata supports tenancy primitives (declared in schema, enforced everywhere), permissions and policy boundaries, provenance for every AI-derived fact, and air-gapped deployment for sensitive data. Inference locality is a first-class governance primitive.

### How does Strata compare to vector databases like Pinecone or Weaviate?

Strata includes a vector index as one of its primitives, alongside KV, JSON, events, and graph. The difference is that vectors aren't a separate database — they share the same versioning, branching, and retrieval model as the rest of the data. You don't run a separate vector store; you don't sync embeddings between systems; you don't write glue code.

### How does Strata compare to agent memory products?

Agent memory products (memory layers, recall services) are applications built *on top of* substrate databases. Strata is the substrate they could be built on. The relationship is closer to "Postgres vs Supabase" than to "Postgres vs MySQL."

### What about real-time / sync / replication?

[OPEN — real-time / sync roadmap to be specified.]

### Can I migrate data into Strata from another database?

[OPEN — migration tooling to be specified. Strata is positioned as a from-scratch substrate; importer tools for popular sources are planned but not the headline.]

---

## 11. Anti-Positioning

What Strata is NOT, stated explicitly to prevent mis-positioning by anyone writing about it:

1. **Not a managed cloud database.** Strata is local-first, embedded, ships with apps. Cloud is an optional extension via StrataHub, not the primary deployment model.
2. **Not an "AI-native" version of Postgres or any existing database.** Strata is built from scratch, with primitives designed for AI agents as primary operators. It is not a wrapper, fork, or AI-feature-overlay on an existing engine.
3. **Not a vector database.** Vectors are one of five storage primitives, alongside KV, JSON, events, and graph. Strata's value is the unification, not any individual primitive.
4. **Not an agent memory product.** Agent memory products (Mem0, Letta, MemOS, Supermemory, Zep, Cognee) are *applications* built on substrate databases. Strata is the substrate. Those products are potential customers and partners.
5. **Not a Supabase / Firebase / Neon competitor.** Those are BaaS products serving application builders. Strata is a substrate; BaaS products could be built *on* Strata. The categories are different.
6. **Not "just SQLite with AI features."** SQLite is single-form (relational). Strata is multi-form (KV + JSON + events + vectors + graph) with branching, time-travel, and inference built in.
7. **Not for agents only.** Strata is a complete database that works for any persistence need — including projects where AI agents aren't involved. The agent-native design is what makes it especially good for AI-driven work, but it isn't a limitation.
8. **Not a hosted service.** Strata does not require signup, provisioning, connection strings, or cloud accounts. The core database runs in-process.
9. **Not a closed-source enterprise product.** [OPEN — confirm licensing approach, but Strata is positioned as developer-accessible infrastructure, not enterprise-gated software.]
10. **Not aspirational. Real.** Strata is in active development with concrete V1 release plans. It is not a research project or speculative architecture.

---

## 12. Open Questions

Things this document doesn't yet specify, and that need decisions as V1 ships:

1. **Pricing model.** Open-source core vs. hosted-cloud vs. paid features. To be added when decided.
2. **Licensing.** MIT / Apache / something else. To be added when decided.
3. **V1 launch date and feature scope.** What's definitely in V1 vs. V1.5 vs. V2.
4. **Language SDK roadmap.** Which languages get first-class SDKs in V1, which come later.
5. **Real-time / sync / replication strategy.** Sync to where, with what consistency model.
6. **Migration tooling.** Importers for Postgres, MongoDB, Firebase, etc.
7. **Performance numbers.** Latency, throughput, and scale claims — to be backed by real benchmarks at V1.
8. **The "six tools" specific list in the forced-choice paragraph.** Currently abstract; could be more specific if needed.
9. **StrataHub launch timing.** V1, V1.5, or V2.
10. **Community surface.** Discord, GitHub Discussions, forum — which is canonical, and when does it open.

---

## 13. Cross-References

This document is part of a larger documentation set:

**Strategic / vision:**
- `docs/gtm/strata-vision-v0.2.md` — internal source-of-truth vision document
- `docs/intelligence/self-understanding-architecture.md` — the hidden architectural depth

**Execution:**
- `docs/gtm/strata-gtm-playbook-v0.1.md` — surface-by-surface execution patterns
- `docs/gtm/why-we-started-strata.md` — committed founder essay

**Research / inputs:**
- `docs/gtm/strata-gtm-research.md` — initial GTM landscape research
- `docs/gtm/strata-vibecoder-pain-points.md` — pain-point validation research

**Memory (founder-side context):**
- `memory/project_founder_mission.md` — the upstream mission
- `memory/project_core_thesis_database_rethink.md` — the seed thesis
- `memory/project_substrate_application_separation.md` — substrate vs application principle
- `memory/project_committed_tagline.md` — tagline commitment context
- `memory/project_tesla_bar_operational.md` — the reaction-bar criterion
- `memory/project_disruption_window.md` — the category-mis-routing finding

When updating this document, also update the referenced docs / memories where applicable. When in conflict, this document (the master GTM doc) wins for external messaging; the vision document wins for internal alignment; the memory files win for founder-side principles.
