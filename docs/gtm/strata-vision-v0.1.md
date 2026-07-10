# Strata Vision

## A ground-up data substrate for AI agents

### Version 0.1

## 1. Executive summary

Strata is a ground-up rethinking of the database for a world where AI agents are primary users of data infrastructure.

Most databases were designed for applications written by humans. In that model, humans design schemas, review migrations, write stable code paths, approve destructive changes, and operate databases through deliberate workflows. AI agents violate many of those assumptions. They create schemas, mutate state, retrieve memory, ingest documents, run experiments, generate application logic, and operate at machine speed.

The central premise of Strata is that the database contract must evolve when the primary user shifts from a human-written application to an autonomous or semi-autonomous agent.

A database for AI agents should not simply make databases easier for humans to use. It should make data safer, more natural, and more useful for agents to work with directly. That means branching, rollback, time travel, reproducible retrieval, agent-readable schemas, machine-actionable errors, auditability, and support for multiple forms of state in one coherent substrate.

Strata is designed as an embedded, branch-native, multi-model database for AI agents and AI-built applications. It provides key-value storage, documents, event history, vector retrieval, and graph relationships under a shared versioning and branching model.

The long-term vision is simple:

> **Strata is the database you would build from first principles if the primary user was an AI agent.**

---

## 2. Core thesis

The AI era does not only require databases with AI features. It requires databases whose core abstractions are designed around agents.

The current data stack assumes a world where:

| Assumption       | Traditional database model                              |
| ---------------- | ------------------------------------------------------- |
| Primary user     | Human-written application                               |
| State model      | Mostly linear                                           |
| Change model     | Planned, reviewed, and deliberate                       |
| Schema lifecycle | Designed by humans                                      |
| Error handling   | Interpreted by humans                                   |
| Recovery model   | Backup, restore, manual repair, or operational rollback |
| Retrieval model  | Query-driven, often separated by data type              |
| Experimentation  | External to the database                                |
| Trust model      | Enforced by human process                               |

AI agents create a different operating environment:

| Assumption       | Agent-native model                             |
| ---------------- | ---------------------------------------------- |
| Primary user     | AI agent supervised by human                   |
| State model      | Branching, exploratory, and temporal           |
| Change model     | High-frequency, provisional, and reversible    |
| Schema lifecycle | Human and agent co-created                     |
| Error handling   | Machine-actionable                             |
| Recovery model   | Branch, diff, rollback, merge, time travel     |
| Retrieval model  | Structured, semantic, temporal, and relational |
| Experimentation  | Native to the data layer                       |
| Trust model      | Enforced by system design                      |

This is the conceptual foundation of Strata.

The product is not merely a database with an AI assistant. It is a database whose interface, state model, retrieval model, error semantics, and safety primitives are designed for agents.

---

## 3. One-sentence vision

**Strata is an embedded, branchable database that lets AI agents safely remember, retrieve, experiment, roll back, and act on data.**

A shorter internal shorthand:

**Git for data, built for AI agents.**

But the broader thesis is larger than branching alone. Branching is the first critical primitive because it makes agentic change safe. The full Strata vision is a database contract designed around agentic work.

---

## 4. Why this matters now

AI agents are rapidly becoming active participants in software development and data workflows. They no longer only answer questions or generate snippets of code. They increasingly:

| Agent behavior               | Data-system implication                                                |
| ---------------------------- | ---------------------------------------------------------------------- |
| Generate applications        | They need to create and modify application state                       |
| Write schemas                | They need schema context that is compact, explicit, and understandable |
| Run migrations               | They need safe testing and rollback                                    |
| Ingest documents             | They need document storage, search, and provenance                     |
| Maintain memory              | They need persistent, queryable, versioned memory                      |
| Retrieve semantically        | They need vector and lexical retrieval                                 |
| Reason over relationships    | They need graph-like traversal                                         |
| Call tools                   | They need audit trails and recoverable actions                         |
| Experiment with alternatives | They need branches and diffs                                           |
| Recover from mistakes        | They need time travel and machine-actionable failure modes             |

