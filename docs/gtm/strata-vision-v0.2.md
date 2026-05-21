# Strata Vision

## The native state substrate for AI agents

**Version:** 0.2
**Status:** Internal source-of-truth draft
**Audience:** Product, engineering, research, design, developer relations, and future external narrative development

---

## 0. How to use this document

This document defines the internal vision for Strata.

It is not website copy, launch copy, fundraising copy, or a short manifesto. Those materials should be derived from the canonical wording in Section 18. This document is meant to align the team on the deeper product thesis, target users, category definition, design principles, and evaluation criteria.

The core purpose is to answer five questions:

1. **Why does Strata exist?**
2. **What category is Strata creating?**
3. **Who is Strata for?**
4. **Which primitives are essential?**
5. **How should we evaluate whether Strata is working?**

The short version:

> **Strata is the native state substrate for AI agents: a branchable, replayable, multi-form data layer for memory, retrieval, coordination, and safe change.**

---

## 1. Executive summary

Strata is a ground-up rethinking of the database for a world where AI agents are primary users of data infrastructure.

Traditional databases were built for applications written by humans. In that model, humans design schemas, review migrations, write stable application code, approve dangerous changes, interpret errors, and recover from mistakes through operational process.

AI agents create a different operating environment. They create schemas, mutate state, retrieve memory, ingest documents, test hypotheses, coordinate tools, produce application logic, and operate at machine speed. They do not simply query data. They work on top of data.

The first generation of AI database work has made databases easier for humans to use through natural language query, AI assistants, query explanation, automated tuning, semantic search, and operational copilots. That work is useful, but it addresses the human interface to databases.

Strata addresses a different question:

> **What should a database become when the primary user is an AI agent?**

The answer is not just a better interface. Agents need a different data contract. They need state that is branchable, replayable, retrievable, inspectable, recoverable, and safe to mutate under supervision.

Strata is designed as an embedded, branch-native, multi-form database for stateful AI systems. It unifies memory, retrieval, events, traces, application state, and safe mutation under one versioned substrate.

The long-term vision is:

> **Strata becomes the default state substrate for AI agents.**

---

## 2. Core thesis

The AI era does not only require databases with AI features. It requires databases whose core abstractions are designed around agents.

The distinction is important.

An AI feature improves what a human can do with a database. An agent-native database changes what an AI system can safely do with state.

Traditional databases assume:

| Traditional assumption                           | Implication                                                              |
| ------------------------------------------------ | ------------------------------------------------------------------------ |
| Human-written applications are the primary users | Access is mediated through stable code paths                             |
| State is mostly linear                           | Changes are committed into one main timeline                             |
| Writes are deliberate                            | Human process slows down risky operations                                |
| Schemas are human-designed                       | Schema evolution is planned and reviewed                                 |
| Errors are human-interpreted                     | Recovery depends on human diagnosis                                      |
| Retrieval is workload-specific                   | Structured, semantic, temporal, and relational access are often separate |
| Recovery is external                             | Backup, restore, rollback scripts, or manual repair                      |
| Experimentation happens outside the database     | The database mostly stores accepted state                                |

AI agents create a different set of requirements:

| Agent-native assumption                     | Implication                                                                         |
| ------------------------------------------- | ----------------------------------------------------------------------------------- |
| Agents are active users of the data layer   | The database must expose agent-usable contracts                                     |
| State is exploratory                        | Branching and provisional change become native                                      |
| Writes are high-frequency                   | Isolation and rollback must be cheap                                                |
| Schemas are co-created by humans and agents | Schema context must be compact and machine-readable                                 |
| Errors must support autonomous recovery     | Error contracts must be machine-actionable                                          |
| Retrieval spans many forms of state         | Memory, documents, events, traces, vectors, and relationships need a coherent model |
| Recovery must be native                     | Rollback, replay, time travel, and diff become core primitives                      |
| Experimentation is part of the workflow     | Agents need to test, compare, and merge state changes safely                        |

The central Strata thesis:

> **AI agents need a state substrate designed around safe, replayable, multi-step work.**

---

## 3. One-sentence vision and shorthand

### One-sentence vision

**Strata is the native state substrate for AI agents: a branchable, replayable, multi-form data layer for memory, retrieval, coordination, and safe change.**

### Useful shorthand

**Git for data, built for AI agents** is useful shorthand for Strata's branching and safety model.

It explains one important part of the product:

* Branches
* Diffs
* Rollback
* Merge
* Review
* Promotion from experimental state to trusted state

But it is not the full company positioning.

