# First-run experience — cross-repo handoff pack

Strata-core's half of the first-run workstream is **landed and pushed** (branch `v1`,
2026-07-06): database-target safety (D2), the `strata agents` family (D3), teaching
errors with stable refs (D4), `strata doctor` (D5), and the MCP transport
`strata mcp serve` (D8's binary half). The remaining deliverables live in the other
repos. Each document in this directory is a self-contained work order for one repo —
it carries every contract fact needed to do the work without access to strata-core.

Source of truth for the overall design: `docs/design/first-run-experience.md` in
`stratalab/strata-core` (delivery map §11 tracks status).

| Document | Repo | Delivers |
|---|---|---|
| [`00-shared-contracts.md`](00-shared-contracts.md) | all (read first) | The binary's shipped contracts: targeting, output, errors, agents surface, MCP, versioning |
| [`stratadb-org.md`](stratadb-org.md) | stratadb.org | install.sh hardening (D6), **`/e/<code>` error pages** (D4 dependency), llms.txt upgrade + release.json (D11) |
| [`strata-mcp.md`](strata-mcp.md) | strata-mcp | `@stratadb/mcp` npx shim + registry listings (D8's distribution half); repo repositioning |
| [`strata-python.md`](strata-python.md) | strata-python | **Full V1 SDK specification** (ground-up: the current wheel binds the old engine) — binding architecture, complete API surface, error model, packaging (D9) |
| [`strata-nodesdk.md`](strata-nodesdk.md) | strata-nodesdk | **Full V1 SDK specification**, isomorphic to the Python spec with Node conventions (D9) |
| [`strata-eval.md`](strata-eval.md) | strata-eval | Agent onboarding evals with budgets + release gates (D12) |

Every repo also owes the **D1 sweep**: all GitHub references point at the `stratalab`
org (`stratalab/strata-core` for the engine). Four different org names shipped in
artifacts before the sweep; each doc lists its repo's known offenders.

Ordering: stratadb-org's `/e/<code>` pages are the most urgent item in the pack —
shipped binaries already emit those URLs on every error. The release train (D7,
runs from strata-core) gates the shim, wheels, and prebuilds being *published*, but
none of the build work needs to wait for it.