This creates a gap between how databases were historically designed and how agents actually operate.

The first generation of AI database work has focused on making existing databases more accessible to humans through natural language interfaces, copilots, text-to-SQL, query explanation, automated tuning, and AI-assisted operations. This is valuable. It improves the human interface to databases.

Strata focuses on a different problem:

> **What should the database be when the agent itself is the primary actor?**

---

## 5. First-principles problem statement

A database is not just a storage engine. It is a contract between software and state.

Traditional databases define a contract around durable writes, query execution, consistency, schemas, indexing, transactions, and recovery. That contract has been enormously successful for human-written software.

However, AI agents introduce new requirements that are not well captured by the traditional contract.

### 5.1 Agents are exploratory

Agents try things. They generate intermediate state. They make assumptions. They test hypotheses. They revise. They may take multiple paths before selecting one.

A linear state model forces exploratory behavior into a final-state database. This is risky. Strata treats exploration as a first-class database operation.

### 5.2 Agents are high-velocity

Agents can produce more database-relevant changes in minutes than a human team may produce in weeks. This increases the cost of irreversible writes, opaque migrations, and manual recovery.

Strata assumes high-velocity change and makes isolation, rollback, and review native.

### 5.3 Agents are probabilistic

Even strong models can hallucinate, misread instructions, misuse APIs, or make incorrect assumptions. The data layer should not assume that every agent action is correct.

Strata treats agent changes as provisional until accepted.

### 5.4 Agents need multi-form memory

An agent's working context is not only relational data. It includes scratch state, documents, facts, embeddings, events, decisions, relationships, and prior actions.

Strata unifies these primitives inside one versioned substrate.

### 5.5 Agents need machine-readable contracts

Human-readable errors and schemas are insufficient when the agent is expected to recover autonomously. Agents need compact schemas, structured error responses, and clear recovery instructions.

Strata should expose data contracts in forms that agents can understand and act on directly.

---

## 6. Definition: agent-native database

An **agent-native database** is a data system whose core abstractions are designed for autonomous or semi-autonomous agents that read, write, retrieve, transform, reason over, and operate on state.

An agent-native database should support:

1. **Safe change**
   Changes can be isolated, inspected, rolled back, and merged.

2. **Temporal awareness**
   Past states are accessible and queryable.

3. **Branch-native workflows**
   Agents can experiment without corrupting trusted state.

4. **Multi-model state**
   Scratch, documents, events, vectors, graph relationships, and application data can coexist coherently.

5. **Reproducible retrieval**
   Retrieval results can be tied to a known state and reproduced later.

6. **Agent-readable schema**
   The database can expose schema and constraints in a compact, structured, agent-usable format.

7. **Machine-actionable errors**
   Errors are designed not only for human diagnosis, but for autonomous correction.

8. **Auditability**
   Agent actions can be traced, reviewed, and attributed.

9. **Embedded operation**
   The database can run close to the agent, especially in local development and agentic build loops.

10. **Human supervision**
    Agents can operate autonomously within boundaries while humans retain review, approval, and recovery controls.

---

## 7. Product vision

Strata is an embedded, branch-native, multi-model database for AI agents.

It provides a unified substrate for:

| Primitive | Agent use case                                      |
| --------- | --------------------------------------------------- |
| Key-value | Scratch state, temporary reasoning artifacts, cache |
| Documents | Application state, user data, structured objects    |
| Events    | Audit trails, action history, decision logs         |
| Vectors   | Semantic memory, retrieval, similarity search       |
| Graphs    | Relationships, dependencies, entities, provenance   |
| Branches  | Safe experimentation, rollback, merge, time travel  |

These primitives are not separate products. They are different views over one versioned state substrate.

The goal is to let an agent use one database for the full lifecycle of agentic work:

1. Understand existing state.
2. Retrieve relevant context.
3. Create a branch.
4. Try a change.
5. Inspect the result.
6. Compare against the base state.
7. Roll back, revise, or merge.
8. Preserve an audit trail.
9. Continue operating from trusted state.

