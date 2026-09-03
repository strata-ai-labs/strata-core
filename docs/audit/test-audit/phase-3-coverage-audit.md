# Phase 3 Coverage Audit

Audit date: 2026-09-02

Worktree: `strata-core-test-coverage-audit`

Base reviewed: `origin/main` at `8f0bf36e`

Source log: `docs/architecture/v1-test-coverage-program.md`

Accepted plan: `docs/architecture/v1-test-coverage-phase3-plan.md`

## Executive Verdict

Phase 3 is substantially implemented. The codebase has real Phase 3 artifacts:
workspace error-code guarding, per-crate product-only coverage floors, IDL
error-envelope replay, command replay-skip ratchets, storage method/fuzz
presence guards, engine capability/property/refusal suites, real-binary CLI
coverage, wasm CI, and focused edge-crate tests. Most individual implementation
slices match the scope they were meant to cover.

The phase is not cleanly certifiable as closed against the accepted plan without
a cleanup pass. The accepted exit criteria and the final ledger disagree in
material ways:

1. The accepted plan required the workspace never-asserted error-code debt to
   drop from 68 to an allowlist of at most 5 entries. The current guard has 31
   allowlisted entries.
2. The accepted plan required product-only coverage ratchets at cli >= 70%,
   hub >= 85%, engine >= 88%, and executor >= 90%. The current floors are near
   the original baseline: cli 46.5%, hub 70.0%, engine 82.0%, executor 85.0%.
3. The "shrink-only" debt lists are locally enforced at a commit, but they have
   grown since the Phase 3 close point: unreplayed error codes are 110 now vs
   105 at `5ceac5ed`, and replay-skipped commands are 12 now vs 7 at
   `5ceac5ed`.
4. The Phase 3 program log is stale in several places. It says the final
   allowlist had 32 entries, but the current guard has 31. It records branch
   merge as absent, but `branch.diff`, `branch.merge`, and `branch.preview`
   now exist in the IDL.
5. TCP3.14 included a wasm bundle-size budget in the accepted scope, but I
   found no implemented budget gate or size ledger.

The practical conclusion: treat Phase 3 as "implemented with certification
cleanup required." Most test slices do not need to be redone. The work to split
out now is ledger/criteria reconciliation, drift-budget enforcement, replay
debt reduction, and one missing wasm budget lane.

## Phase 3 Gate Findings

### P0 - Accepted exit criteria were not met or were silently relaxed

Scope: The accepted plan says Phase 3 exits only after Tier-1 guards are green,
the workspace unasserted-code count is reduced to an allowlist of at most 5,
per-crate product-only ratchets hit cli >= 70%, hub >= 85%, engine >= 88%, and
executor >= 90%, every surface has a behavior lane or allowlist reason, and the
charter ledger is updated (`docs/architecture/v1-test-coverage-phase3-plan.md:245`).

Current state:

- `crates/storage/tests/error_code_assertion_guard.rs:81` still has 31
  `ALLOWED_UNASSERTED` entries.
- `scripts/coverage_floors.py:26` sets current floors at baseline levels, with
  cli 46.5, hub 70.0, engine 82.0, and executor 85.0
  (`scripts/coverage_floors.py:30`, `scripts/coverage_floors.py:31`,
  `scripts/coverage_floors.py:32`, `scripts/coverage_floors.py:40`).
- The same script comments say the floors were set from the 2026-07-17 baseline
  and are runner-baseline floors, not the higher exit targets
  (`scripts/coverage_floors.py:22`).

Verdict: The implementation correctly created the machinery, but the numeric
exit criteria were not satisfied as written. Either the criteria need to be
amended with an explicit Phase 3 closeout rationale, or Phase 3 should retain a
follow-up certification item until those thresholds are achieved.

Fix split:

- TCP3.GATE-A: reconcile the accepted exit criteria with the actual closeout.
- TCP3.GATE-B: decide whether the <= 5 allowlist target is still active or has
  been replaced by "defensive/unreachable or fixture-proven only."
- TCP3.GATE-C: decide whether the coverage targets are still active, moved to a
  later phase, or replaced by baseline ratchets.

### P1 - Shrink-only ledgers are commit-local, not program-historical

