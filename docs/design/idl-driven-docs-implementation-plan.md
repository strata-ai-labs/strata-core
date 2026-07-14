# IDL-driven documentation — implementation plan

Companion to `idl-driven-docs.md`. Phased, each phase shippable and
drift-guarded. Convention mirrors the existing `strata-idl` tooling
(`generate` / `check` / `generate-cli` / `check-cli` / `verify-fixtures`).

## Repos & ownership

- **strata-core** — owns the IDL, the `strata-idl` generator, generated output,
  and the drift guards. All generation lives here (site policy §6: "strata-core
  owns prose").
- **stratadb.org** — consumes the generated markdown via the existing
  `repository_dispatch: docs-update` pipeline; owns presentation (Astro
  collections, nav, `.md` serving). No IDL logic.

## Output contract (all phases target this)

Generated reference lands in `crates/executor/idl/v1/generated/docs/` (committed,
drift-guarded), one file per command + one index per family + root `llms.txt` /
`llms-full.txt`. Each page: Astro-compatible frontmatter (`title`, `description`,
`source: strata-core@<rev>`, `section`, `order`) + body. The `docs-update`
pipeline maps this dir into `stratadb.org/src/content/docs/reference/**`.

---

## Phase 0 — Template + generator scaffold (design lock)

**Goal:** agree the reference-page markdown template and wire an empty
`generate-docs` command end-to-end on one family.

- New `crates/executor/src/idl_tooling/docs.rs`; `generate-docs` + `check-docs`
  arms in `src/bin/strata-idl/main.rs` and `idl_tooling.rs`.
- Hand-write the target markdown for 2 commands (`kv.get`, `inference.generate`)
  as the golden template (frontmatter + body + params + returns + errors).
- Decide: output dir path, filename scheme (`<family>/<op>.md`), index page shape.
- **Deliverable:** `strata-idl generate-docs` emits those 2 pages byte-matching
  the hand-written template. **Verify:** `check-docs` passes on the 2 pages.

## Phase 1 — Reference from prose + schema + errors (the fast win, zero new authoring)

**Goal:** generate the full reference for all ~125 commands from content that
already exists. No examples yet.

- `docs.rs` renders, per command in `command-index.json`:
  - frontmatter from `title`/`summary`/family/`docs`;
  - body from `prose/commands/<id>.md` (+ `prose/snippets/` expansion);
  - **Parameters** table from `generated/schemas/<id>.json` request properties
    (name, type, required, description; `branch`/`space`/`type` → shared scope
    note);
  - **Returns** from `response_model` + response schema;
  - **Errors** from `families.yaml` (family) + `command-index` `errors` (command),
    each linking `/e/<code>`;
  - a per-family index page.
- Generate root `llms.txt` (per §7 spec: H1 + summary + `## Docs` section listing
  every reference page with stable URLs) and `llms-full.txt` (concatenated).
- **Drift guard:** `strata-idl check-docs` in CI (re-generate, diff), alongside
  the existing IDL guards.
- **Coverage guard:** every command in the catalog has a generated page (fail on
  any gap), mirroring the SDK coverage guard.
- **Deliverable:** `generated/docs/**` for all families + `llms.txt`.
  **Verify:** `check-docs` green; coverage guard green; spot-render a family and
  eyeball; no prose duplicated by hand anywhere.

## Phase 2 — Canonical example engine (the multiplier)

**Goal:** one example per command → rendered to CLI + Python, CI-tested, feeding
the reference pages *and* the SDK docstrings *and* the agents guides.

- **Example spec:** add an `example:` block to `prose/commands/<id>.md`
  frontmatter (or `examples/<id>.yaml`) — language-neutral: setup calls, the call
  under test with named args, and expected stable result. Start with the
  high-traffic commands; a `missing-example` allowlist (shrink-only) tracks the
  tail (same pattern as the SDK coverage guard).
- **Renderers (in `docs.rs`, deterministic from command id):**
  - CLI: `strata <family> <op> <args>`;
  - Python: `db.<family>.<op>(<args>)` (naming convention already 1:1 with the SDK
    curated namespaces);
  - wire: the raw `{type,data}` command (optional tab).
- **CI verification of examples (this is the anti-drift core):**
  - Python: extend `strata-python/tests/test_doctests.py` to also run
    *generated* example snippets against a fresh `Strata(cache=True)`.
  - CLI: a golden runner in strata-core executes each rendered CLI example on a
    cache DB and checks the output.
- **Wire-back:** the reference pages gain multi-language example tabs; the SDK
  `Examples:` docstrings become *generated* from the same spec (retiring the
  hand-written doctests from the DX1 slice, keeping their harness).
- **Deliverable:** examples on reference pages + generated SDK docstrings, both
  CI-verified. **Verify:** example CI green in both repos; `check-docs` green.

## Phase 3 — Site integration (stratadb.org)

**Goal:** the generated reference + `llms.txt` are live on the site.

- Extend the `docs-update` sync to bring `generated/docs/**` into
  `src/content/docs/reference/**`; reinstate the `reference` Astro content
  collection (removed as dead schema in site §6) pointing at the real dir.
- Nav + footer "Reference" and "For agents" wired (site §4).
- Serve per-page `.md` twins (append `.md`) and publish `/llms.txt` +
  `/llms-full.txt` from the generated files (site already serves `.md`).
- **Deliverable:** `/docs/<family>/<op>` pages live; `/llms.txt` lists them.
  **Verify:** links resolve (no 404s); `curl <page>.md` returns clean markdown;
  `source:` provenance present.

## Phase 4 — Unify the agent surfaces (close the loop)

**Goal:** one IDL → all agent-facing guides, mutually consistent.

- Generate the SDK `agents_guide()` from the IDL (per-language render of the
  namespace overviews + examples), retiring the hand-written
  `python/stratadb/_data/agent-guide.md` in favor of a generated bundle.
- Generate the site `/docs/getting-started/for-agents` from the same source.
- Keep `strata agents guide` (CLI) as the CLI-flavored render of the same spine.
- **Deliverable:** CLI guide, SDK guide, and site for-agents page are three
  renders of one source. **Verify:** a guide-consistency guard (all three contain
  the same command set); SDK guide-drift guard retargeted to the generated bundle.

---

## Sequencing & risk

- **Ship Phase 1 first** — highest value, lowest risk, no new content authoring
  (everything it needs already exists in prose/schema/errors). It alone gives the
  site a complete, non-drifting reference + `llms.txt`.
- **Phase 2 is the ambitious slice** — the example spec + per-language renderers +
  dual-repo CI. De-risk with the `missing-example` allowlist so partial coverage
  ships. This is where the "multiples" compound.
- **Phase 3** is mostly stratadb.org work (Astro), gated on the `docs-update`
  pipeline; can proceed in parallel with Phase 2 once Phase 1's output exists.
- **Phase 4** is a cleanup/unification once examples (Phase 2) exist.

## Guards summary (all in strata-core CI)

- `check-docs` — reference pages match the IDL (drift).
- docs coverage — every command has a page (gap).
- example coverage — every command has an example or is allowlisted (shrink-only).
- example execution — rendered CLI + Python examples run and match (both repos).

## Definition of done (v1 = Phases 1 + 3)

Every command has a generated, non-drifting reference page live at its
`families.yaml` URL; `/llms.txt` + `.md` twins are published from the IDL; the
strata-core `docs-update` pipeline is the only path prose reaches the site; a
`check-docs` guard prevents drift. Examples (Phase 2) and full agent-surface
unification (Phase 4) follow as distinct, independently shippable slices.
