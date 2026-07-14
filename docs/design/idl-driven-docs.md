# IDL-driven documentation: one source, many surfaces

Status: design / proposal · Owner: TBD · Scope: `strata-core` IDL + `strata-idl`
generator → `stratadb.org` docs, SDK docstrings, agents guides, `llms.txt`

## Context

The V1 IDL (`crates/executor/idl/v1/`) has become the single source of truth for
a growing set of surfaces. From one set of files it already drives:

1. the executor `Command`/`Output` wire (JSON Schemas, `deny_unknown_fields`);
2. the **CLI** command surface (validated against the catalog);
3. the **Python SDK** generated core (one typed method per command) + its
   coverage guard;
4. **MCP tool descriptions** (`mcp_description` in prose frontmatter);
5. **agents guides** (`strata agents guide`, and now the SDK's `agents_guide()`);
6. `command-index.json` (offline introspection, bundled in the CLI + SDK);
7. per-command **JSON Schemas** (validation, golden fixtures);
8. the **error registry** + `/e/<code>` error routes;
9. golden **fixtures** (`verify-fixtures`).

The IDL was, in fact, designed with docs in mind: `families.yaml` already
declares a docs URL per family (`docs: /docs/kv/{op_path}`) with per-family
error lists, and `prose/commands/<id>.md` carries curated `summary` /
`mcp_description` / body prose plus reusable `prose/snippets/`. The generated
`command-index.json` already carries, per command: `title`, `summary`,
`description`, `docs`, `errors` (codes + `/e/` URLs), `cli`, `mcp`, `kind`,
`response_model`, `pagination`, `fixtures`.

The website (`stratadb.org`, Astro) has **already committed to this
architecture**: its IA strategy (`docs/product/01-ia-content-strategy.md`) states
in §6 that *"docs markdown is synced from `strata-core` … this repo owns
presentation, not prose,"* via an existing `repository_dispatch: docs-update`
pipeline; every docs page carries a `source:` frontmatter field recording *which
repo@version it documents*; and §7 specs the `llms.txt` with "stable source URLs
per section so agents can cite precisely." What is missing is only that the
**reference layer of that synced content is not yet generated from the IDL** — so
it can drift, and it does not cover 100% of the command surface.

This is also the highest-leverage item from the agent-SDK-discovery research
(`strata-internal/research/agent-dx/`): a coding agent's correctness is bounded
by **accurate, non-drifting, retrievable reference + examples**. Generating the
reference from the IDL makes "the docs are wrong" — the actively harmful failure
mode — structurally impossible.

## Decision

**The IDL is the documentation spine. Generate the reference + machine layer from
it; curate the narrative but ground and CI-test its examples against the IDL.**

The `strata-idl` generator gains a `generate-docs` mode that renders, per
command, a reference page at the `families.yaml` docs path, plus per-family index
pages and the machine layer (`llms.txt`, per-page `.md`). This feeds the existing
`strata-core → stratadb.org` `docs-update` pipeline, honoring the site's stated
"strata-core owns prose, site owns presentation" split.

## What is generated vs. curated

| Layer | URL shape | Source | Rationale |
|---|---|---|---|
| **Reference** — per command / primitive | `/docs/<family>/<op>` | **Generated** from prose + schema + errors + families | Bulk of the site; where drift hurts and agents need accuracy most |
| **Machine layer** — `llms.txt`, `llms-full.txt`, per-page `.md`, `/e/<code>` | site root | **Generated** from the same IDL | One IDL → consistent across CLI/SDK/MCP/web; §7-spec'd stable URLs |
| **How-to / cookbook** | `/docs/cookbook/*` | **Curated** narrative; code snippets are IDL-derived + CI-tested | You can't generate a good recipe; you *can* keep its code honest |
| **Concepts / explanation** | `/docs/concepts/*` | **Curated**; IDL supplies vocabulary + cross-links | Essays are human work |
| **Getting started / tutorials** | `/docs/getting-started/*` | **Curated**, examples IDL-grounded | — |

Model: **the IDL is the spine; the reference + machine layer hang off it as
generated output; the narrative is curated but grounded and cross-linked.** Never
generate narrative prose; never hand-maintain reference.

## A generated reference page (composition)

Each command renders one Astro-content markdown page from data that already
exists:

- **Frontmatter**: `title` (from `title`/prose), `description` (`summary`),
  `source: strata-core@<rev>` (provenance / drift, matching the site's existing
  `source` field), `section`, ordering.
- **Body**: the prose body (`prose/commands/<id>.md`), unchanged.
- **Parameters**: a table from the request JSON Schema — name, type, required,
  description (already present, e.g. `as_of: Optional timestamp in
  microseconds.`). `branch`/`space`/`type` render as the shared scope note.
- **Returns**: the `response_model` + response schema.
- **Errors**: family-level (`families.yaml`) + command-level (`command-index`
  `errors`), each linking its `/e/<code>` page.
- **Examples**: per language (§ below).
- Cross-links to related commands in the family, and to concept pages by topic.

The per-family index page (`/docs/<family>`) lists its commands with summaries —
also generated.

## The multiplier: a canonical example *in* the IDL

The largest win is making a **canonical example per command a first-class IDL
citizen** — a language-neutral spec (which arguments, expected result), stored in
the prose frontmatter (e.g. an `example:` block) or a sibling
`examples/<id>.yaml`. From that one source, per-language renderers emit:

- the **CLI** invocation (`strata kv get greeting`);
- the **Python** call (`db.kv.get("greeting")` → `b'hello'`) — reusing the SDK
  codegen's existing per-command call rendering (`tools/generate.py` already maps
  a command to a typed Python method);
