# V1 Test Coverage Program — Phase 3: Gap Analysis and Layer-by-Layer Plan

**Status:** ACCEPTED (2026-07-17) — co-authored; all six decisions in §5 resolved (D1 b, D2 a, D3 b, D4 a, D5 a, D6 delete).
**Parent:** `v1-test-coverage-program.md` (the charter/ledger — slice status updates land there, not here).
**Inputs:** five per-layer deep-dives (core / storage L1-L9 / engine / executor+IDL / CLI+inference+hub+wasm+stratadb), a workspace product-only coverage measurement, and an error-code assertion cross-cut (all 2026-07-17).

---

## 1. How gaps are tracked (the methodology)

The repo's history is the argument: `v1-progress-tracker.md` froze at M3H, both
test-inventory docs map a deleted tree, and `cli-command-coverage.md` is stale
today (lists shipped verbs as deferred, omits `clone`/`config set`/`agents`).
**Point-in-time audit documents do not track gaps. Guards do.** Phase 3
therefore ships its tracking machinery *first*, and every subsequent slice
registers its surface in a guard, so the gap map can only go stale by failing
CI.

Three tiers:

**Tier 1 — surface guards (the primary tracker).** Per-layer guard tests in
the style of `crates/storage/tests/testing_charter_guard.rs`: enumerate the
real surface mechanically (error codes emitted, `pub fn`s on the boundary,
IDL command index, clap verb tree, render result-type tags, enum variants),
and assert each element is either exercised by a test or listed in a
**shrink-only allowlist entry that carries a reason**. New surface without a
coverage decision fails CI. "Absence is a decision" becomes enforceable.

**Tier 2 — measured coverage ratchets.** `cargo llvm-cov` per crate,
**product-only** (excluding `src/testkit/`, `src/test_support/`, `tests/` —
the current workspace number understates storage by 16 points because test
infrastructure counts as uncovered product code). Nightly job compares
against per-crate floors that only ratchet up. Coverage percent is the
*smoke detector*, not the tracker: it catches wholesale regressions and
points at weak modules, but a green percent never proves a contract is
tested — that's Tier 1's job.

**Tier 3 — the charter ledger.** Human decisions live in
`v1-test-coverage-program.md`: slice status, the deferred register with
re-entry conditions, resolved-not-a-bug records. The ledger is enforced by
the existing charter guard (doc anchors must resolve to real files).

## 2. Measured baseline (2026-07-17)

Product-only line coverage (testkit/test-infra excluded):

| Crate | Line % | Product lines | Verdict |
|---|---|---|---|
| core | 94.7% | 453 | strong; gaps are contract-shaped, not %-shaped |
| inference | 92.0% | 7,873 | strong; residuals deterministic-testable |
| storage | 87.3% | 78,285 | strong; gaps are error-variant + concurrency shaped |
| executor | 85.8% | 11,205 | good; `facade/vector.rs` = 233 lines at 0% |
| engine | 82.9% | 25,994 | vector data plane thin (`service.rs` 45%, `index.rs` 55%) |
| hub | 70.6% | 1,420 | `resolve.rs` 33% |
| gpu-cache | 59.4% | 2,998 | CUDA-gated; hardware lane only |
| cli | 47.3% | 3,654 | weakest testable crate |
| wasm | n/a (host) | 47 | tests run on wasm32; host llvm-cov can't see them |

Cross-cutting error-code measurement: **202 distinct `<class>.<area>.<detail>`
codes in product sources; 134 asserted by at least one test; 68 never
asserted** (53 engine — mostly `data_loss.*`, 11 executor, 4 CLI). At the
engine layer alone the deep-dive counts 160 emitted / 55 asserted (66%
unasserted). Inside storage, three whole inner enums (`CommitError` ~33,
`BranchError` ~41, `TableError` ~11 variants) have **zero** variant
assertions anywhere, because the boundary collapses them to
`LowerLayer { layer, reason: &str }` and rule 29 (no display-text
assertions) leaves tests nothing lawful to assert on.

## 3. Cross-layer findings (what the five deep-dives agree on)

1. **The error surface is the systemic hole.** Every layer independently
   found the same shape: codes/variants defined, emitted, documented — and
   never asserted. Worst where it matters most: `data_loss.*` corruption
   codes are 37 of engine's 105 unasserted codes.
2. **Contract docs have drifted from code.** The engine error registry doc
   uses `<class>.<area>` while code emits `<class>.engine.<detail>`;
   `cli-command-coverage.md` is stale; rule 20 (merge strict-refusal) has
   neither an entrypoint nor a guard asserting its absence.