This is the core workflow Strata is designed to enable.

---

## 8. The central primitive: branching

Branching is the first defining primitive of Strata.

In software development, branching made experimentation safe. Developers could try changes without corrupting the main codebase. They could compare diffs, review changes, merge good work, and discard bad work.

AI agents need the same safety model for data.

Every agent run should be able to operate on an isolated branch. This allows the agent to:

| Operation                    | Why it matters                                            |
| ---------------------------- | --------------------------------------------------------- |
| Create schema changes safely | Migrations can be tested before becoming real             |
| Modify records provisionally | Data changes are not final until accepted                 |
| Ingest uncertain data        | Messy inputs can be cleaned before promotion              |
| Test retrieval strategies    | Search behavior can be evaluated against a known snapshot |
| Simulate workflows           | Agents can reason over hypothetical states                |
| Compare outcomes             | Diffs make agent behavior inspectable                     |
| Roll back mistakes           | Failed runs do not corrupt trusted state                  |
| Merge approved changes       | Valid work can become part of the main branch             |

Branching changes the trust model.

Without branching, the agent's action becomes a direct mutation of state.

With branching, the agent's action becomes a proposal.

That distinction is foundational.

---

## 9. Why multi-model matters

Agents do not naturally think in database categories. They do not experience tasks as "relational workload," "vector workload," "document workload," or "graph workload." They experience a task as a chain of reasoning over many kinds of information.

For example, an agent building an application may need to:

1. Store user records.
2. Cache intermediate build state.
3. Search design documents.
4. Embed and retrieve prior decisions.
5. Track migrations as events.
6. Understand relationships between entities.
7. Roll back a failed schema change.
8. Explain what changed.

A fragmented toolchain forces the builder to stitch together different systems for each part of this workflow.

Strata's multi-model design is not primarily about feature breadth. It is about coherence for agentic work.

The important question is not:

> Can Strata support many data types?

The important question is:

> Can an agent work across many forms of state through one consistent model of versioning, retrieval, audit, and recovery?

That is why multi-model support belongs inside the Strata vision.

---

## 10. Why embedded matters

Strata is designed to be embedded because many agentic workflows begin close to the builder, the development loop, or the local agent runtime.

An embedded database gives Strata several advantages:

| Advantage                   | Rationale                                                    |
| --------------------------- | ------------------------------------------------------------ |
| Low friction                | No signup, provisioning, or external infrastructure required |
| Low latency                 | No network round trip in the agent development loop          |
| Local-first workflow        | Agents can build, test, and revise applications locally      |
| Simple distribution         | Strata can ship with the agent or application                |
| Strong developer experience | The database is immediately available during creation        |
| Safe experimentation        | Branches can be created cheaply and frequently               |
| Offline capability          | Agent workflows can continue without cloud dependency        |

Embedded does not mean Strata can never connect to cloud services. It means the core product should be useful without requiring cloud infrastructure first.

The initial adoption path should feel closer to SQLite, Git, or DuckDB than to a managed enterprise database.

---

## 11. Agent interface requirements

For Strata to be genuinely agent-native, it cannot expose only human-centric database interfaces.

Agents require interfaces that make state, schema, errors, and recovery actions explicit.

### 11.1 Agent-readable schema

The schema should be available as a compact artifact that an agent can consume efficiently.

This should include:

| Schema element                 | Purpose                          |
| ------------------------------ | -------------------------------- |
| Entities and fields            | Understand data shape            |
| Types and constraints          | Avoid invalid writes             |
| Relationships                  | Understand dependencies          |
| Indexes and retrieval surfaces | Choose efficient access paths    |
| Branch metadata                | Understand current state context |
| Permissions and boundaries     | Avoid unauthorized actions       |
| Examples                       | Improve agent reliability        |

The schema should not require repeated introspection across many calls. An agent should be able to understand the relevant data model quickly and accurately.