Scope: Phase 3 intended allowlists to be shrink-only ratchets. The error replay
guard says every declared code must be replayed or listed, and listed entries
must be removed once replayed (`crates/executor/src/idl_tooling.rs:1651`,
`crates/executor/src/idl_tooling.rs:1706`). The replay-skip guard similarly
requires every skipped command to be listed and stale entries to be removed
(`crates/executor/src/idl_tooling.rs:1483`).

Current state:

- Current unreplayed error codes: 110 entries in
  `crates/executor/idl/v1/unreplayed-error-codes.yaml:21`.
- Phase 3 close point (`5ceac5ed`) unreplayed error codes: 105 entries.
- Current replay-skipped commands: 12 entries in
  `crates/executor/idl/v1/replay-skipped-commands.yaml:18`.
- Phase 3 close point (`5ceac5ed`) replay-skipped commands: 7 entries.

Verdict: The guards prevent stale entries and unlisted current drift, but they
do not enforce a historical monotonic debt budget. New justified entries can be
added, which is sometimes correct, but the program should not call that
"shrink-only" without also tracking budget growth.

Fix split:

- Add max-count budgets or dated trend ledgers for `unreplayed-error-codes.yaml`
  and `replay-skipped-commands.yaml`.
- Require an owner, issue/slice, and planned harness for each new entry.
- Add a CI check that rejects growth unless the same change updates an explicit
  debt-budget ledger.

### P1 - Phase 3 docs are stale against current product surface

Scope: The program log is the status ledger. It currently says Phase 3 planned
16 slices from TCP3.0 to TCP3.15 (`docs/architecture/v1-test-coverage-program.md:167`),
then later says all 15 slices were implemented and the allowlist had 32 entries
(`docs/architecture/v1-test-coverage-program.md:225`).

Current state:

- TCP3.16 exists as an addendum (`docs/architecture/v1-test-coverage-program.md:236`).
- The current Rust error-code allowlist count is 31, not 32
  (`crates/storage/tests/error_code_assertion_guard.rs:81`).
- TCP3.7's row describes merge/revert/cherry-pick as absent
  (`docs/architecture/v1-test-coverage-program.md:198`), but current IDL
  includes `branch.diff`, `branch.merge`, and `branch.preview`
  (`crates/executor/idl/v1/commands/branch.yaml:32`,
  `crates/executor/idl/v1/commands/branch.yaml:46`,
  `crates/executor/idl/v1/commands/branch.yaml:75`).
- The current absence guard has been correctly narrowed to only
  cherry-pick/revert tokens after preview and merge landed
  (`crates/engine/tests/branch_merge_absence.rs:1`,
  `crates/engine/tests/branch_merge_absence.rs:8`,
  `crates/engine/tests/branch_merge_absence.rs:24`).

Verdict: The code adaptation looks correct; the program ledger did not keep up.
The Phase 3 audit status should supersede the earlier broad audit's Phase 3
summary until the main ledger is corrected.

Fix split:

- Update the Phase 3 closeout count and slice count.
- Rewrite TCP3.7's status to say the guard was narrowed after branch preview and
  promote landed, and now protects only cherry-pick/revert.
- Cross-link this focused audit from the broad program audit.

### P2 - Several Tier-1 guards are intentionally coarse

Scope: Phase 3 made "no silent drift" executable. It did not make every guard a
semantic proof.

Current state:

- The workspace error-code guard is deliberately "grep-grade" and counts
  doc-comment mentions as assertions (`crates/storage/tests/error_code_assertion_guard.rs:15`).
- The storage L9 method guard checks that a method name is referenced somewhere
  in the test tree, not that the reference is deep behavior coverage
  (`crates/storage/tests/l9_method_presence_guard.rs:11`).
- The layer fuzz guard checks filename prefixes and layer representation, not
  target quality (`crates/storage/tests/layer_fuzz_presence_guard.rs:10`).

Verdict: This is acceptable for Phase 3's tracking goal, but any "world class"
certification should keep these guards in the right box. They are inventory
guards. Phase 4-style mutation, generated, differential, and model checks are
the evidence of behavioral depth.

Fix split:

- Replace comment-counted error-code "assertions" with structured assertions or
  machine-readable fixture metadata where feasible.
- Add mutation probes or generated tests for high-risk surfaces currently
  protected only by presence guards.
- Keep the coarse guards as cheap drift tripwires even after stronger lanes
  exist.

## Slice Audit

The table below normalizes every logged Phase 3 row. The narrative sections
that follow group closely related sub-slices where the same evidence and caveats
apply.

| Slice | Audit status | Notes |
|---|---|---|
| 3.0 | Implemented with certification caveat | Tracking machinery landed, but the accepted numeric exit targets were not reconciled with current floors/allowlist size. |
| 3.1 | Closed | Core durable atoms, adversarial decode, hash/boundary, API doc parity, and default-prevention coverage are in place. |
| 3.2a | Closed | Storage boundary/branch inner-code plumbing landed. |
| 3.2b | Closed | Commit runtime/lower-layer code exhaustiveness landed. |
| 3.2c | Closed | Table/lifecycle code plumbing and construct-every-variant coverage landed. |
| 3.3a | Closed with guard-depth caveat | L2 codec fuzz targets and layer-fuzz presence guard landed; guard proves representation, not fuzz quality. |
| 3.3b | Closed | Recovery read/list/metadata fault sweep landed. |
| 3.3c | Closed with guard-depth caveat | L9 negatives and method-presence guard landed; guard proves references, not semantic depth. |
| 3.3d | Closed | Lifecycle residual error-code coverage landed. |
| 3.4a | Closed | Commit lock-order enforcement landed. |
| 3.4b | Closed with explicit deferral | Threaded COW race tests landed; deterministic scheduler remains deferred by D3. |
| 3.5a | Closed | Error contract reconciliation and class-parity guard landed. |
| 3.5b | Closed with allowlist caveat | Graph reachable refusals landed; defensive/unreachable entries remain allowlisted. |
| 3.5c | Closed with allowlist caveat | Vector/JSON reachable refusals landed; defensive/unreachable entries remain allowlisted. |
| 3.5d | Closed with allowlist caveat | Event/space reachable refusals landed; defensive/unreachable entries remain allowlisted. |
| 3.6a | Closed | Capability fault dimension landed. |
| 3.6b-i | Closed | Keyed mutable temporal oracle landed for KV/JSON/graph. |
| 3.6b-ii | Closed | Append-only event timeline oracle landed. |
| 3.6c | Closed as N/A | Vector/JSON cross-branch reference surface does not exist. |
| 3.7 | Implemented/adapted; docs stale | Absence guard now correctly protects only cherry-pick/revert after preview/promote landed. |
| 3.8a | Closed | Error-case fixture schema and replay path landed. |
| 3.8b | Implemented with high residual debt | Replay coverage guard landed, but 110 declared codes remain unreplayed. |
| 3.8c | Implemented with growing debt | Replay-skip ratchet landed, but skipped commands grew from 7 at Phase 3 close to 12 now. |
| 3.9a | Closed | Vector facade coverage landed. |
| 3.9b | Closed for original scope | Branch/session behavior landed; later branch promotion surfaces need separate ownership. |
| 3.9c-i | Closed | Runtime-level inference fake/service seam landed. |
| 3.9c-ii | Closed | Inference replay skips were converted to deterministic fake replays. |
| 3.10a | Closed | Render helper tests landed. |
| 3.10b | Closed with inventory caveat | Clap verb and render tag guards landed; guard is inventory, not behavior for every verb. |
| 3.10c | Closed | Config write-path tests landed; doctor moved to 3.16 addendum. |
| 3.11a | Closed for original scope | JSON/space real-binary coverage landed. |
| 3.11b | Closed for original scope | Event/graph real-binary coverage landed. |
| 3.11c | Closed for original scope | Arrow/pipe/raw/command/inference non-model coverage landed. |
| 3.12 | Implemented with documented residuals | Whole-body inference request goldens landed; some lower-value parser/error text residuals remain deferred. |
| 3.13 | Closed for clone scope | Hub clone/fault coverage landed; newer hub browse commands need follow-up. |
| 3.14 | Partially closed | wasm and stratadb behavior tests landed; wasm bundle-size budget is missing. |
| 3.15a | Closed | Corruption infra plus KV/JSON public-path assertions landed. |
| 3.15b | Closed | Graph public-path corruption assertions landed. |
| 3.15c | Closed with classification caveat | Vector corruption/reopen/IO assertions landed, partly decoder-only by design. |
| 3.15d | Closed with certification caveat | Control-plane corruption and exit-gate truing landed, but the accepted numeric gate remains unreconciled. |
| 3.16 | Closed | CLI doctor addendum landed. |