3. **Existence guards ≠ behavior guards.** The IDL program's four drift
   guards are excellent for *shape* (fixtures vs schema catches field renames
   for every command with a fixture) but only ~110/125 commands replay
   against a live executor, error envelopes are never replayed at all, and
   nothing requires a behavioral test per command (the `branch` family —
   fork/delete — has none).
4. **Concurrency coverage is real-thread-only.** No deterministic scheduler
   anywhere; the DST driver is single-threaded; BS5 commit groups are tested
   sequentially; the L7-mandated lock-order test doesn't exist.
5. **The strongest estates are reference-grade** (storage L3 format, L4
   services, engine vector index lifecycle, inference provider
   serialization) — Phase 3 should not spend there beyond guards.

## 4. Per-layer plans

Slice codes continue the charter convention (`TCP3.{n}`), ordered
core → storage → engine → executor → CLI → edge crates, with the tracking
machinery first. Sizes: S ≈ ≤1 day, M ≈ 1-3 days, L = split before starting.

### TCP3.0 — Tracking machinery (before any layer work)
- Nightly: per-crate **product-only** llvm-cov ratchets (floors: current
  measured values rounded down; ratchet-up rule as in the existing 73%
  workspace gate, which this supersedes).
- Workspace **error-code assertion guard**: enumerate 3-part codes in
  product sources, assert each appears in a test or in a shrink-only
  allowlist with a reason (seeds at 68 entries; every later slice shrinks it).
- Charter Phase 3 section rewritten to point here; stale trackers
  (`cli-command-coverage.md`) either regenerated or superseded-and-marked.

### TCP3.1 — Core (1 slice closes the layer)
Golden wire-format byte fixtures for `BranchId`/`CommitVersion`/`Timestamp`
(both directions, boundary values); adversarial tests for the hand-written
`BranchId` deserializer (`visit_bytes`/`visit_seq`/invalid text, non-ASCII);
Eq/Hash-consistency + map-key properties; `Timestamp` saturating overflow
branch + truncation direction; doc-table↔`public_api.txt` parity guard;
`trybuild` compile-fail locking "no `BranchId::default()`". Size: M total.

