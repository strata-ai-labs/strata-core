---
summary: Define a graph link type.
mcp_description: Use this when the user wants to declare (or, while the ontology is still a draft, redefine) an edge link type between two object types, optionally with a cardinality hint and typed properties.
---

Declares a link type in the graph's ontology: a name, its source and target object types, an optional cardinality hint (for example `many-to-one`), and property definitions. Source and target must name declared object types by the time the ontology is frozen. After freezing, this command fails with `failed_precondition.engine.graph_ontology_frozen`.
