# V1 Test-Coverage Audit — Remediation Plan

Companion to the expert audit in this directory (`v1-test-coverage-program-status-audit.md`
+ the four per-phase files). Drafted 2026-09-03 from a structured re-read of all five
documents. Status: **draft for scope sign-off** — see Open Decisions at the end.

## The reframe (what the audit actually says)

The audit is **not** "the test suite has huge holes." Read carefully, the ~2,500 lines sort
into four very different kinds of work, and only two of them are large:

1. **Ledger/doc staleness (the bulk of it, all cheap).** The program header and many slice
   rows describe a paused 2026-08-31 snapshot; the code is ahead. This is not missing
   coverage — it is an untrustworthy status page. Fixing it is fast and is a prerequisite
   for tracking everything else.
2. **A handful of product-defect *pins* (the real reason "Phase 4 can't exit").** 4.7/4.8/4.9
   harnesses correctly *pin* known bugs. Pins are the right pattern but are not closure —
   these need **product fixes then unpin**, not new tests.
3. **Two missing CI/CD lanes.** The charter-promised release-tag soak isn't wired, and the
   mutation-kill plateau is never published as an auditable artifact.
4. **Genuine new coverage to build** — a bounded set of generators/harnesses, plus re-entry
   coverage for surfaces (branch ops, new CLI verbs, hub browse) that shipped *after* their
   owning phase closed.

Everything else is an explicitly accepted deferral (correct, no action).

## Workstreams

Ordered by execution sequence. Effort: S ≈ ≤1 day, M ≈ a few days, L ≈ a slice/epic.

### W0 — Make the ledger honest + stop the bleeding · **do first, cheap, force-multiplier**

Fast work that makes the program status trustworthy and prevents further silent drift. Batch
the doc edits into 1–2 PRs; the two guards are small code.

