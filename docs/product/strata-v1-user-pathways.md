# Strata V1 User Pathways

This document is the pathway index for the V1 product definition. Each line
names a user pathway that can later be expanded with Goal, Flow, Surface,
Guarantees, Failures, V1 Decision, Cleanup, and Notes.

Focused direction for versioning and time travel lives in
`docs/product/strata-v1-versioning-time-travel.md`.

Expanded pathway groups:

1. `docs/product/pathways/runtime-and-portability.md`
2. `docs/product/pathways/data-capabilities.md`
3. `docs/product/pathways/retrieval-and-intelligence.md`
4. `docs/product/pathways/branching-versioning-time-travel.md`
5. `docs/product/pathways/operations-and-interfaces.md`

## V1 Pathway Index

1. Pathway 1: Create or open a local embedded database - a developer opens a path and gets a durable, usable Strata database.
2. Pathway 2: Open an ephemeral cache database - a developer or test runtime creates a temporary Strata database without durable persistence.
3. Pathway 3: Open a database read-only - a user inspects or queries an existing database without being able to mutate it.
4. Pathway 4: Share a local database through IPC - an application, Strata AI, or another local process uses an already-open database through the supported shared-process path.
5. Pathway 5: Clone a portable dataset - a user runs `strata clone <source> <destination>` and receives a normal local Strata database.
6. Pathway 6: Use a cloned dataset offline - a user branches, modifies, searches, and exports a cloned database without contacting the source.
7. Pathway 7: Write and read key-value data - a user stores and retrieves simple binary or string-like records.
8. Pathway 8: Write and read JSON documents - a user stores structured documents and retrieves or filters them through product APIs.
9. Pathway 9: Append and query events - a user records ordered event data and reads it back for history, audit, or timeline workflows.
10. Pathway 10: Create and manage graphs - a user creates, deletes, lists, and inspects named graphs in a branch and space.
11. Pathway 11: Model graph entities and relationships - a user stores graph-native nodes and edges or connects KV, JSON, event, vector, and graph records through entity references without duplicating source payloads.
12. Pathway 12: Define graph ontology - a user defines object and link types, freezes ontology metadata, inspects status, and lists nodes by type.
13. Pathway 13: Traverse and query graph neighborhoods - a user queries neighbors or runs bounded BFS with direction, edge-type, branch, space, and time-travel context.
14. Pathway 14: Run graph analytics - a user runs connected-components, community detection, PageRank, clustering, or shortest-path analytics on graph data.
15. Pathway 15: Store and query vectors - a user stores embeddings or numeric vectors and performs similarity search.
16. Pathway 16: Run keyword search - a user searches stored text with BM25-style ranking across supported data capabilities.
17. Pathway 17: Run semantic or hybrid search - a user searches by meaning using stored embeddings, optional query embeddings, and fused keyword/vector results.
18. Pathway 18: Run graph-aware retrieval - a user expands, boosts, or explains search results through relationship-layer graph context when graph data is present.
19. Pathway 19: Use search recipes - a user selects or configures named retrieval recipes such as keyword, semantic, hybrid, graph, default, or RAG.
20. Pathway 20: Use query expansion and reranking - a user improves retrieval quality through configured expansion, HyDE-style variants, fusion, and reranking.
21. Pathway 21: Ask retrieval-backed questions - a user receives search results plus an optional generated answer grounded in retrieved context.
22. Pathway 22: Configure auto-embedding and indexing - a user explicitly enables, observes, repairs, or reindexes branch-local shadow embeddings stored in system space.
23. Pathway 23: Manage models and inference configuration - a user lists, pulls, configures, and inspects local or provider-backed model runtime support.
24. Pathway 24: Generate, tokenize, and detokenize text - a user runs intentional intelligence utilities through a configured inference runtime.
25. Pathway 25: Create and manage branch workspaces - a user creates a branch from existing data, selects branch context, lists branches, inspects branch state, and deletes branches safely.
26. Pathway 26: Inspect record history - a user asks how a KV, JSON, vector, graph relationship, or other supported record changed and sees versions, timestamps, values, deletions, and retained-history limits.
27. Pathway 27: Read data as of a point in time - a user selects a timestamp or version and runs normal reads, lists, graph lookups, vector queries, or supported searches against that historical view.
28. Pathway 28: Scrub and explain a branch timeline - a user inspects the available time range, picks a point, resolves it to concrete state, and understands what changed before or after that point.
29. Pathway 29: Create a branch from historical state - a user creates a new branch from current state, a retained commit version, or a timestamp that resolves to a retained branch point.
30. Pathway 30: Compare, promote, copy, and restore branch changes - a user compares current or historical branch state, previews conflicts, promotes completed work, copies selected records or changes, and restores a bad version range by writing a compensating change.
31. Pathway 31: Organize data with spaces - a user separates logical namespaces while preserving branch, temporal, and command context.
32. Pathway 32: Import and export primitive data - a user moves supported primitive data in and out of Strata through stable formats such as Arrow where supported.
33. Pathway 33: Inspect database state - a user runs describe, health, metrics, and durability-counter commands to understand a database.
34. Pathway 34: Recover from ordinary failures - a user reopens after crashes, lock conflicts, unsupported backends, or configuration errors and receives clear outcomes.
35. Pathway 35: Configure Strata safely - a user manages runtime config, recipes, credentials, and provider settings without leaking secrets.
36. Pathway 36: Run Strata from the CLI - a user operates databases from scripts, terminals, and JSON-output automation.
37. Pathway 37: Use Strata from application code - an application embeds Strata through the public SDK without depending on CLI-only behavior.
38. Pathway 38: Use Strata in agent or sandbox workflows - an agent runtime opens, clones, queries, mutates, and inspects local datasets with explicit filesystem and network behavior.
39. Pathway 39: Choose a storage backend intentionally - a user selects local filesystem, browser/cache, object storage, or OpenDAL-backed targets based on explicit capability errors and guarantees.

## Post-V1 Directional Pathways

1. Future Pathway 1: Discover datasets through StrataHub Library - a user finds a curated dataset and clones it into a local Strata database.
2. Future Pathway 2: Publish a dataset to StrataHub Library - a user shares a cloneable dataset with metadata, provenance, license, and release information.
3. Future Pathway 3: Fork and derive public datasets - a user creates a modified dataset lineage that can be traced back to its source.
4. Future Pathway 4: Observe a fleet of Strata instances - an operator sees where databases are deployed and checks health, versions, storage targets, and last-seen status.
5. Future Pathway 5: Coordinate backup, sync, or movement - an operator explicitly moves or synchronizes Strata data without hidden replication semantics.

## Explicit Non-Pathways For V1

1. Non-pathway 1: Follower mode - multi-process local access should use IPC, not follower refresh semantics.
2. Non-pathway 2: Public begin/commit/rollback workflow - users should not manage transaction sessions directly in V1.
3. Non-pathway 3: Legacy branch bundle workflow - portable cloneable datasets should replace branch-specific bundle commands.
4. Non-pathway 4: Disk-backed cache mode - cache mode should not be a second durability model.
5. Non-pathway 5: Hidden network behavior - Strata should not upload, register, sync, or call providers without explicit user action.
6. Non-pathway 6: Public tags and notes - version labels and annotations are not critical to V1 and can return later if dataset releases or provenance require them.
7. Non-pathway 7: Manual database maintenance - users should not have to flush, compact, checkpoint, or apply retention during normal use.