### 3.0 - Tracking Machinery

Scope: Install the Phase 3 tracking system first: workspace error-code assertion
guard, per-crate product-only coverage floors, charter ledger updates, and
replacement of stale manual trackers
(`docs/architecture/v1-test-coverage-phase3-plan.md:98`).

Implemented: Yes. The workspace error-code guard exists and scans product
source vs test locations (`crates/storage/tests/error_code_assertion_guard.rs:1`).
The nightly coverage job runs `cargo llvm-cov` and `scripts/coverage_floors.py`
(`.github/workflows/nightly.yml:271`). CI also gates workspace tests, feature
matrix checks, and IDL gates (`.github/workflows/ci.yml:53`).

Correctness verdict: Mechanically correct as a tracking layer. Certification is
not clean because the current floors are baseline floors, not the accepted exit
targets, and the current error-code allowlist remains far above the accepted
<= 5 target.

More to cover:

- Add explicit historical budget checks for allowed-unasserted, unreplayed, and
  replay-skipped ledgers.
- Formalize whether JSON fixture assertions are first-class assertions for the
  workspace error-code guard, because several executor entries remain
  allowlisted only because the Rust grep guard does not parse fixture JSON.

Status: Implemented with certification caveat.

### 3.1 - Core

Scope: Close the core crate with durable atom goldens, adversarial `BranchId`
deserialization, hash/boundary properties, doc/public API parity, and no
`BranchId::default()` leakage
(`docs/architecture/v1-test-coverage-phase3-plan.md:108`).

Implemented: Yes. `wire_goldens.rs` pins JSON and bincode encodings for
`BranchId`, `CommitVersion`, and `Timestamp`; `adversarial_decode.rs` drives
invalid text, non-string, byte, and sequence deserialization paths;
`hash_and_boundary.rs` covers Eq/Hash and timestamp boundary behavior; and
`doc_parity_guard.rs` checks documented public API boundaries.

Correctness verdict: Correct for Phase 3. This is one of the cleanest slices:
the tests are close to the durable contract and not just round-trips.

More to cover:

- No current Phase 3 gap found. Revisit only when new core atoms or wire formats
  are introduced.

Status: Closed.

### 3.2 - Storage Inner-Error Assertability

Scope: Make storage lower-layer failures assertable through the L9 boundary by
surfacing stable inner codes for branch, commit, table, and lifecycle runtime
errors (`docs/architecture/v1-test-coverage-phase3-plan.md:116`).

Implemented: Yes.

- 3.2a added/used `inner_code` at the boundary and corrected the guard's storage
  area coverage, closing a blind spot where storage layer areas were invisible
  to the workspace scan.
- 3.2b added exhaustive `CommitRuntimeError::code()` and lower-layer code
  routing.
- 3.2c added table/lifecycle wiring and construct-every-variant tests for the
  inner enums.

Evidence includes boundary contract tests that assert lower-layer code
preservation and `inner_code` specificity (`crates/storage/tests/api_error_contract.rs:361`,
`crates/storage/tests/api_error_contract.rs:401`) and exhaustive runtime error
code functions such as `CommitRuntimeError::code`
(`crates/storage/src/commit/error.rs:145`).

Correctness verdict: Correct. The implementation chose stable discriminants and
literal code assertions, which is the right solution under rule 29 because
display text should not be asserted.

More to cover:

- Extend the same discriminant/construct-every-variant pattern to any new
  storage inner enum added later.
- Keep checking class/code agreement when lower-layer classes are renamed.

Status: Closed.

### 3.3 - Storage Decode, Fault, and L9 Negative Paths

