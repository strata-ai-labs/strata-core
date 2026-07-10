# Strata Vision & Framing

*The canonical source for everything Strata says — externally and to itself. Every pitch, homepage, talk, blog post, and customer conversation traces back to this document. When in doubt, return here.*

---

## Executive Frame

**Strata is rebuilding the database from the ground up for AI agents.**

For fifty years, databases were designed for humans making careful, deliberate writes — and every primitive in the category was shaped by what that operator needed. The operator has changed. AI agents now generate, mutate, retrieve, search, audit, and reorganize data at machine speed, with characteristics no human-era database was designed to support. The first generation of AI work in databases made existing databases easier for humans to use. The next shift makes databases safe and natural for agents to use.

Strata is one branch-aware, versioned, multi-shape, embedded substrate that brings together the persistence patterns AI systems actually need — key-value, documents, events, vectors, and graph relationships, all as views over one branchable state. Our customers are the founders building the next generation of data products for AI. Their products are how Strata reaches the wider builder population. We sit in the substrate category alongside Postgres, DuckDB, and SQLite — not the application category. We are building the database for the agent era.

---

## 1. The Thesis

**AI agents fundamentally need a ground-up rethinking of what databases mean.**

The category "database" was defined in the previous era for a specific operator — humans — and every primitive in the category was shaped by what that operator needed: schemas as contracts, linear write logs, ACID semantics, SQL as the interface language, indexes tuned for human-paced workloads, branches as deploy environments, backups as the recovery story. The operator has changed. The category has to be rebuilt.

This is the load-bearing claim under everything Strata does. Every product decision, every roadmap item, every external word follows from this thesis. The rest of this document articulates the shift, the shape of the rebuild, and what it produces.

---

## 2. The Shift Underway

For decades, databases sat behind software that was expected to be stable, tested, and deliberate. A human designed the schema. A human reviewed the migration. A human wrote the code path. A human decided when production data should change. The pace of change was bounded by code review, deployment pipelines, and the safety of low write volume.

The operator has changed. AI agents now generate the schema, write the seed data, run the migrations, search the documents, store the memory, test the hypotheses, and make changes faster than the human can reconstruct what happened. They are fast where humans were slow, parallel where humans were sequential, willing to fail where humans avoided failure, capable of remembering everything they did, uninterested in human-readable errors. The category's defining primitives were not wrong. They were designed for a different operator.

The first generation of AI work in databases tried to bridge this gap from the human side — natural-language query interfaces, AI-assisted schema design, intelligent error explanations, query autotuning. That work matters; it makes existing databases more accessible. But there is a deeper shift underway. AI does not just change how humans interact with databases. **It changes who the database is for.**

---

## 3. The Category Rewrite

When the operator changes, the category changes. Seven primitives that defined "database" under the human operator are reshaped under the agent operator:

- **Schemas become context.** The agent does not need a contract authored by an operator; it needs the schema as a typed artifact it can read in one prompt, fresh each session.
- **History becomes a tree.** The agent does not need a linear write log it cannot exit; it needs branches as the default unit of any operation, with the canonical path as one of many.
- **Recovery becomes a primitive.** The agent does not need a backup someone hopefully remembered to take; it needs any past state recoverable in milliseconds by construction.
- **Errors become machine-actionable.** The agent does not need a human-readable exception; it needs a structured error it can read and act on without a human in the loop.
- **Interfaces become agent-native.** The agent does not need SQL as a human-readable query language; it needs an interface designed for the operator, with semantics it can use directly.
- **Distribution becomes embedded.** The agent does not need to round-trip across a network to interact with state; it needs the substrate next to it, in-process, with no signup and no connection string.
- **Substrates become multi-shape.** The agent does not need six separate tools stitched together by a human; it needs one substrate where key-value, documents, events, vectors, and graph relationships are views of the same versioned state.

Each of these is a category-defining primitive in the new category. None of them is a feature added to the old category. The hard work is not any single one of them. The hard work is making all seven coherent at the substrate layer — which requires changing the contract between software and state, not adding capabilities to an existing contract.

---

## 4. Who We Are For

In order of immediacy:

**Primary: founders building data products for AI systems.** Memory layers, retrieval engines, agent persistence tools, RAG-as-a-service products, versioned dataset registries, agent infrastructure products, audit substrates for AI agents, embedded analytics for agentic workloads, vertical AI tools with rich persistence needs. Anyone currently stitching together a relational database, a vector index, a key-value store, an event log, and custom glue code to make their product work. These are the customers. They pick up Strata as substrate and ship the domain layer.

**Secondary: application builders integrating AI agents.** Teams shipping production applications where AI agents do meaningful work — generating, mutating, retrieving, summarizing, transforming data. They use Strata directly as the application's data layer and inherit the agent-native primitives.