### 11.2 Machine-actionable errors

Traditional database errors are written for humans. Agents need structured errors that support correction.

A machine-actionable error should include:

| Field              | Example purpose                                         |
| ------------------ | ------------------------------------------------------- |
| Error type         | Constraint violation, type mismatch, missing field      |
| Relevant object    | Table, document collection, branch, index               |
| Failed operation   | Insert, update, merge, query                            |
| Cause              | Why the operation failed                                |
| Suggested recovery | Valid next actions                                      |
| Safety level       | Whether retry, rollback, or human review is recommended |

The goal is not merely to explain failure. The goal is to help the agent recover safely.

### 11.3 MCP and tool-native access

Strata should support agent-native access patterns, including MCP-style interfaces, so agents can inspect state, query data, create branches, run changes, view diffs, and request merges through structured tools.

The agent interface should make core operations explicit:

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

The interface should make safe behavior the easiest behavior.

---

## 12. Trust as the core product problem

The fundamental product problem is not storage alone. It is trust.

As AI systems become more capable, users will ask:

| Trust question                             | Strata answer                                               |
| ------------------------------------------ | ----------------------------------------------------------- |
| What did the agent change?                 | Branch diffs and audit logs                                 |
| Can I undo it?                             | Rollback and time travel                                    |
| Can I test it first?                       | Branch-native execution                                     |
| Can I recover old state?                   | Versioned storage                                           |
| Can I understand why it happened?          | Event history and explanations                              |
| Can the agent avoid repeating the mistake? | Structured errors and memory                                |
| Can I approve before commit?               | Human-supervised merge                                      |
| Can the agent retrieve the right context?  | Unified retrieval over state, documents, vectors, and graph |

The core trust gap is between:

> "The agent did something."

and:

> "I understand, accept, and can rely on what the agent did."

Strata exists to close that gap at the data layer.

---

## 13. Design principles

### Principle 1: The agent is a first-class database user

Strata should be designed as if the agent is not merely a source of queries, but an active user of the database.

### Principle 2: Change should be provisional by default

Agent changes should be easy to isolate, inspect, accept, or discard.

### Principle 3: Trusted state and experimental state must be separated

The database should distinguish between what is accepted and what is being explored.

### Principle 4: Retrieval should be reproducible

An agent should be able to reproduce the context that led to a decision.

### Principle 5: Multi-model support should serve coherence, not feature breadth

The purpose of key-value, documents, vectors, events, and graphs is to support agent workflows under one consistent contract.

### Principle 6: Errors should enable recovery

An error should help an agent take the next safe action.

### Principle 7: The system should be useful before it is cloud-managed

The embedded product should deliver value immediately in local and agentic build workflows.

### Principle 8: Human supervision should be built into the workflow

Strata should allow agents to operate quickly while preserving human review over important state transitions.

### Principle 9: Auditability is not optional

Agentic systems need durable records of what happened, when, why, and on which branch.

### Principle 10: The database should make safe behavior easier than unsafe behavior

The default workflow should encourage branch, inspect, diff, and merge.

---

## 14. Target users and initial use cases

### 14.1 AI-assisted application builders

These users build applications through coding agents. They need a database that is safe for agents to manipulate during development.

Core needs:

| Need                   | Strata capability           |
| ---------------------- | --------------------------- |
| Safe schema generation | Branch-based schema changes |
| Reversible migrations  | Time travel and rollback    |
| Local development loop | Embedded runtime            |
| Application state      | Documents and key-value     |
| Semantic memory        | Vector retrieval            |
| Build audit            | Event log                   |

### 14.2 Agent framework developers

These users build agents that need persistent memory, retrieval, state management, and tool interaction.

Core needs:

| Need                 | Strata capability                 |
| -------------------- | --------------------------------- |
| Agent memory         | Documents, events, vectors        |
| Safe tool execution  | Branches and rollback             |
| State inspection     | Agent-readable schema             |
| Traceability         | Event logs and diffs              |
| Multi-step workflows | Versioned state across agent runs |