Scope: Add L2 object-name/id codec fuzz targets, a layer-fuzz presence guard,
recovery read/list/metadata fault sweeps, L9 negative path assertions, and an
L9 method-presence guard (`docs/architecture/v1-test-coverage-phase3-plan.md:127`).

Implemented: Yes.

- 3.3a added L2 fuzz targets and the layer presence guard
  (`crates/storage/tests/layer_fuzz_presence_guard.rs:1`).
- 3.3b added the recovery read fault harness; its invariant is that recovery
  faults must fail loudly or degrade, never open silently healthy
  (`crates/storage/src/testkit/recovery_read_faults.rs:1`).
- 3.3c added L9 negative-path tests and method-presence guarding
  (`crates/storage/tests/l9_method_presence_guard.rs:1`).
- 3.3d drained storage lifecycle inner-error allowlist debt with construct-all
  lifecycle tests.

Correctness verdict: Correct for Phase 3. The fault and error-code tests are
valuable. The fuzz and method guards are intentionally coarse but useful as
drift tripwires.

More to cover:

- Measure fuzz target quality by corpus coverage or sanitizer/crash history,
  not only prefix presence.
- Add stronger semantic checks for any L9 method currently only "referenced" by
  the method guard.

Status: Closed, with guard-depth caveat.

### 3.4 - Storage Concurrency

Scope: Add debug lock-order enforcement and threaded L6 copy-on-write race
coverage, while deferring deterministic multi-actor scheduling per D3
(`docs/architecture/v1-test-coverage-phase3-plan.md:133`).

Implemented: Yes. `commit/lock_order.rs` implements debug lock-rank tracking
and tests (`crates/storage/src/commit/lock_order.rs:1`). Commit guard code uses
the lock-order guard around actual mutex acquisitions. Threaded COW tests cover
fork snapshots racing writers and pinned reads racing flush/compaction in
`off_lock_concurrency.rs`.

Correctness verdict: Correct for the accepted Phase 3 scope. The deferral of a
deterministic scheduler was explicit, not an accidental miss.

More to cover:

- The deterministic multi-actor DST lane remains the right follow-up when a
  concurrency bug escapes the real-thread tests or when Phase 4/5 wants repeatable
  interleaving search.

Status: Closed with explicit deferral.

### 3.5 - Engine Error Registry and Refusal Batches

Scope: Reconcile the engine error contract with emitted 3-part codes, add
doc/code class parity, and assert reachable graph/vector/json/event/space
refusal codes (`docs/architecture/v1-test-coverage-phase3-plan.md:141`).

Implemented: Yes. `error_contract_class_parity_guard.rs` ties documented error
classes to the tracked guard classes. Refusal batches exist for graph, JSON,
vector, event, and space (`crates/engine/tests/engine_graph_refusals.rs:1`,
`crates/engine/tests/engine_json_vector_refusals.rs:1`,
`crates/engine/tests/engine_event_space_refusals.rs:1`).

Correctness verdict: Correct for reachable user-facing refusals. The remaining
engine entries in `ALLOWED_UNASSERTED` are mostly defensive encode arms,
internal invariants, or verified unreachable code paths
(`crates/storage/tests/error_code_assertion_guard.rs:83`).

More to cover:

- The allowlist should be audited periodically because "unreachable today" can
  become reachable after new public surfaces land.
- Consider a generated reachability ledger that distinguishes
  user-reachable, decoder-only, fixture-proven, and defensive-unreachable.

Status: Closed with allowlist-maintenance caveat.

### 3.6 - Engine Conformance Depth

Scope: Extend engine capability conformance with storage fault dimensions,
generalize temporal oracles beyond KV, and investigate vector/json cross-branch
rejection (`docs/architecture/v1-test-coverage-phase3-plan.md:147`).

Implemented: Yes.

- 3.6a added read/scan/commit fault coverage across five capabilities in
  `capability_conformance.rs`.
- 3.6b generalized temporal timeline properties for KV/JSON/graph and added a
  separate append-only event timeline model.
- 3.6c correctly closed as N/A because vector and JSON have no user-facing
  cross-branch reference field.

Correctness verdict: Correct. The implementation improved shared conformance
rather than adding one-off tests. The 3.6c N/A conclusion is defensible because
the surface it planned to reject does not exist.

