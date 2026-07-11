---
summary: Delete a draft link type.
mcp_description: Use this when the user wants to remove a link type from a graph's draft ontology. Frozen ontologies cannot change.
---

Removes a link type from the graph's draft ontology. Deleting a type that was never declared is not an error: the acknowledgement reports `deleted: false`. Once the ontology is frozen this command fails with `failed_precondition.engine.graph_ontology_frozen`.
