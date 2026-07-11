---
summary: Freeze the graph ontology.
mcp_description: Use this when the user wants to lock a graph's ontology so subsequent node and edge writes are validated against the declared types. Freezing is permanent and validates the draft first.
---

Validates the draft ontology and freezes it. Validation requires at least one declared type and rejects link types whose source or target reference undeclared object types (`failed_precondition.engine.graph_ontology_freeze`). After freezing, writes enforce declared node object types, required properties, and link-type endpoint rules; the ontology itself can no longer change (`failed_precondition.engine.graph_ontology_frozen`).