Strata is broader than version control for data. It is a substrate for stateful agent systems. Branching is the most intuitive way to explain safe agentic change, but the full system also includes memory, retrieval, replay, audit, schema contracts, machine-actionable errors, and agent-native operation.

Internal rule:

> **Use "Git for data" to explain the branching primitive. Do not use it to define the entire company.**

---

## 4. The primary-user shift

The most important shift is not that databases now have AI features. The most important shift is that the primary user of the database is changing.

For decades, databases primarily served applications written by humans. A human designed the schema. A human wrote the application logic. A human tested the migration. A human approved production changes. A human interpreted errors and decided how to recover.

In agentic systems, the agent becomes an active participant in the data layer.

Agents may:

* Create new application state
* Generate schemas
* Write and modify records
* Search documents
* Maintain memory
* Retrieve semantic context
* Traverse relationships
* Call tools
* Record traces
* Debug workflows
* Propose fixes
* Run migrations
* Recover from failures
* Coordinate with other agents

This changes the database from a passive store behind an application into a workspace where agents reason, act, and recover.

That is the foundational product shift.

> **AI-assisted databases improve the human interface. Agent-native databases change the state contract.**

Strata is focused on the second.

---

## 5. Problem statement

A database is not just a storage engine. It is a contract between software and state.

Traditional databases define this contract through durable writes, queries, schemas, indexes, transactions, constraints, consistency, and recovery. That contract has worked extremely well for human-written applications.

AI agents introduce requirements that are not fully captured by the traditional contract.

### 5.1 Agents are exploratory

Agents try things. They generate intermediate state. They create hypotheses. They revise. They may take multiple paths before selecting one.

A linear state model forces exploratory work into final state. That makes mistakes expensive.

Strata treats exploration as a first-class database operation.

### 5.2 Agents are high-velocity

Agents can produce many database-relevant changes in a short time: schema changes, seed data, generated records, migration attempts, retrieval experiments, and tool outputs.

The higher the velocity of change, the more important it becomes for the data layer to provide cheap isolation, rollback, and review.

### 5.3 Agents are probabilistic

Even strong models can misunderstand a task, misuse a tool, hallucinate a field, choose the wrong operation, or overfit to incomplete context.

The data layer should not assume that every agent action is correct.

Strata treats agent changes as provisional until accepted.

### 5.4 Agents need multi-form memory

Agent state is not only relational data. It includes scratch state, application data, documents, events, traces, embeddings, relationships, decisions, tool calls, and prior actions.

A useful agent substrate must support many forms of state under one coherent model.

### 5.5 Agents need machine-readable contracts

Human-readable schemas and error messages are not enough when agents are expected to operate and recover autonomously.

Agents need compact schemas, structured errors, explicit recovery options, and tool-native interfaces.

### 5.6 Agents need replay and audit

As agents perform longer-running workflows, users need to know what happened, why it happened, what state the agent saw, what the agent changed, and how to recover.

The database must make agent behavior inspectable.

---

## 6. Category definition: agent-native database

An **agent-native database** is a data system whose core contract assumes the primary user is an AI agent.

The agent is not merely sending queries. It is reading, writing, retrieving, remembering, experimenting, coordinating, recovering, and acting on state.

An agent-native database must support five load-bearing capabilities.

---

### 6.1 Branchable state

Agents need to try changes without corrupting trusted state.

Core capabilities:

* Branch
* Diff
* Rollback
* Merge
* Conflict handling
* Safe promotion from experimental state to accepted state

Branching changes the meaning of an agent action.

Without branching, an agent action is a direct mutation.

With branching, an agent action becomes a proposal.

---

### 6.2 Replayable history

Agents and humans need to understand what happened, reproduce prior context, and recover from mistakes.

Core capabilities:

* Time travel
* Event history
* Traceability
* Deterministic replay
* Provenance
* State snapshots

Replay is essential because agent work is multi-step. The system must preserve not only final state, but the path that produced it.

---

### 6.3 Unified memory and retrieval

Agents need more than one type of data. They need a coherent way to work across scratch state, application data, documents, events, traces, vectors, and relationships.

Core capabilities:

* Structured lookup
* Semantic retrieval
* Event and trace search
* Relationship traversal
* Retrieval from known snapshots
* Queryable memory
* Inspectable context

The goal is not generic feature breadth. The goal is coherence across the forms of state agents actually use.

---

### 6.4 Agent-readable contracts

Agents need schemas, errors, and operations designed for machines, not only for humans.

Core capabilities:

* Compact schema artifacts
* Machine-actionable errors
* Tool-native operations
* Explicit recovery guidance
* Agent-readable constraints
* Agent-readable permissions
* Safe default workflows