**Downstream beneficiaries: the wider AI-builder population.** Vibecoders, indie developers, citizen developers building with AI tools. Strata reaches them through the data products built on top. Every product built on Strata serves them better than the same product built on the old substrate. We do not sell to them directly. We make sure the products that serve them are built on a substrate worth building on.

The motion is developer-to-developer, concentrated, partnership-led. Higher ACV, smaller cohort, deeper integration. Adoption propagates through the products built on the substrate, not through mass acquisition at the substrate layer itself.

---

## 5. What Strata Is

Strata is an embedded data substrate for AI agents. It brings together key-value storage, documents, events, vectors, and graph relationships inside one branch-aware, versioned system. These are not separate experiences stitched together. They are different views over one substrate.

The substrate is:

- **Branch-aware by construction.** Every agent run can happen on its own branch. Destructive operations are isolated by default. Merging is explicit.
- **Time-traveled by construction.** Any past state is recoverable in milliseconds. State diff between two points is a primitive, not a feature.
- **Multi-shape.** Key-value for scratch and cache. Documents for application data. Events for the audit trail. Vectors for semantic memory and search. A graph for the relationships between everything. One substrate, every shape.
- **Embedded.** Ships alongside the agent. Single binary. Fast enough to be the agent's scratch pad. Durable enough to be the application's database.
- **MCP-native.** The agent invokes Strata directly. Errors are structured and machine-actionable. The schema is a typed artifact the agent reads in one prompt.

StrataHub extends the substrate with a registry where any developer can clone, branch, and version real datasets the way they fork a code repository. The same branching, versioning, and time-travel primitives apply.

---

## 6. What Strata Delivers

For each party in the chain:

**For data product founders.** Stop doing substrate work that should already be done. The branch-aware KV store is here. The versioned vector index is here. The event log with point-in-time recovery is here. The multi-shape consistency model is here. Build the memory algorithm, the retrieval model, the agent ergonomics, the domain layer that is actually your product. Ship in months what would have taken years on a stitched-together stack.

**For application builders.** Your AI agent can work with data the way a developer works with code. Branch it. Test it. Inspect it. Roll it back. Merge only what is safe. The trust gap between "the agent did something" and "I can live with what it did" closes at the substrate level — without you writing the safety layer yourself.

**For the agent itself, as primary operator.** A substrate designed for how you actually work. Schemas you can read in a single prompt. Errors you can act on without a human translating. State you can branch and experiment with at machine speed. Memory you can search, audit, and version. One place where the things you remember, the things you store, the things you retrieve, and the things you log live in one queryable system.

---

## 7. Positioning & Vocabulary

**Category.** Strata is in the substrate category — the foundational engine that other things are built on. Same shelf as Postgres, DuckDB, SQLite. Not the same shelf as application-layer AI data products. The relationship to those is the relationship Postgres has with the application layers built on it: we enable, we do not compete.

**Posture.** Foundational, not finished. Substrate, not application. Agent-first, not human-replacing. Generous, not confrontational. Decade-scale, not blitz-paced. Built from a thesis, not stitched from features.

**Vocabulary we use:**
- *Substrate* (not "database product")
- *Operator* (when distinguishing humans from agents)
- *Agent* (not "user" when we mean the operator)
- *Branch, merge, diff, time-travel* (the git-for-data primitives)
- *Schema-as-context* (not "schema as contract")
- *Machine-actionable* (not "developer-friendly")
- *Embedded* (not "local" or "edge")
- *Versioned by construction* (not "with version control features")
- *Multi-shape* (not "multi-modal" or "multi-purpose")

**Vocabulary we avoid:**
- "AI database" — overloaded, reads as application-layer
- "AI memory" — a specific application built on substrates like Strata
- "Safer Postgres" / "AI-native Postgres" / "AI-native SQLite" — implies old category done better
- "Undo for AI" — too narrow, makes Strata sound like a safety feature
- "Foolproof" / "AI-safe" — overpromising on bounded systems
- "Replace Postgres" — we don't replace; we are a different category
- Competitor names anywhere in external text — see Principles

---

## 8. The Story We Tell

The first wave of AI infrastructure was about models. The industry learned how to make them capable.

The second wave was about retrieval. The industry learned how to give them context.

The next wave is about trusted state. The industry learns how to give them a substrate they can act on safely.

For fifty years, databases were for humans. They were powerful. The category did what it was designed to do.

The operator has changed. The category is being rebuilt.

There were databases before agents. There will be different databases after them.

---

## 9. What Makes This Hard