### TCP3.2-3.4 — Storage
- **TCP3.2 (L): inner-error assertability** (#2632). Expose a stable
  `kind()`/code discriminant for `CommitError`/`BranchError`/`TableError`
  through the L9 boundary; per-variant boundary-mapping conformance tests;
  variant-reachability guard (constructed in src + referenced in test,
  allowlisted otherwise). This is the storage half of the systemic error
  hole. **Correction (2026-07-17):** the deep-dive's "~13 never-referenced
  `LifecycleError` variants" did NOT survive verification — most have real
  construct/match sites, and the two that looked dead (`ClosePhase`,
  `StorageBudgetPool`) are *field types*, not variant names. No dead-variant
  cleanup is in scope; the original grep conflated types with variants.
- **TCP3.3 (M): decode + fault edges.** L2 object-name/ID codec fuzz targets
  (the one decoder layer with no fuzzer — the layer-fuzz presence guard
  ships here and fails until it's fixed); L1 read/list/metadata fault
  positions in the recovery-scoped fault sweep; L9 negative paths
  (timeline lookups, immutable-source scan, maintenance-drain error arms);
  L9 public-method test-presence guard.
- **TCP3.4 (M-L): concurrency** (#2636). Lock-acquisition-order debug
  assertion + driving test (the L7 mandate; the contract also cites a
  `src/txn/lock_ordering.rs` that no longer exists); threaded L6 families #23/#24 (fork/
  materialize/clear vs writes; pinned views under flush/compaction/clear).
  Per D3: the deterministic multi-actor DST extension is deferred with a
  register entry (re-entry: a concurrency bug these lanes fail to
  reproduce); loom was already rejected for V1 in Phase 2.

### TCP3.5-3.7 — Engine
- **TCP3.5 (M): error registry** (#2633). Engine error-code coverage guard
  (ratchet 55/160); per D2, rewrite the contract doc's `<class>.<area>` registry to
  the emitted 3-part `<class>.engine.<detail>` reality and add the
  doc↔code parity guard; then the testable-now
  refusal batches: graph (~22 codes), vector (~15), json (~10), event (~4).
- **TCP3.6 (M): conformance depth.** Add a fault dimension to
  `capability_conformance` (Read/Scan/Commit fault × 5 capabilities —
  today no test injects a fault mid-capability-operation); generalize the
  temporal timeline property oracle beyond KV (json/graph/event);
  cross-branch rejection extended to vector/json references.
- **TCP3.7 (S): contract truth-ups** (#2635, #2634, #2638). Per D1: amend rule 20 to "merge
  absent in V1" and add the absence guard (no merge/cherry-pick/revert
  entrypoint exists; the guard fails when one appears without its
  strict-refusal tests). Per D6: delete the dead `retention_window`
  contract code (event retention is unimplemented) and the never-emitted
  `conflict.branch_merge/_cherry_pick/_revert` registry rows.
- **TCP3.15 (L, scheduled after TCP3.8): corruption injection** (#2637). Per D5:
  add a corruption fault kind to the persistence testkit (StorageFaultKind
  has none today), then assert the engine `data_loss.*` corruption codes
  (37) class-by-class. Runs last so the cheap wins land first, but it is
  IN Phase 3's exit gate, not deferred.

### TCP3.8-3.9 — Executor + IDL
- **TCP3.8 (M): error-envelope replay.** Extend the fixture format with
  error cases (request that must yield a typed error + pinned `ErrorStatus`
  envelope), teach `verify_case` the expected-error path, drive coverage
  from each command's declared `errors[]`. Closes the highest-risk executor
  gap: today no guard replays a failing command, so a broken error mapping
  ships silently to every SDK. Plus: per-command behavior-test existence
  guard and `replay_skip` shrink ratchet; IDL-driven golden-or-replay
  requirement per command.
- **TCP3.9 (M): hermetic inference lane + missing families.** The
  `GenerationEngine` testkit injection variant (already recorded as the
  natural next increment in Phase 2.5) unlocks converting the 9 inference
  `replay_skip`s into real deterministic replays; `branch_behavior.rs`
  (fork/fork-at-version/fork-at-timestamp/delete at the executor boundary);
  `session_behavior.rs` (omit-vs-explicit branch/space defaults,
  cross-branch rejection). Also covers `facade/vector.rs` (233 lines, 0%).

### TCP3.10-3.11 — CLI (weakest crate, 47.3%)
- **TCP3.10 (S-M): renderers + guards.** Unit tests for every `render.rs`
  tagged result type (~18 of ~20 have none — pure Value→String functions,
  the highest-ROI gap in the repo); render result-type guard; clap-verb
  enumeration guard (every leaf verb in a tested/known-untested set — makes
  the stale coverage doc executable and then deletes it); `config
  set/unset/path/show` write path incl. 0600 permission and env-precedence
  assertions.
- **TCP3.11 (M-L): family coverage.** json/event/graph/space/arrow verb
  families currently have zero CI-gated behavioral tests. Per D4: port the
  un-wired shell corpus (`scripts/cli_next_corpus.sh`) into
  `cli_execution.rs`-style Rust tests. Plus pagination/
  time-travel flags (`--as-of`/`--cursor`/`--prefix`) across families, and
  inference verbs' deterministic non-model paths.

### TCP3.12-3.14 — Edge crates
- **TCP3.12 (S-M) inference:** whole-body request golden snapshots per
  provider (field asserts exist; byte snapshots don't); exhaustive
  `wire.rs` status→error mapping via fake responder; `runtime.rs`
  dispatch/cache/unload via `FakeInferenceEngine`; offline download
  resume/partial-file; BYOK precedence end-to-end.
- **TCP3.13 (M) hub** (#2631, #2630): fault-injecting `HubTransport` fake —
  401/403 auth, truncated/interrupted object with resume, corrupt manifest
  hash mid-clone, retry exhaustion (`clone_faults.rs`); negative cases for
  `default_branch`/`resolve_ref`. (Watch: `CloneError::Transport` is a
  stringly catch-all — the fault tests will likely force typed refinement.)
- **TCP3.14 (S-M) wasm + stratadb:** wasm persistence-absence contract +
  all-6-services-over-`StrataSession` guard + bundle-size budget; stratadb
  feature-flag matrix (cargo hack), facade doctests, pre-V1 layout refusal
  at the facade.

## 5. Decision register (all RESOLVED 2026-07-17, co-authored)

| # | Decision | Resolution |
|---|---|---|
| D1 | Rule 20 merge strict-refusal has no entrypoint and no absence guard | **(b)** amend rule 20 to "merge absent in V1" + absence guard test (TCP3.7); typed-refusal surface lands with post-V1 merge work |
| D2 | Engine error-registry doc format drift (`<class>.<area>` vs emitted 3-part) | **(a)** rewrite doc registry to 3-part — doc follows code; parity guard then enforces it (TCP3.5) |
| D3 | Deterministic multi-actor concurrency (DST driver extension) | **(b)** defer with register entry after TCP3.4's lock-order guard + threaded #23/#24 land; re-entry: a concurrency bug those lanes fail to reproduce |
| D4 | CLI family coverage vehicle | **(a)** port shell corpus scenarios to Rust tests in the `cli_execution.rs` style (TCP3.11); scripts stay as authoring reference |
| D5 | Engine `data_loss.*` corruption injection (37 codes) | **(a)** build the corruption fault kind, scheduled as the final engine slice **TCP3.15** — corruption detectors must not exit V1 unasserted |
| D6 | Dead code found by the dive (13 `LifecycleError` variants, `retention_window`, stale `cli-command-coverage.md`) | **delete** in the owning slice (charter Prefer: deletion over documentation) |

## 5b. Production issues opened by the gap analysis (2026-07-17)

The dive found production changes, not only test gaps. Each was verified
before filing (two candidate findings were falsified and are NOT filed —
see the TCP3.2 correction above, and note that CLI read verbs creating a
database at a *named* path is intended behaviour per
`docs/design/first-run-experience.md`, not a bug).

| Issue | What | Lands with |
|---|---|---|
| #2630 | Hub fabricates `not_found.engine.database` (a code the engine never emits) and creates the DB before reporting it missing | TCP3.13 |
| #2631 | `CloneError::Transport` stringly catch-all — auth/not-found/network indistinguishable, blocks rule-29 assertions + retry decisions | TCP3.13 |
| #2632 | Storage inner error enums have no assertable discriminant at the L9 boundary (~85 variants) | TCP3.2 |
| #2633 | Error contract registry uses 2-part codes; all code emits 3-part | TCP3.5 |
| #2634 | Contract declares four codes nothing emits (retention_window, branch_merge/_revert/_cherry_pick) | TCP3.7 |
| #2635 | Rule 20 promises a merge strict-refusal surface that does not exist (no entrypoint, no test, no absence guard) | TCP3.7 |
| #2636 | L7 contract cites a non-existent file; commit lock order is comment-only, unenforced | TCP3.4 |
| #2637 | 37 engine `data_loss.*` corruption detectors unreachable — no malformed-record injection | TCP3.15 |
| #2638 | Scope decision: event retention windows have no surface but the contract declares a refusal | TCP3.7 |

## 6. Exit criteria for Phase 3

1. Every Tier-1 guard listed above merged and green; all allowlists
   shrink-only with reasons.
2. Workspace error-code assertions: every USER-REACHABLE refusal code is
   asserted; the allowlist holds only defensive-unreachable, decoder-only, or
   fixture-proven codes, each with a reason (D5 resolved: corruption/data_loss
   codes are asserted via TCP3.15, not allowlisted).
   **Amended 2026-09-03 (remediation W0d — "amend + ratchet"):** the original
   "allowlist ≤ 5" was aspirational and never squared with the closeout. The
   allowlist seeded at 68 and has only shrunk (31 today), and every remaining
   entry is defensive-unreachable / decoder-only, not a user-reachable refusal.
   The real exit bar is qualitative — no reachable refusal left unasserted — plus
   the shrink-only per-entry guard (an entry dies the moment its code gains a
   test); there is no fixed count target. Because the allowlist has only ever
   shrunk, a total-count budget (the pattern W0b applied to the replay-debt
   ledgers) is a candidate follow-up, not a current gate.
3. Per-crate product-only ratchet FLOORS live in `scripts/coverage_floors.py`:
   each crate's floor is a hard gate (measured `< floor` fails CI) and moves UP
   only, raised from measurement via the ratchet-up hint — never down.
   **Amended 2026-09-03 (remediation W0d — "amend + ratchet"):** the original
   aspirational targets (cli ≥ 70 / hub ≥ 85 / engine ≥ 88 / executor ≥ 90) are
   retired as hard gates; the criterion is met by the live ratchet, not by a fixed
   percentage the suite never reached. Floors at amendment: core 94.0,
   inference 91.0, storage 86.5, executor 85.0, engine 82.0, hub 70.0,
   gpu-cache 54.0, cli 46.5 (each held ~0.7–1.0 below its measurement as the
   regression margin). Coverage rises by ratcheting the floors up as measurement
   improves — the honest mechanism the phrase "ratchet from measurement, not
   aspiration" always intended.
4. No command family, CLI verb family, or capability without a behavioral
   lane or an allowlist reason.
5. Charter ledger updated per slice; this doc's tables do NOT track status —
   the charter does.

## 7. Raw inputs

Full per-layer deep-dive reports (surface inventories, contract→test maps,
file-level anchors) are preserved in the session working notes and distilled
above; the coverage table and error-code counts are reproducible with:
`cargo llvm-cov --workspace` (filter `src/testkit|/tests/`) and a 3-part
code grep over `crates/*/src` vs test locations.