A database for agents should help the agent choose valid operations, recover from invalid operations, and escalate when necessary.

---

### 6.5 Supervised state transition

Agents need to act quickly, but important changes need review, policy boundaries, and audit.

Core capabilities:

* Approval flows
* Permission boundaries
* Action logs
* Reviewable diffs
* Merge gates
* Human-supervised promotion into trusted state

The goal is not full autonomy by default. The goal is safe autonomy under explicit boundaries.

---

## 7. Product vision

Strata is an embedded, branch-native, multi-form database for stateful AI systems.

It provides one substrate for:

| Primitive           | Agent use case                                      |
| ------------------- | --------------------------------------------------- |
| Key-value           | Scratch state, temporary reasoning artifacts, cache |
| Documents           | Application state, user data, structured objects    |
| Events              | Audit trail, action history, decision log           |
| Traces              | Agent runs, tool calls, execution history           |
| Vectors             | Semantic memory, retrieval, similarity search       |
| Graph relationships | Entities, dependencies, provenance, workflows       |
| Branches            | Safe experimentation, rollback, merge, time travel  |

These are not separate product experiences. They are different views over one versioned state substrate.

The goal is to let an agent use one database for the full lifecycle of stateful work:

1. Understand current state.
2. Retrieve relevant context.
3. Create a branch.
4. Try a change.
5. Inspect the result.
6. Compare against the base state.
7. Roll back, revise, or merge.
8. Preserve an audit trail.
9. Continue operating from trusted state.

Strata is not only where final state lives. It is where agents explore, remember, retrieve, test, branch, merge, recover, and explain.

---

## 8. Primitive hierarchy

There is an important hierarchy inside the Strata vision.

### 8.1 Versioned state is the foundation

Strata must maintain state in a way that supports history, replay, rollback, and safe mutation.

Without versioned state, branching is superficial. Replay is incomplete. Audit is fragile. Recovery depends on external process.

Versioned state is the foundation on which the rest of Strata is built.

### 8.2 Branching is the first visible safety primitive

Branching is the most intuitive expression of Strata's safety model.

It lets an agent:

* Try a schema change
* Modify records
* Ingest uncertain data
* Test a retrieval strategy
* Simulate a workflow
* Compare outcomes
* Discard bad work
* Promote good work

Branching makes agentic change safe because it turns direct mutation into provisional work.

### 8.3 Multi-form state is the structural advantage

Agents do not work only with rows. They work with scratch state, documents, memories, events, traces, embeddings, relationships, and application data.

Strata's advantage is not that it has many features. Its advantage is that these forms of state share one model of versioning, branching, retrieval, audit, and recovery.

### 8.4 Agent-native interfaces complete the contract

Branching and multi-form state are not enough if agents cannot use them directly.

Strata must expose schemas, errors, tools, diffs, retrieval operations, and merge workflows in forms agents can understand and act on.

### 8.5 Human supervision creates trust

The point of agent-native design is not to remove the human from every workflow.

The point is to let agents operate at machine speed while preserving human control over important state transitions.

The hierarchy can be summarized as:

> **Versioned state is the foundation. Branching is the safety wedge. Multi-form state is the structural advantage. Agent-native interfaces are the product contract. Human supervision is the trust model.**

---

## 9. Why multi-form state matters

Agents do not experience tasks as database categories.

They do not think: "Now I need a document database, then a vector database, then an event log, then a graph database."

They experience a task as a chain of stateful work.

For example, an agent building or operating an application may need to:

1. Store user records.
2. Cache intermediate reasoning state.
3. Search project documents.
4. Retrieve prior decisions.
5. Track migrations as events.
6. Inspect relationships between entities.
7. Understand tool-call history.
8. Roll back a failed change.
9. Explain what changed.
10. Continue from the corrected state.

If each part of this workflow lives in a different system, the builder has to create the coherence manually.

Strata's multi-form design is meant to provide that coherence natively.

The important question is not:

> Can Strata support many data types?

The important question is:

> Can an agent work across many forms of state through one consistent model of versioning, retrieval, audit, and recovery?

That is why multi-form state is central to Strata.

---

## 10. Why embedded matters

Strata is designed to be embedded first.

Embedded does not mean small in ambition. It means the database can live close to the agent, the developer, and the application. This matters because many agentic workflows begin in local development loops, coding tools, notebooks, agent runtimes, and small stateful systems before they become managed cloud workflows.

Embedded deployment gives Strata several advantages:

| Advantage                   | Rationale                                                    |
| --------------------------- | ------------------------------------------------------------ |
| Low friction                | No signup, provisioning, or external infrastructure required |
| Low latency                 | No network round trip in the agent loop                      |
| Local-first operation       | Agents can build, test, and revise locally                   |
| Simple distribution         | Strata can ship with the agent or application                |
| Strong developer experience | The database is immediately available                        |
| Cheap branching             | Branches can be created frequently                           |
| Offline capability          | Agent workflows can continue without cloud dependency        |

The initial adoption path should feel closer to SQLite, DuckDB, or Git than to a managed enterprise database.

However, embedded is an adoption and architecture strategy, not the entire category definition. The category definition is agent-native state. Embedded is the best initial way to make that state substrate usable.

---

## 11. Target users and ICP hierarchy

Strata is for people building stateful AI systems.

The primary user of the system is the agent. The primary customer is the builder responsible for giving that agent durable memory, retrieval, replay, coordination, and safe access to state.

The target hierarchy is explicit.

---

### 11.1 Primary ICP: agent-infrastructure and data-product builders

These are the most important early users.

They include:

* Founders building AI-native data products
* Builders of agent memory systems
* Builders of retrieval and RAG infrastructure
* Agent framework developers
* Internal AI platform teams
* Teams building long-running, tool-using agents
* Teams building evaluation, simulation, replay, or agent observability systems
* Teams building stateful AI workflows over documents, tools, and application data

Their core problem is not simply:

> "I need a database."

Their real problem is:

> **I need a reliable state substrate for agents that remember, retrieve, branch, replay, coordinate, and recover.**

Core needs:

| Need                            | Strata capability                              |
| ------------------------------- | ---------------------------------------------- |
| Persistent agent memory         | Versioned state, documents, events, vectors    |
| Retrieval over evolving context | Snapshot-aware retrieval                       |
| Replay of agent runs            | Temporal history and traces                    |
| Safe experimentation            | Branches, diffs, rollback, merge               |
| Tool coordination               | Event logs and state transitions               |
| Debugging agent behavior        | Provenance, traceability, replay               |
| Human supervision               | Reviewable changes and merge gates             |
| Agent-native operation          | Schemas, errors, and tools designed for agents |

This is the durable substrate ICP.

---

### 11.2 Secondary wedge: AI-assisted application builders

AI-assisted application builders are an important wedge because their pain is concrete and easy to explain.

They use coding agents to create applications, schemas, seed data, migrations, and local workflows. They quickly encounter the trust gap:

> "The AI changed something. Can I understand it, undo it, or trust it?"

Strata helps by giving every agent run a branch, making schema and data changes inspectable, and allowing good changes to be merged while bad changes are discarded.

Core needs:

| Need                   | Strata capability           |
| ---------------------- | --------------------------- |
| Safe local development | Embedded runtime            |
| Agent-created schema   | Branch-based schema changes |
| Reversible migrations  | Rollback and time travel    |
| Inspectable changes    | Diffs                       |
| Application state      | Documents and key-value     |
| Search and memory      | Vectors, documents, events  |
| Build history          | Event trail and replay      |

This persona is useful for adoption and storytelling, but it should not collapse the vision into "a database for AI-assisted app building."

The broader vision is state infrastructure for agents.

---

### 11.3 Expansion ICP: enterprise agent platform teams

As organizations deploy agents into real workflows, they will need stateful infrastructure for memory, retrieval, audit, supervision, and recovery.

These teams may build agents for:

* Operations
* Support
* Data cleanup
* Internal copilots
* Workflow automation
* Incident response
* Research and analysis
* Customer-facing automation
* Developer productivity
* Data product operations

Their needs overlap with the primary ICP, but with more emphasis on governance, security, policy, audit, and integration with existing systems.

---

## 12. Core workflows

Strata should be evaluated through workflows, not only features.

### 12.1 Safe agentic change

1. Agent starts a task.
2. Strata creates a branch.
3. Agent modifies state.
4. Strata records events and traces.
5. Agent inspects results.
6. Human or policy engine reviews the diff.
7. Change is merged, revised, or discarded.

This is the core trust workflow.

---

### 12.2 Agent memory and retrieval

1. Agent stores memories, decisions, documents, events, and tool outputs.
2. Strata indexes the relevant state across structured and semantic surfaces.
3. Agent retrieves context from a known snapshot.
4. Retrieval results are tied to the state that produced them.
5. Future agents can replay or inspect the retrieval context.

This is the core memory workflow.

---

### 12.3 Data ingestion under uncertainty

1. Agent ingests messy external data.
2. Data lands on an isolated branch.
3. Agent cleans, normalizes, links, and annotates the data.
4. Strata preserves provenance and transformation history.
5. Validated records are promoted.
6. Invalid or uncertain records remain isolated.

