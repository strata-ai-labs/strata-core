---
summary: Create a named graph.
mcp_description: Use this when the user wants to create a new named graph for nodes and edges. Fails if a graph with this name already exists.
---

Creates an empty named graph in the selected space and returns its metadata, including node and edge counts (zero at creation) and the create commit coordinates. A database can hold many graphs; graph names are unique per branch and space. Creating a name that already exists fails with `already_exists.engine.graph`.