More to cover:

- Add analogous conformance when newer branch promotion surfaces require it.
- Keep event-specific temporal properties separate from keyed mutable
  capabilities.

Status: Closed.

### 3.7 - Engine Contract Truth-Ups

Scope: Amend the V1 branch merge contract to match reality, add an absence guard
for absent branch promotion operations, and delete dead event-retention/branch
merge error rows (`docs/architecture/v1-test-coverage-phase3-plan.md:152`).

Implemented: Yes, but the implementation has since been adapted. The original
guard asserted merge/revert/cherry-pick absence. Current code correctly narrows
that guard to only cherry-pick and revert because read-only preview and mutating
promote have since landed (`crates/engine/tests/branch_merge_absence.rs:8`).
The current IDL has `branch.diff`, `branch.merge`, and `branch.preview`
(`crates/executor/idl/v1/commands/branch.yaml:32`,
`crates/executor/idl/v1/commands/branch.yaml:46`,
`crates/executor/idl/v1/commands/branch.yaml:75`).

Correctness verdict: Current code is correct for the current surface. The
program log is stale because it still describes branch merge as absent
(`docs/architecture/v1-test-coverage-program.md:198`).

More to cover:

- Update the Phase 3 ledger to say the absence guard now protects only
  cherry-pick/revert.
- Audit branch merge/preview behavior under the later phase that introduced
  them; do not leave them implied by the old absence-surface story.

Status: Implemented/adapted; documentation stale.

### 3.8 - Executor and IDL Error-Envelope Replay

Scope: Extend fixtures with error cases, replay expected error envelopes against
a scratch executor, enforce declared-vs-replayed error coverage, and add a
replay-skip ratchet (`docs/architecture/v1-test-coverage-phase3-plan.md:164`).

Implemented: Yes. The replay enforcement requires every replayed error to be
declared by its command and every declared error to be replayed or listed
(`crates/executor/src/idl_tooling.rs:1651`). The replay-skip ratchet requires
every skipped command to be listed and stale entries to be removed
(`crates/executor/src/idl_tooling.rs:1483`).

Correctness verdict: The machinery is correct. The assurance debt remains
large: `unreplayed-error-codes.yaml` currently lists 110 declared codes without
error-case replay fixtures (`crates/executor/idl/v1/unreplayed-error-codes.yaml:21`),
and `replay-skipped-commands.yaml` lists 12 commands whose primary fixtures are
not replayed (`crates/executor/idl/v1/replay-skipped-commands.yaml:18`).

More to cover:

- Closed runtime harnesses for `runtime_closed`.
- Live or high-fidelity fake provider lanes for inference provider errors.
- Arrow feature/file/IO harnesses.
- Configured hub/browse/list/get harnesses.
- State/fault setup for graph ontology, graph analytics, JSON/vector boundary,
  branch, and space failures still in the unreplayed list.

Status: Implemented, but high residual replay debt.

### 3.9 - Executor Hermetic Inference, Vector Facade, Branch, and Session

Scope: Cover the vector facade, branch/session behavior, and executor-level
inference through a deterministic fake service
(`docs/architecture/v1-test-coverage-phase3-plan.md:173`).

Implemented: Yes. `vector_facade_behavior.rs` exercises all 19 vector
convenience methods against explicit command execution. `branch_behavior.rs`
and `session_behavior.rs` cover branch lifecycle, fork-as-of behavior, default
branch/space resolution, and missing branch errors. `inference_hermetic_behavior.rs`
uses the `FakeInferenceService` from `crates/inference/src/testkit.rs`.

Correctness verdict: Correct for hermetic executor coverage. The fake is the
right tool for deterministic CI and avoids network/model dependence.

More to cover:

- Live provider behavior is intentionally outside this slice.
- Branch merge/preview landed after the original branch behavior slice; those
  need to be covered in their owning later phase or added as a follow-up.

Status: Closed for original scope.

### 3.10 - CLI Renderers, Config, and Surface Guards

Scope: Add render helper tests, config write-path tests, a render result-type
guard, and a clap leaf-verb guard (`docs/architecture/v1-test-coverage-phase3-plan.md:181`).