### 14.3 Data and workflow automation teams

These users deploy agents to ingest, transform, reconcile, and operate on data.

Core needs:

| Need                        | Strata capability             |
| --------------------------- | ----------------------------- |
| Messy data ingestion        | Branches for uncertain inputs |
| Validation before promotion | Diff and merge                |
| Provenance                  | Event trail                   |
| Relationship reasoning      | Graph                         |
| Contextual retrieval        | Vector and document search    |

### 14.4 Future: AI-native application platforms

Long term, Strata can become the default local or embedded data layer for applications that are created, modified, and operated by agents.

---

## 15. Core system requirements

| Requirement               | Description                                           | Why it matters                                |
| ------------------------- | ----------------------------------------------------- | --------------------------------------------- |
| Embedded runtime          | Runs locally with minimal setup                       | Enables adoption in agentic development loops |
| Branch-native storage     | Every state can be forked and merged                  | Makes agent experimentation safe              |
| Time travel               | Past states are recoverable                           | Supports rollback and audit                   |
| Multi-model primitives    | KV, documents, events, vectors, graph                 | Matches agent memory and state needs          |
| Shared versioning model   | All primitives participate in common branch semantics | Prevents fragmented state                     |
| Agent-readable schema     | Compact schema artifact for agents                    | Reduces error and repeated introspection      |
| Machine-actionable errors | Structured errors with recovery guidance              | Enables autonomous correction                 |
| Deterministic retrieval   | Retrieval tied to snapshot and recipe                 | Supports reproducibility                      |
| Audit log                 | Durable record of changes and actions                 | Enables supervision and trust                 |
| Diff and merge            | Compare branches and promote changes                  | Enables human review                          |
| MCP interface             | Structured tool access for agents                     | Makes Strata usable by modern agents          |
| Fast local operations     | Low-latency reads and writes                          | Supports scratchpad and build-loop use cases  |

---

## 16. Non-goals

A clear vision also requires clear non-goals.

### 16.1 Strata is not primarily an AI assistant for database administrators

Strata may eventually expose assistant-like experiences, but the core product is not a chatbot over database metadata. The core product is a new database contract for agents.

### 16.2 Strata is not initially a replacement for every production database

The initial goal is not to displace Oracle, Postgres, MySQL, MongoDB, Snowflake, or other established systems in their strongest production environments. The initial goal is to create a new category around agent-native state.

### 16.3 Strata is not only a vector database

Vectors are important, but agent memory requires more than embeddings. It requires structured state, events, provenance, relationships, and safe mutation.

### 16.4 Strata is not only a developer toy

Although the embedded local workflow is important for adoption, the architecture should support serious state management, durability, auditability, and eventual production use.

### 16.5 Strata should not optimize for feature breadth at the cost of coherence

Multi-model support is valuable only if it strengthens the unified agent workflow. A shallow combination of unrelated features is not the goal.

---

## 17. Research and product hypotheses

Strata should be developed around testable hypotheses.

### Hypothesis 1: Agents need branch-native data workflows

AI agents will make more safe and useful changes when their database operations are isolated in branches by default.

Possible evaluation:

| Metric                  | Measurement                                                       |
| ----------------------- | ----------------------------------------------------------------- |
| Recovery rate           | Percentage of failed agent runs recoverable without manual repair |
| Time to rollback        | Latency to restore prior state                                    |
| Human review efficiency | Time to inspect and approve agent changes                         |
| Error containment       | Percentage of bad changes prevented from reaching main state      |

### Hypothesis 2: Unified state improves agent reliability

Agents will perform better when scratch, memory, documents, events, vectors, and relationships are available through a coherent interface.

Possible evaluation:

| Metric                     | Measurement                                   |
| -------------------------- | --------------------------------------------- |
| Task completion rate       | Agent succeeds in multi-step workflow         |
| Context retrieval accuracy | Relevant context retrieved from mixed data    |
| API complexity             | Number of tools or systems required           |
| Failure attribution        | Ability to diagnose where the workflow failed |