This is the core data-product workflow.

---

### 12.4 Replay and debugging

1. Agent run fails or produces suspicious output.
2. Developer inspects the state snapshot used by the agent.
3. Developer reviews events, traces, retrieval results, and mutations.
4. Developer replays the run or compares against another branch.
5. Developer fixes the issue or adjusts the agent workflow.

This is the core debugging workflow.

---

### 12.5 Multi-agent coordination

1. Multiple agents operate over related state.
2. Each agent works on isolated branches or scoped state regions.
3. Strata records actions, dependencies, and conflicts.
4. Agents or humans reconcile changes.
5. Accepted work is merged into shared state.

This is the long-term coordination workflow.

---

## 13. Agent interface requirements

For Strata to be genuinely agent-native, it cannot expose only human-centric database interfaces.

Agents need interfaces that make state, schema, errors, and recovery actions explicit.

### 13.1 Agent-readable schema

The schema should be available as a compact artifact an agent can consume efficiently.

It should include:

| Schema element                 | Purpose                          |
| ------------------------------ | -------------------------------- |
| Entities and fields            | Understand data shape            |
| Types and constraints          | Avoid invalid writes             |
| Relationships                  | Understand dependencies          |
| Indexes and retrieval surfaces | Choose access paths              |
| Branch metadata                | Understand current state context |
| Permissions and boundaries     | Avoid unauthorized actions       |
| Examples                       | Improve agent reliability        |

The agent should not need repeated introspection across many calls to understand the relevant data model.

---

### 13.2 Machine-actionable errors

Traditional database errors are usually written for humans. Agents need structured errors that support correction.

A machine-actionable error should include:

| Field              | Purpose                                                 |
| ------------------ | ------------------------------------------------------- |
| Error type         | Constraint violation, type mismatch, missing field      |
| Relevant object    | Table, document collection, branch, index               |
| Failed operation   | Insert, update, merge, query                            |
| Cause              | Why the operation failed                                |
| Suggested recovery | Valid next actions                                      |
| Safety level       | Whether retry, rollback, or human review is recommended |

The goal is not merely to explain failure. The goal is to help the agent recover safely.

---

### 13.3 Tool-native access

Strata should support agent-native access patterns, including MCP-style interfaces, so agents can inspect state, query data, create branches, run changes, view diffs, and request merges through structured tools.

Core operations should be explicit:

| Tool operation    | Purpose                               |
| ----------------- | ------------------------------------- |
| `create_branch`   | Isolate an agent run                  |
| `query_state`     | Read structured state                 |
| `semantic_search` | Retrieve relevant memory or documents |
| `inspect_schema`  | Understand available data             |
| `propose_change`  | Stage a mutation                      |
| `diff_branch`     | Compare against base                  |
| `rollback_branch` | Discard unsafe work                   |
| `merge_branch`    | Promote accepted changes              |
| `explain_change`  | Summarize what happened               |
| `replay_run`      | Reconstruct prior agent behavior      |

The interface should make safe behavior the easiest behavior.

---

## 14. Trust as the core product problem

The fundamental product problem is not storage alone. It is trust.

As AI systems become more capable, users will ask:

| Trust question                             | Strata answer                                                       |
| ------------------------------------------ | ------------------------------------------------------------------- |
| What did the agent change?                 | Branch diffs and audit logs                                         |
| Can I undo it?                             | Rollback and time travel                                            |
| Can I test it first?                       | Branch-native execution                                             |
| Can I recover old state?                   | Versioned storage                                                   |
| Can I understand why it happened?          | Events, traces, and provenance                                      |
| Can the agent avoid repeating the mistake? | Structured errors and memory                                        |
| Can I approve before commit?               | Human-supervised merge                                              |
| Can the agent retrieve the right context?  | Unified retrieval over state, documents, vectors, and relationships |
| Can I replay the run?                      | Snapshot-aware replay                                               |

The core trust gap is between:

> "The agent did something."

and:

> "I understand, accept, and can rely on what the agent did."

Strata exists to close that gap at the data layer.

---

## 15. Design principles

### Principle 1: The agent is a first-class database user

Strata should be designed as if the agent is not merely a source of queries, but an active user of the database.

### Principle 2: Change should be provisional by default

Agent changes should be easy to isolate, inspect, accept, or discard.

### Principle 3: Trusted state and experimental state must be separated

The database should distinguish between what is accepted and what is being explored.

### Principle 4: Retrieval should be reproducible

An agent should be able to reproduce the context that led to a decision.

### Principle 5: Multi-form support should serve coherence