Implemented: Yes. `render.rs` has a `RENDERED_TAGS` inventory and dispatch-arm
guard (`crates/cli/src/render.rs:912`, `crates/cli/src/render.rs:968`).
`options.rs` walks the clap tree and compares it to the maintained
`EXPECTED_VERBS` list (`crates/cli/src/options.rs:1986`,
`crates/cli/src/options.rs:2013`). `config_behavior.rs` covers config set/show/
unset/path, env precedence, permissions, redaction, and unknown keys.

Correctness verdict: Correct as an inventory and rendering/config coverage
slice. The current verb list has grown beyond the original Phase 3 families,
including agents, branch merge/preview/tag/note, and hub browse commands
(`crates/cli/src/options.rs:2018`).

More to cover:

- The clap guard proves inventory maintenance, not behavior coverage for every
  verb.
- Newer verbs should have an owning behavior lane or explicit allowlist reason.

Status: Closed for original scope; newer verbs need later-phase ownership.

### 3.11 - CLI Family Coverage

Scope: Port json/event/graph/space/arrow shell-corpus workflows into real-binary
Rust tests, including pagination, time-travel flags, pipe/raw mode, and
structured error envelopes (`docs/architecture/v1-test-coverage-phase3-plan.md:189`).

Implemented: Yes. `json_space_behavior.rs`, `event_graph_behavior.rs`,
`arrow_pipe_behavior.rs`, and `inference_verb_behavior.rs` drive the real binary
against temp databases and hermetic environments.

Correctness verdict: Correct for the families named in the slice. It covers
real stdout/stderr behavior, not just parser internals.

More to cover:

- New command families or large new verbs introduced after Phase 3 should not
  inherit "3.11 covered every CLI family" without additional tests. Current
  candidates include agents, hub browse/list/get, and branch promotion/status
  surfaces.

Status: Closed for original scope.

### 3.12 - Inference Deterministic Residuals

Scope: Add whole-body provider request goldens, status-to-error wire mapping,
runtime dispatch/cache/unload via fake inference, offline download resume, and
BYOK precedence (`docs/architecture/v1-test-coverage-phase3-plan.md:196`).

Implemented: Mostly yes for deterministic provider request/wire shape. The
program log records whole-body JSON goldens across providers and explicit
deferred residuals for low-value `map_http_error` string branches and malformed
`parse_tool_calls`/`parse_logprobs` inputs
(`docs/architecture/v1-test-coverage-program.md:216`).

Correctness verdict: Correct for whole-request shape drift and provider option
parity. Some originally listed residuals appear to have been narrowed during
implementation rather than exhaustively completed.

More to cover:

- Decide whether offline download resume and BYOK precedence are covered in a
  different slice or should be explicitly deferred in the Phase 3 ledger.
- Keep live provider behavior separate from hermetic wire-shape tests.

Status: Implemented with documented residuals.

### 3.13 - Hub Transport Fault Injection

Scope: Add fault-injecting `HubTransport` tests for clone failures, auth/not
found/network distinctions, interrupted/corrupt object flows, retry exhaustion,
and negative default branch/ref resolution
(`docs/architecture/v1-test-coverage-phase3-plan.md:202`).

Implemented: Yes for clone fault injection. The program log records
`clone_faults.rs`, fault short-circuit counting, no-destination-state checks,
corrupt object bytes, malformed engine requirements, and executor envelope
coverage for clone compatibility/non-database paths
(`docs/architecture/v1-test-coverage-program.md:217`).

Correctness verdict: Correct for the original clone-centered hub scope.

More to cover:

- Current hub browse/list/get commands are now in the replay-skip allowlist
  (`crates/executor/idl/v1/replay-skipped-commands.yaml:26`). They need their
  own deterministic transport lane and should not be considered covered by the
  original clone-only tests.
- Add historical debt budgeting so hub replay skips do not silently become the
  new normal.

Status: Closed for clone scope; newer hub commands need follow-up.

### 3.14 - wasm and stratadb Residuals

Scope: Add wasm persistence-absence behavior, all-six-services-over
`StrataSession`, a wasm bundle-size budget, and stratadb facade residuals
(`docs/architecture/v1-test-coverage-phase3-plan.md:207`).

