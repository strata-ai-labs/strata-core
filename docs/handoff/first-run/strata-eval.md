# strata-eval — work order (D12)

Prereq reading: `00-shared-contracts.md`. Onboarding is a benchmarked surface
(design principle P7): agent-driven install evals with explicit budgets, run per
release, gating exactly like a perf regression. The eval framework in this repo
exists; this adds the onboarding category.

## Setup

Clean container per run (no Strata preinstalled, no caches), an agent
(Claude and/or Codex) with shell access and network. Fix the release under test
via `STRATA_VERSION` / pinned package versions so runs are reproducible.

## Tasks (one eval each; grow the set over time)

1. **cURL path** — "Install Strata and store/retrieve a key."
   Expected shape: `curl … | sh` → epilogue teaches next steps →
   `strata ./db kv put/get`. The D2 refusal is part of the surface: an agent
   that tries a bare `strata kv put` should be corrected by the teaching error
   in one step.
2. **Python path** — "Add Strata to this Python project and index 50 documents
   for semantic search." (`uv add stratadb`, vector collection, upserts, query.)
   Gated on the M9 V1 wheel.
3. **MCP path** — "Wire Strata as persistent MCP memory for this Claude Desktop
   config." Expected: one `npx -y @stratadb/mcp` config line; the model then
   orients via the `strata_guide` tool.
4. **Recovery path** — seed a failure (e.g. read from a nonexistent branch) and
   ask the agent to fix it. Measures whether the teaching-error loop works:
   code + hint + `stratadb.org/e/<code>` ref should make the fix zero-search.

## Metrics (per task)

- Tool calls to first verified write (primary).
- **Web searches — target: 0.** The offline surface (`strata agents guide`,
  embedded READMEs, teaching errors) exists precisely to make this zero.
- Hallucinated flags/commands — **file each one as an issue**; hallucinations
  reveal what agents expect the surface to be, and sometimes the right fix is
  an alias, not a doc.
- Wall clock; success rate over N seeded runs.

## Cadence and gating

Every release candidate, plus whenever any channel artifact changes
(install.sh, shim, wheel, npm, guide content). A regression in tool-calls-to-
first-write or success rate gates the release like a perf regression.

## Acceptance

- The four tasks run against a release candidate with reproducible pinning.
- A scoreboard per release (metrics above) lands somewhere durable.
- At least one full green run of tasks 1 and 3 before V1 promotion
  (task 2 gates on M9; task 4 on the /e/ pages being live).