The purpose of key-value, documents, events, traces, vectors, and graphs is to support agent workflows under one consistent contract.

### Principle 6: Errors should enable recovery

An error should help an agent take the next safe action.

### Principle 7: The system should be useful before it is cloud-managed

The embedded product should deliver value immediately in local and agentic build workflows.

### Principle 8: Human supervision should be built into the workflow

Strata should allow agents to operate quickly while preserving human review over important state transitions.

### Principle 9: Auditability is not optional

Agentic systems need durable records of what happened, when, why, and on which branch.

### Principle 10: Safe behavior should be the default path

The default workflow should encourage branch, inspect, diff, replay, and merge.

---

## 16. Strategic wedge

Strata's strategic wedge is:

> **Make stateful agent systems safer to build, debug, replay, and trust.**

The most concrete initial demonstration may come from AI-assisted application development, where the pain is immediately visible. A coding agent creates a schema, changes data, runs a migration, or modifies application state. Strata gives that work a branch, a diff, a rollback path, and a merge decision.

But the deeper ICP is broader: builders of stateful agent infrastructure.

The wedge should be understood at two levels.

---

### 16.1 Narrative wedge: safe agentic change

This is the story people understand quickly.

Agents are increasingly allowed to change things. Change creates risk. Strata makes agentic change branchable, inspectable, reversible, and mergeable.

This is where "Git for data" is useful.

---

### 16.2 Product wedge: state substrate for agent builders

This is the product direction.

Builders of agent systems need a substrate for:

* Memory
* Retrieval
* Replay
* Branching
* Audit
* State coordination
* Tool execution history
* Human-supervised promotion
* Debugging long-running agent behavior

This is where Strata becomes more than a local database for AI-built apps.

---

### 16.3 Initial product experience

A strong V1 experience could be:

1. Install Strata locally.
2. Connect it to an agent or coding tool.
3. Let the agent create or modify state.
4. Every run happens on a branch.
5. The builder inspects the diff.
6. Good changes are merged.
7. Bad changes are rolled back.
8. The run can be replayed and understood.

The roadmap should be evaluated against the broader substrate thesis:

> **Does this make stateful agent systems easier to build, trust, debug, replay, and operate?**

If yes, it belongs in Strata.

If no, it may be a useful database feature, but it is not core.

---

## 17. Evaluation framework

Strata should be evaluated not only as a database, but as an agent substrate.

Traditional database metrics remain important:

| Traditional metric | Why it matters                             |
| ------------------ | ------------------------------------------ |
| Read latency       | Required for agent loops                   |
| Write latency      | Required for scratch and application state |
| Durability         | Required for serious use                   |
| Storage efficiency | Required for scale                         |
| Index performance  | Required for retrieval                     |
| Concurrency        | Required for multi-agent workflows         |

But Strata also needs agent-native metrics:

| Agent-native metric       | Question answered                                         |
| ------------------------- | --------------------------------------------------------- |
| Branch creation latency   | Can every agent run get a branch cheaply?                 |
| Rollback latency          | Can mistakes be undone quickly?                           |
| Merge correctness         | Can accepted changes be promoted safely?                  |
| Diff interpretability     | Can humans and agents understand what changed?            |
| Retrieval reproducibility | Can context be reproduced from a prior state?             |
| Agent recovery rate       | Can agents recover from errors autonomously?              |
| Schema comprehension      | Can agents operate without repeated introspection?        |
| Replay fidelity           | Can a prior run be reconstructed accurately?              |
| Trust improvement         | Do users allow agents to perform more serious operations? |

The most important long-term benchmark may be:

> **Can an AI agent build, modify, and operate a real stateful system with less human intervention and lower state-corruption risk when using Strata?**

---

## 18. Proposed canonical wording

This section should propagate into external narratives.

### One sentence

**Strata is the native state substrate for AI agents: a branchable, replayable data layer for memory, retrieval, coordination, and safe change.**

### Two sentences

**Strata is built for teams creating stateful AI agents, not just applications with AI features. It gives agents a branchable, replayable, multi-form data layer so they can safely remember, retrieve, experiment, recover, and act.**

### Short product description

**Strata is an embedded, branch-native database for stateful AI systems. It unifies memory, retrieval, events, traces, application state, and safe mutation under one versioned substrate designed for agents.**

### Category definition

**An agent-native database is a data system whose core contract assumes the primary user is an AI agent: state is branchable, history is replayable, memory is retrievable, errors are machine-actionable, and important changes can be supervised before becoming trusted state.**

### Strategic claim

**The next wave of AI infrastructure is trusted state. Models provide intelligence. Tools provide action. Strata provides the state substrate that makes agentic action safe, replayable, and useful over time.**