### Hypothesis 3: Agent-readable schemas reduce errors

Agents will produce fewer invalid operations when schemas are exposed in compact, structured, task-relevant form.

Possible evaluation:

| Metric                | Measurement                                           |
| --------------------- | ----------------------------------------------------- |
| Invalid write rate    | Type, constraint, and missing-field errors            |
| Query correction rate | Agent recovers after failed query                     |
| Schema comprehension  | Accuracy of generated operations                      |
| Token efficiency      | Schema context size required for successful operation |

### Hypothesis 4: Machine-actionable errors improve autonomous recovery

Structured errors with recovery guidance will help agents correct failures without human intervention.

Possible evaluation:

| Metric                   | Measurement                                       |
| ------------------------ | ------------------------------------------------- |
| Autonomous recovery rate | Agent fixes failed operation without human help   |
| Retry efficiency         | Number of attempts before success                 |
| Unsafe retry rate        | Agent avoids repeating harmful operations         |
| Escalation accuracy      | Agent correctly asks for human review when needed |

### Hypothesis 5: Embedded deployment accelerates adoption

A local-first embedded database will be easier for agent builders to adopt than a managed service requiring provisioning.

Possible evaluation:

| Metric                             | Measurement                                |
| ---------------------------------- | ------------------------------------------ |
| Time to first successful agent run | Setup to working state                     |
| Developer activation rate          | Percentage reaching meaningful usage       |
| Retention                          | Continued use across projects              |
| Integration complexity             | Lines of setup code or tool calls required |

---

## 18. Evaluation framework

Strata should be evaluated not only as a database, but as an agent substrate.

Traditional database metrics remain important:

| Traditional metric | Importance                         |
| ------------------ | ---------------------------------- |
| Read latency       | Required for agent loops           |
| Write latency      | Required for scratch and app state |
| Durability         | Required for production use        |
| Storage efficiency | Required for scale                 |
| Index performance  | Required for retrieval             |
| Concurrency        | Required for multi-agent use       |

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
| Trust improvement         | Do users allow agents to perform more serious operations? |

The most important long-term benchmark may be:

> Can an AI agent build, modify, and operate a real application with less human intervention and lower state-corruption risk when using Strata?

---

## 19. Strategic wedge

The initial wedge should be the trust problem in AI-assisted software development.

AI coding agents are becoming increasingly capable at generating applications. However, the data layer remains a source of fragility. Builders are willing to let agents write code because code has source control. They are less willing to let agents manipulate data because data changes are harder to inspect, isolate, and reverse.

Strata's early wedge:

> **Make it safe for AI agents to work with data during application creation.**

This is narrow enough to be concrete and broad enough to expand.

Initial product experience:

1. Install Strata locally.
2. Connect it to an AI coding agent.
3. Let the agent create a database schema.
4. Every run happens on a branch.
5. The builder can inspect diffs.
6. Good changes are merged.
7. Bad changes are rolled back.
8. The app uses Strata as its embedded database.

This gives the user an immediate reason to care.

The long-term expansion is from local agentic development to agent-native production state.

---

## 20. Long-term vision

In the long term, every serious AI system will need a data layer designed around agentic behavior.

That data layer will not only store final state. It will support:

| Capability   | Long-term role                                         |
| ------------ | ------------------------------------------------------ |
| Memory       | Persistent state across agent runs                     |
| Retrieval    | Relevant context from structured and unstructured data |
| Branching    | Safe exploration and isolated action                   |
| Simulation   | Testing changes before committing                      |
| Provenance   | Understanding where information came from              |
| Audit        | Reviewing what agents did                              |
| Rollback     | Recovering from mistakes                               |
| Merge        | Promoting validated changes                            |
| Coordination | Supporting multiple agents working over shared state   |
| Supervision  | Giving humans control over important transitions       |

This suggests a future in which databases are not passive systems behind applications. They become active workspaces for agents.

Strata's long-term objective is to become that workspace.