- the website's **multi-language tabs**;
- the SDK **docstring** `Examples:` (today hand-written + CI-tested by
  `tests/test_doctests.py` — these become generated);
- the **agents guide** snippets.

All consistent, all **CI-tested** against the real binary/SDK (the doctest
harness and a CLI golden-runner become the verifier). This is the Stripe/Supabase
multi-language-docs model, and it closes the loop: the SDK-native `agents_guide()`
(currently hand-written) can then be *generated* per language from the IDL.

## Pipeline and consistency

- **Generation lives in `strata-core`** (it owns the IDL): `strata-idl
  generate-docs` writes the reference markdown + `llms.txt` into a strata-core
  output dir; the existing `repository_dispatch: docs-update` pipeline syncs it to
  `stratadb.org`, which presents it. This honors §6's split (strata-core owns
  prose, site owns presentation) and is preferred over the site pulling the IDL.
- **Drift guard**: `strata-idl check-docs` (re-generate, diff) in strata-core CI,
  exactly like `check` / `check-cli` / `verify-fixtures`. A stale reference page
  fails CI.
- **Provenance**: every generated page's `source: strata-core@<rev>` makes
  version and origin explicit (the site already models this).

## Alignment with the site's existing plan

- §3 sitemap keeps **Reference** (execution-mode personas P1/P3); this fills it.
- §6 "strata-core owns prose" — satisfied: the IDL *is* the prose+structure.
- §7 `llms.txt` spec ("stable source URLs so agents can cite precisely") —
  generated from the catalog, so URLs are stable and complete.
- §4 footer "For agents" (llms.txt / for-agents / MCP reference) — all
  IDL-derived, mutually consistent.
- The removed `reference` content-collection (§6 dead-schema cleanup) is
  reinstated with a real, generated directory behind it.

## Non-goals / boundaries

- Do **not** generate tutorials, concepts, or cookbook prose. Generate the
  reference + machine layer; ground the narrative's *code* only.
- Per-language example renderers reuse the SDK codegen path; the docs generator
  does not re-derive type mappings.
- No new runtime dependency in the shipped binary — `generate-docs` is behind the
  `idl-tooling` dev feature, like the rest of `strata-idl`.

## Open decisions (resolved in the implementation plan)

1. **Examples source** — canonical `example:` in the IDL (recommended; the
   multiplier) vs. reuse SDK doctests for Python-only vs. no examples in phase 1.
2. **Cross-repo mechanism** — strata-core generates + `docs-update` syncs
   (recommended; matches §6) vs. site vendors the IDL by rev and generates at
   build.
3. **Scope of v1** — reference-from-prose+schema first (fast, zero new authoring),
   then the canonical-example engine as a distinct phase.

## Consequences

- Reference docs cannot drift; they cover 100% of commands by construction.
- One edit to `prose/commands/<id>.md` (or the example spec) updates the CLI help,
  MCP description, agents guides, SDK docstring, *and* the website — the "paying
  off in multiples" made literal.
- The agent-discovery research's top lever (accurate, retrievable, non-drifting
  reference + examples) is achieved structurally, for every surface at once.