### Shorthand

**Git for data, built for AI agents.**

Use this only as shorthand for the branching and safety model.

### Strong internal thesis

**Strata is not just making databases easier to use with AI. It is rethinking the database around the agent as the primary user.**

---

## 19. Non-goals

A clear vision also requires clear non-goals.

### 19.1 Strata is not primarily an AI assistant for database administrators

Strata may eventually expose assistant-like experiences, but the core product is not a chatbot over database metadata. The core product is a new state contract for agents.

### 19.2 Strata is not initially a replacement for every production database

The initial goal is not to displace established production databases in their strongest environments. The initial goal is to create a new category around agent-native state.

### 19.3 Strata is not only a vector database

Vectors are important, but agent memory requires more than embeddings. It requires structured state, events, provenance, relationships, traces, and safe mutation.

### 19.4 Strata is not only a local developer toy

Although the embedded local workflow is important for adoption, the architecture should support serious state management, durability, auditability, and eventual production use.

### 19.5 Strata should not optimize for feature breadth at the cost of coherence

Multi-form support is valuable only if it strengthens the unified agent workflow. A shallow combination of unrelated features is not the goal.

### 19.6 Strata is not just Git for data

Branching is the safety wedge. The broader vision is a full state substrate for agents.

---

## 20. Risks and open questions

### 20.1 Risk: The market may understand "Git for data" but miss the broader substrate thesis

Mitigation: Use "Git for data" as the entry point, then quickly expand to memory, retrieval, replay, audit, and agent-native operation.

### 20.2 Risk: Multi-form scope may become too broad

Mitigation: Prioritize forms of state that directly support agent workflows. Avoid implementing features that do not strengthen the agent-native contract.

### 20.3 Risk: Branching may be technically expensive

Mitigation: Optimize branch creation, copy-on-write behavior, compaction, and merge semantics early. Branching must feel cheap enough to be default.

### 20.4 Risk: Developers may initially use Strata only as a local development tool

Mitigation: Make the local experience excellent, but design durability, audit, replay, and production-readiness into the architecture from the beginning.

### 20.5 Risk: Existing infrastructure may add partial agent-native features

Mitigation: Focus on the integrated contract. Strata's differentiation is not any single feature, but the coherence of branching, replay, multi-form state, retrieval, and agent-native recovery.

### 20.6 Risk: Agents may not yet be trusted with enough database operations

Mitigation: Start with supervised workflows. The point is not full autonomy on day one. The point is making increasingly serious agent operations safe enough to permit.

### 20.7 Risk: Merge semantics across data forms may be complex

Mitigation: Define clear merge strategies by primitive, starting with conservative defaults and explicit conflict handling.

### 20.8 Risk: "Database" may be too narrow a word for the full vision

Mitigation: Use "state substrate" internally and in strategic contexts. Use "database" when communicating with developers who need a concrete product category.

---

## 21. Relationship to StrataHub

StrataHub can extend the core Strata vision by making Strata databases cloneable, branchable, and shareable across developers and agents.

The analogy is useful:

| Code world     | Strata world                       |
| -------------- | ---------------------------------- |
| Git repository | Strata database                    |
| Branch         | Data branch                        |
| Commit         | State checkpoint                   |
| Diff           | Data and schema diff               |
| Merge          | Promote validated changes          |
| GitHub         | StrataHub                          |
| Fork project   | Clone dataset or application state |
| Pull request   | Proposed state change              |

StrataHub could allow developers to:

1. Publish Strata databases.
2. Clone real datasets.
3. Branch application state.
4. Share agent-built applications with data included.
5. Review proposed state changes.
6. Build reproducible agent benchmarks.
7. Host curated datasets for AI builders.

StrataHub should be treated as a platform extension, not the initial core. The core must first prove that Strata is a valuable embedded state substrate for agents.

---

## 22. Research and product hypotheses

Strata should be developed around testable hypotheses.

### Hypothesis 1: Agents need branch-native state workflows

AI agents will make safer and more useful changes when database operations are isolated in branches by default.

Possible evaluation:

| Metric                  | Measurement                                                       |
| ----------------------- | ----------------------------------------------------------------- |
| Recovery rate           | Percentage of failed agent runs recoverable without manual repair |
| Time to rollback        | Latency to restore prior state                                    |
| Human review efficiency | Time to inspect and approve agent changes                         |
| Error containment       | Percentage of bad changes prevented from reaching trusted state   |

---

### Hypothesis 2: Unified state improves agent reliability

Agents will perform better when scratch, memory, documents, events, traces, vectors, and relationships are available through a coherent interface.