---

## 21. Relationship to StrataHub

StrataHub can extend the core Strata vision by making databases cloneable, branchable, and shareable across developers and agents.

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
| Pull request   | Proposed data change               |

StrataHub could allow developers to:

1. Publish Strata databases.
2. Clone real datasets.
3. Branch application state.
4. Share agent-built applications with data included.
5. Review proposed state changes.
6. Build reproducible agent benchmarks.
7. Host curated datasets for AI builders.

This should be treated as a platform extension, not the initial core. The core must first prove that Strata is a valuable local, embedded database for agents.

---

## 22. Risks and open questions

### 22.1 Risk: The market may understand "Git for data" but not the broader database thesis

Mitigation: Use "Git for data" as the wedge, then explain the broader agent-native substrate.

### 22.2 Risk: Multi-model scope may become too broad

Mitigation: Prioritize primitives that directly support agent workflows. Avoid implementing features that do not strengthen the agent-native contract.

### 22.3 Risk: Branching may be technically expensive

Mitigation: Optimize branch creation, copy-on-write behavior, compaction, and merge semantics early. Branching must feel cheap enough to be default.

### 22.4 Risk: Developers may initially use Strata only as a local toy

Mitigation: Make the local experience excellent, but design durability, audit, and production-readiness into the architecture from the beginning.

### 22.5 Risk: Existing databases may add partial agent features

Mitigation: Focus on the integrated contract. Strata's differentiation is not one feature, but the coherence of branching, multi-model state, agent interface, and recovery.

### 22.6 Risk: Agents may not yet be trusted with enough database operations

Mitigation: Start with supervised workflows. The point is not full autonomy on day one. The point is making increasingly serious agent operations safe enough to permit.

### 22.7 Risk: Merge semantics across data types may be complex

Mitigation: Define clear merge strategies by primitive, starting with conservative defaults and explicit conflict handling.

---

## 23. Internal positioning

Strata should be positioned internally as:

> **An agent-native data substrate for safe stateful AI systems.**

Supporting language:

* **Not:** A database with an AI assistant.
* **Not:** Just a vector database.
* **Not:** Just a local embedded database.
* **Not:** Just Git for data.
* **Yes:** A branch-native, embedded, multi-model database designed around the agent as the primary user.

The most important internal distinction:

> **AI-assisted databases improve the human interface. Agent-native databases change the state contract.**

Strata is focused on the second.

---

## 24. Proposed canonical wording

### One sentence

**Strata is a branch-native embedded database that lets AI agents safely remember, retrieve, experiment, roll back, and act on data.**

### Two sentences

**Strata is the database you would build from scratch if the primary user was an AI agent. It gives agents a branchable, reversible, multi-model data layer for memory, retrieval, application state, audit, and safe experimentation.**

### Internal thesis

**AI agents require a new database contract. Traditional databases assume human-written applications, linear state, deliberate writes, human-readable errors, and external recovery workflows. Strata assumes agents are active users of the data layer and makes branching, rollback, retrieval, audit, and machine-actionable operation part of the database itself.**

### Strategic claim

**The next wave of AI infrastructure is trusted state. Models provide intelligence, tools provide action, and Strata provides the data layer that makes agentic action safe.**

---

## 25. Conclusion

Strata exists because the role of the database is changing.

In the pre-agent era, the database primarily served applications written and operated by humans. In the agent era, the database must increasingly serve autonomous systems that create, modify, retrieve, reason over, and act on state directly.

This shift requires more than AI features on top of existing interfaces. It requires a different database contract.

Strata's contract is built around safe agentic work:

* Branch before changing.
* Retrieve from known state.
* Preserve history.
* Expose schemas agents can understand.
* Return errors agents can act on.
* Let humans inspect, approve, roll back, or merge.
* Unify the forms of state agents need to reason and build.

The core vision is that agents should be able to work with data the way developers work with code: experimentally, safely, reversibly, and with a clear path from draft state to trusted state.

Strata is the database for that world.