- **Rewrite the program header + stale row labels** (P0/P2 STALE-DOC). Replace "no slice since
  TCP4.11c / every Phase-4 lane landed" with the audited state; fix stale rows 4.3, 4.4 (#3024
  promoted), 4.5 ("In progress"→closed), 4.2 nightly comment, Phase-3 slice/allowlist counts,
  TCP3.7 branch-merge-absence, and the Phase-1 STH-6/STH-7 prose + nightly coverage-comment
  mislabel, and the Phase-2 2.4 (#2618 fixed), fuzz counts, loom/shuttle-superseded, and
  `storage-next` path. **Also refresh the four simulation/graph READMEs** (4.4/#3024, 4.11/#2828,
  4.12b, 4.5). Effort: **M** (mechanical but broad).
- **Historical debt budgets** (P1). Add max-count / dated-trend budgets to
  `unreplayed-error-codes.yaml` (110) and `replay-skipped-commands.yaml` (12) so CI **rejects
  net growth** unless a debt-budget ledger is updated in the same change (owner + issue +
  planned harness per new entry). This is the root-cause fix for why 3.8b/3.8c drifted. **M.**
- **Machine-readable STH status ledger + status-lint test** (P2 TRUE-GAP). Today's charter
  guard only checks cited files *exist*; add a small structured ledger (slice status, CI job
  names, fuzz-target count, gate type, accepted deferrals) and a lint test over it. Catches
  exactly the drift this audit found. **M.**
- **Split the branch-ops deferred rows** (P1 RE-ENTRY): landed ops (diff/preview/promote) → need
  audited coverage (W3); still-absent (cherry-pick/revert/restore/copy/undo) → stay deferred. **S.**
- **Repair or retire the drifted shell CLI suites** still using `event len`
  (`scripts/cli-tests/08_time_travel.sh` et al) — migrate to `event count` or retire in favor
  of the Rust CLI suite; stop counting them as evidence. **M.**

### W1 — Fix the product-defect pins · **highest correctness value; unblocks Phase-4 exit**

Each is a bounded `/audit-fix` on a real bug; fixing it lets the harness pin become a permanent
contract. Fixes ride the weekly **patch** cadence. Sequence easy→hard:

- **#2749** — `data_loss.*` codes surface public class `corruption` (should be `data_loss`). 4.8. **S–M.**
- **#2750** — feature-disabled codes mis-classed `invalid_argument`/`AfterStateChange`. 4.8. **S–M.**
- **#2754** — snapshot-object absence reported retryable `unavailable` instead of permanent
  corruption. 4.9. **M.**
- **#2567** — recovery memory budget ignored; replace the shrink-only pin with a budget-envelope
  contract. 4.9. **M.**
- **4.7 cross-surface parity — triage the nine divergences** (#2694, #2695, #2700×3, #2701×2,
  #2702, #2704×2): each is either a product fix + unpin **or** a formal "accept as V1 contract"
  decision (needs product judgment per entry). **L.** *Open decision below.*

### W2 — Wire the two missing certification lanes · **the infrastructure behind "not release-grade"**

- **G1 — release-tag pre-release soak** in `release.yml`: run the Phase-4 generated/differential/
  fuzz/DST/history volume across corpora, config matrix, and fault schedules on the tag (folds in
  the 4.5 matrix + 4.6 corpus restore). **M.**
- **G2 — mutation-kill plateau certification ledger** by slice/gap class (mutant set, killed/
  survived/timed-out, equivalent exclusions, date, commit, plateaued?) so a "Phase 4 closed" claim
  is independently auditable. Distinct from the per-PR `--in-diff` gate, which is not plateau
  evidence. **M.**

### W3 — Cover surfaces that landed after their phase · **re-entry; conditions already fired**

Consolidates the Phase-2/3 re-entry findings — these surfaces ship in the product but never got
owning-phase behavior coverage:

- Branch **diff/preview/promote** — audited executor + CLI behavior coverage (3.9b/3.10b/3.11). **M.**
- Newer CLI verbs (agents, hub browse/list/get, branch promotion/status) — behavior lanes, not just
  the inventory guard. **M.**
- **Hub browse/list/get fault-injecting transport lane** (analogous to `clone_faults.rs`); drains
  the replay-skip debt rather than normalizing it (3.13/3.8c). **M.**
- **Cross-version metamorphic harness** — re-entry fired (v1.0.0/v1.1.0/v1.1.1 exist); likely built
  on the 4.2 corpus machinery. **M.**

### W4 — Build the remaining real coverage generators · **the large net-new work; one per release**

Sequence over the release train; each is an epic:

- **4.1b IDL generator completion**: render-mode goldens (json/raw/human), schema-guided boundary +
  adversarial mutation, REPL/pipe/argv text round-trip (#2571), help-text parity (#2569). **L.**
- **Drain the 110 unreplayed error codes** (3.8b / 4.8-replay-debt — the single largest debt):
  closed-runtime, inference provider-error, Arrow, hub, and state/fault setups; extend write-path
  fault fixtures beyond KV; add `EngineErrorClass`→public-class parity sweeps. **L.**
- **4.9 fault families**: durable-table/snapshot fault coverage, health-vs-truth after *runtime*
  faults (not just artifact/reopen), and the continuous randomized crash tier (whitebox FS-op crash
  points + blackbox SIGKILL under sanitizers). **L.**
- **4.12 history checking**: full Adya SSG cycle inference, faulted + pruned concurrent histories,
  explicit event-log ordering/monotonicity checker. **L.**
- **4.2 Strata-only metamorphic tier**: branch/time-travel op-sequences, deep fork DAGs,
  delete/recreate lifecycle churn, KV replay-corpus growth. **L.**
- Medium coverage: **2.5** inference runtime-cache lifecycle (fill→status→unload, hermetic); **2.3/2.6**
  CLI clone-over-real-HTTP at the binary layer; **3.14** wasm bundle-size budget gate; **1.2 STH-5**
  quarantine compound-fault (only if the scope decision says quarantine was an exit surface). **M each.**

### W5 — Blocked on a product observable · **schedule after the feature exists**

- **4.10 inert-index detection + QPG** needs a product explain/stats observable that doesn't exist
  yet. Treat as blocked; the observable is itself a user-facing feature worth its own scoping. **L.**

### W6 — Phase 5 perf/trend buildout · **separate, lower-urgency track**

5.3 same-runner A/B relative gate (advisory first), 5.4 nightly macro-trend artifacts + drift
alarms, 5.5 release-leg comparative/GPU, 5.6 characteristic metrics + allocation-count ratchet.
**M–L each.**

## Explicitly NOT in scope (accepted deferrals — document as intentional)

STH-1/2/4 foundation deferrals; 2.1 power-loss-beyond-page-cache boundary; 2.7 per-branch orphan
recovery (until multi-branch durable maintenance begins, L); 3.4b deterministic multi-actor
scheduler; 3.5/3.15 defensive/decoder-only allowlist entries; 4.3 shuttle/whole-runtime exploration;
4.6 (strong as-is). Keep these visible as *accepted*, not silently dropped.

## Recommended sequencing (Now / Next / Later)

- **Now (next 1–2 weekly releases):** all of **W0** (cheap, unblocks tracking + stops drift), and
  begin **W1** — land #2749/#2750/#2754/#2567 as patch-cadence bug fixes.
- **Next (this quarter):** finish **W1** (the 4.7 triage), **W2** (both cert lanes), **W3** (re-entry
  coverage). At this point the "release-grade" claim is defensible.
- **Later (rolling, one epic per release):** **W4** generators, then **W5** once the explain observable
  lands, then **W6** Phase 5.

## Decisions (settled 2026-09-03)

1. **Ambition → W0+W1 now, then reassess.** Land the honest-ledger cleanup and fix the
   product-defect pins first (real bugs, and they unblock the exit claim); decide certification
   depth (W2/W4 breadth) once the ledger is honest and the bugs are down. W0+W1 are the committed
   near-term scope; W2/W3/W4/W6 are planned but not yet committed.
2. **Phase-3 exit criteria → amend + ratchet.** Reconcile the criteria to the real closeout and
   convert the coverage targets to **upward-only ratchets**, not hard gates. This folds into W0
   (it is a doc/gate reconciliation, not new test work) — no cli-46.5→70 crash program.
3. **4.7 divergences → triage each fix-vs-accept.** W1 includes a triage pass over all nine
   (#2694/#2695/#2700×3/#2701×2/#2702/#2704×2) producing a per-entry recommendation: product fix +
   unpin, or formally accept as V1 contract (with the acceptance recorded in the divergence ledger).
   The triage is the first W1-4.7 deliverable; the resulting fixes/acceptances follow.
