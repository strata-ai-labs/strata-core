---
summary: Define a graph object type.
mcp_description: Use this when the user wants to declare (or, while the ontology is still a draft, redefine) a node object type with named, typed properties in a graph's ontology.
---

Declares an object type in the graph's ontology: a name plus property definitions (`value_type`, `required`). While the ontology is a draft, redefining a type replaces it freely. After `graph.ontology.freeze`, the ontology is immutable and this command fails with `failed_precondition.engine.graph_ontology_frozen`.