Possible evaluation:

| Metric                     | Measurement                                   |
| -------------------------- | --------------------------------------------- |
| Task completion rate       | Agent succeeds in multi-step workflow         |
| Context retrieval accuracy | Relevant context retrieved from mixed data    |
| API complexity             | Number of tools or systems required           |
| Failure attribution        | Ability to diagnose where the workflow failed |

---

### Hypothesis 3: Agent-readable schemas reduce invalid operations

Agents will produce fewer invalid operations when schemas are exposed in compact, structured, task-relevant form.

Possible evaluation:

| Metric                | Measurement                                           |
| --------------------- | ----------------------------------------------------- |
| Invalid write rate    | Type, constraint, and missing-field errors            |
| Query correction rate | Agent recovers after failed query                     |
| Schema comprehension  | Accuracy of generated operations                      |
| Token efficiency      | Schema context size required for successful operation |

---

### Hypothesis 4: Machine-actionable errors improve autonomous recovery

Structured errors with recovery guidance will help agents correct failures without human intervention.

Possible evaluation:

| Metric                   | Measurement                                       |
| ------------------------ | ------------------------------------------------- |
| Autonomous recovery rate | Agent fixes failed operation without human help   |
| Retry efficiency         | Number of attempts before success                 |
| Unsafe retry rate        | Agent avoids repeating harmful operations         |
| Escalation accuracy      | Agent correctly asks for human review when needed |

---

### Hypothesis 5: Replay improves trust and debuggability

Users will trust agentic systems more when agent runs can be replayed, inspected, and tied to exact state snapshots.

Possible evaluation:

| Metric              | Measurement                                       |
| ------------------- | ------------------------------------------------- |
| Replay success rate | Percentage of runs reproducible from stored state |
| Debugging time      | Time to identify failure cause                    |
| Trust rating        | User willingness to allow future agent action     |
| Correction accuracy | Ability to fix failed workflows after replay      |

---

## 23. Internal positioning

Strata should be positioned internally as:

> **The native state substrate for AI agents.**

Supporting description:

> **Strata gives agent builders a branchable, replayable, multi-form data layer for memory, retrieval, coordination, audit, and safe state change.**

Positioning boundaries:

| Avoid reducing Strata to               | Better framing                                                |
| -------------------------------------- | ------------------------------------------------------------- |
| A database with an AI assistant        | A data layer designed for agents as first-class users         |
| A vector database                      | Unified memory and retrieval across many forms of agent state |
| A local embedded database              | Embedded first, but architected as an agent-native substrate  |
| Git for data only                      | Branching is the safety wedge, not the full product           |
| A general-purpose database replacement | A new substrate for stateful AI systems                       |
| A vibe-coding tool                     | A trust and replay layer for agentic state                    |

The most important internal distinction:

> **AI-assisted databases improve the human interface. Agent-native databases change the state contract.**

Strata is focused on the second.

---

## 24. The document spine

The entire vision should ladder up like this:

1. **AI agents are becoming stateful actors.**
   They do not just query data. They remember, retrieve, mutate, coordinate, and act.

2. **Stateful agents need a different data contract.**
   Linear state, human-readable errors, fragmented retrieval, and external recovery are insufficient.

3. **The core missing capability is trusted state.**
   Agents need state they can branch, replay, inspect, recover, and promote.

4. **Strata is the native state substrate for that world.**
   It provides branchable, replayable, multi-form state for AI agents.

5. **The initial wedge is safe agentic change.**
   AI-assisted app builders make the pain obvious, but the deeper ICP is agent-infrastructure and data-product builders.

6. **The long-term category is agent-native databases.**
   These are not merely databases with AI interfaces. They are databases whose state model, retrieval model, error model, and recovery model are built around agents.

---

## 25. Conclusion

Strata exists because the role of the database is changing.

In the pre-agent era, the database primarily served applications written and operated by humans. In the agent era, the database increasingly serves autonomous or semi-autonomous systems that create, modify, retrieve, reason over, and act on state directly.

This shift requires more than AI features on top of existing interfaces. It requires a different database contract.

Strata's contract is built around safe agentic work:

* Branch before changing.
* Retrieve from known state.
* Preserve history.
* Expose schemas agents can understand.
* Return errors agents can act on.
* Let humans inspect, approve, roll back, or merge.
* Unify the forms of state agents need to reason and build.
* Make agent runs replayable and auditable.

The core vision is that agents should be able to work with data the way developers work with code: experimentally, safely, reversibly, and with a clear path from provisional state to trusted state.

But Strata is not only "Git for data."

It is the native state substrate for AI agents.