There is a reason every major database in production today is decades old. They are extremely difficult to build. To rebuild the category for AI agents, you have to do all of the following at once:

- A storage substrate that supports branching as a first-class primitive, not as a layer on top of a linear log.
- Versioning that works across multiple data shapes — relational, key-value, document, vector, graph — under one consistency model.
- Retrieval that combines structured state, documents, vectors, events, and relationships in a single query plan.
- An execution model that lets agents try changes before committing them, with semantics that hold under concurrent branching.
- An interface agents can use directly, with a machine-actionable error contract designed for autonomous recovery.
- Embedded distribution: a single binary fast enough to be the agent's scratch pad and durable enough to be the application's database.
- A schema artifact format that fits in a prompt for the modal project and stays consistent across the agent's sessions.
- Tenancy and authorization as primitives declared once and enforced everywhere.

None of these is the hard part in isolation. The hard part is making all of them coherent at the substrate layer. That requires changing the contract between software and state — which is what defines a category rewrite versus a feature addition.

---

## 10. What This Becomes

By the early 2030s, branchable, reversible, agent-native data layers feel obvious. Builders expect every AI agent to work on its own branch. They expect data diffs before any change that touches main. They expect rollback as a default operation, not a recovery procedure. They expect memory, retrieval, audit, and application state to live in one coherent system. They expect the database to understand that the user is no longer only a human writing careful SQL.

The next decade of software produces thousands of new data products built specifically for AI systems. The major ones are built on substrates designed for the agent operator. The era of stitching together six tools and writing custom glue is ending. The shape of data infrastructure is changing the way the shape of software changed when source control changed — once developers had branching, merging, rollback, and collaboration built into their workflow, the way software was built changed around it. Data infrastructure is at the same moment.

The bottleneck to AI building is no longer intelligence. The bottleneck is whether the substrate the AI works on can absorb what it does — give the agent room to experiment, give the operator room to recover, give the application room to ship. Strata is that substrate.

---

## 11. How We Work

**Decade-scale pace.** The data market moves in decade-scale cycles. SQLite has been doing what SQLite does for twenty-five years. Postgres took twenty-plus years to become the default. We are building for the next decade, not the next news cycle. Linear-style cohorts, weekly iteration, deliberate pace. No HN-blitz launches, no riding the frontier-model launch window.

**User-truth over competitor surveillance.** We do not position relative to other companies. The job is to build the best possible product for the people we serve. What other companies ship is downstream of user need; we focus upstream. Competitor moves are not strategic input — user contact is.

**Substrate purity.** The substrate stays focused on substrate work. Application-layer products (memory, retrieval, agent infrastructure, audit, search applications) are strictly separate repositories, deployments, and SDKs — and built by our customers, not by us. This is the pattern that kept Linux, Postgres, Git, and Redis defensible.

**Thesis at the head of every artifact.** Every external piece — pitch, blog post, talk, conversation — opens on the thesis: the category is being rebuilt because the operator has changed. Consequences (safety, multi-role, embedded, trust, branching) come second. Each is downstream of the thesis and loses force when treated as the seed.

**Generous framing.** The first generation of AI database work mattered. We are not against it. We are doing the next thing. The contrast is between generations, not between companies. Generous-not-dismissive lets us make the category-rewrite claim cleanly.

---

## 12. What We Refuse

**No native source connectors.** Integration is MCP-only. The moment we ship a Postgres or MongoDB connector, the agent-as-the-primary-integration story becomes mush, and the substrate's posture toward the operator weakens.

**No application-layer products inside Strata core.** Memory products, retrieval products, agent-infrastructure products are downstream of Strata. They are customers, not internal projects. The temptation to ship "Strata Memory" or "Strata Retrieval" as packaged products dilutes the substrate.

**No safety / undo as the headline pitch.** Trust is a downstream consequence of getting the substrate right. Leading with safety positions Strata as a thin layer of safety features over an existing database — wrong category. The headline is the category rewrite.

**No "AI-native Postgres" / "AI-native SQLite" framing.** That implies we are the old category done better. We are a different category, built up from the operator change. Choosing the old category's language costs us the thesis.

**No mass acquisition push at the substrate layer.** The motion is concentrated and developer-to-developer. Substrate adoption propagates through the products built on it, not through Show-HN volume.

**No competitor names in external text.** Discussing other products in strategy conversations is fine; naming them in pitches, posts, or marketing copy is not. Position by category and by truth, not by relative comparison.

---

*This document is the source-of-truth for Strata's external voice. Edits to it propagate downstream into every derivative artifact — homepage copy, launch posts, sales narratives, investor materials, technical blog posts, talks, customer conversations. When a downstream artifact contradicts this document, the document wins until amended here first.*
