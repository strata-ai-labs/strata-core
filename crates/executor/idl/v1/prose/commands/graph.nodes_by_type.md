---
summary: List nodes declaring an object type.
mcp_description: Use this when the user wants every node of a given object type, for example all Person nodes. Works with draft or frozen ontologies.
---

Lists the nodes that declare a given object type, in node-id order. The type index is maintained from each node's declared `object_type`, so this works whether the ontology is draft or frozen. Accepts a limit, an exclusive cursor, and `as_of` for time travel.