Implemented: Partially. wasm session tests cover branch/space scoping, invalid
space, closed session behavior, and `engine_version()`; CI runs them on
`wasm32-unknown-unknown` through `wasm-bindgen-test-runner`
(`crates/wasm/tests/session.rs:1`, `.github/workflows/ci.yml:286`,
`.github/workflows/ci.yml:303`). `crates/stratadb/tests/facade.rs` covers the
re-exported facade, fork isolation, stable errors, and time travel.

Correctness verdict: Correct for runtime behavior. I found no implemented wasm
bundle-size budget; searches for bundle-size/wasm-budget only found the Phase 3
plan entry and release packaging of `strata-wasm-web.tar.gz`.

More to cover:

- Add a CI or release gate that builds the wasm bundle and enforces a documented
  compressed and/or raw `.wasm` size budget.
- Record the budget, allowed growth policy, and exception process in the Phase 3
  ledger or release docs.

Status: Partially closed; behavior covered, bundle-size budget missing.

### 3.15 - Engine Corruption Injection

Scope: Add a row-corruption seam and assert engine `data_loss.*` corruption
codes class-by-class, including KV/JSON, graph, vector/reopen/IO, and control
plane (`docs/architecture/v1-test-coverage-phase3-plan.md:158`).

Implemented: Yes, with justified narrower paths for some vector/control codes.
`corruption_injection.rs` drives KV/JSON and graph corruption through public
read/scan paths (`crates/engine/tests/corruption_injection.rs:1`,
`crates/engine/tests/corruption_injection.rs:27`,
`crates/engine/tests/corruption_injection.rs:55`,
`crates/engine/tests/corruption_injection.rs:108`). Vector record corruption is
asserted at decoder level (`crates/engine/src/data/vector/record.rs:275`).
Vector artifact corruption is also decoder-level because runtime behavior
converts corrupt derived state into a corrupt load status rather than surfacing
the code to callers (`crates/engine/src/data/vector/artifact.rs:2048`).

Correctness verdict: Correct. The slice made a real distinction between
public-path corruption, decoder-only corruption, and genuinely unreachable
defensive arms.

More to cover:

- Preserve the distinction in the allowlist: user-path, decoder-only,
  diagnostic-status-only, and unreachable should stay separate.
- Re-audit "unreachable" entries when new open/bootstrap, branch, or vector
  artifact surfaces land.

Status: Closed with classification caveat.

### 3.16 - CLI Doctor Addendum

Scope: Close the follow-up discovery that `doctor.rs` had reachable, hermetic
error codes but no tests (`docs/architecture/v1-test-coverage-program.md:236`).

Implemented: Yes. `doctor_behavior.rs` exercises the healthy path and four
environment-driven failures: missing database, non-directory `STRATA_HOME`,
unresolvable home, and binary off `PATH`.

Correctness verdict: Correct. This was a good example of Phase 3 working as an
audit program rather than accepting a too-generous deferral.

More to cover:

- None for the addendum's original scope.

Status: Closed.

## Recommended Work Split

1. Phase 3 certification cleanup: update the Phase 3 ledger, slice count,
   closeout count, TCP3.7 branch status, and exit-criteria language.
2. Numeric debt decision: either reactivate the original <= 5 allowlist and
   coverage target thresholds, or explicitly move them into later-phase debt
   with owners.
3. Replay debt lane: reduce the 110 unreplayed error codes and 12 skipped
   commands, starting with hub browse commands, Arrow feature/file paths, and
   closed-runtime harnesses.
4. Historical debt budgets: add max-count or trend-ledger checks so
   "shrink-only" remains true over time, not just within one commit.
5. Guard hardening: replace grep/comment-counted assertions with structured
   fixture/test metadata for high-risk code families.
6. wasm budget lane: add a bundle-size gate and document the growth policy.

## Bottom Line

Phase 3 delivered major testing infrastructure and many high-quality slices.
It should not be dismissed or redone wholesale. The remaining work is
certification-grade cleanup: align the ledger with reality, decide what the
numeric exit criteria now mean, make debt ledgers historically enforceable, and
close the missing wasm bundle-size budget.
