---
summary: Apply a delete policy to bound graph facts.
mcp_description: Use this after deleting an entity in another primitive, when the user must decide what happens to graph nodes bound to it - cascade-delete them, detach the bindings, or keep them dangling. Wire-only; invoke via the generic command runner.
---

Applies an explicit policy to every graph node bound to the given entity target: `cascade` deletes the bound nodes and their incident edges, `detach` keeps the nodes but removes their bindings, and `keep_dangling` preserves the bindings so traversal can report the target's status. The acknowledgement reports how many bound nodes the policy covered.
