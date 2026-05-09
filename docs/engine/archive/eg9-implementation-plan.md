# EG9 Implementation Plan

## Purpose

`EG9` closes the engine consolidation project.

`EG2` through `EG8` already moved security/open options, product open behavior,
graph, vector, search, executor storage access, and intelligence boundary
cleanup into the intended shape. `EG9` should not be another migration phase.
It is the final ratchet: prove the absorbed crates are gone, make the final
dependency graph enforceable, refresh active architecture documents, and record
the exact verification ledger before the next architecture effort begins.

The target stack remains:

```text
core -> storage -> engine -> intelligence -> executor -> cli
```

The default normal graph should be:

```text
strata-storage      -> strata-core
strata-engine       -> strata-core, strata-storage
strata-intelligence -> strata-core, strata-engine
strata-executor     -> strata-core, strata-engine, strata-intelligence
strata-cli          -> strata-executor
stratadb            -> strata-executor
```

With `embed` enabled, `strata-intelligence` may add the optional
`strata-inference` edge, and `strata-cli` may reach intelligence as an optional
command surface. No crate should import `strata-inference` directly except
`strata-intelligence`.

Read this with:

- [engine-consolidation-plan.md](./engine-consolidation-plan.md)
- [engine-crate-map.md](./engine-crate-map.md)
- [eg7-implementation-plan.md](./eg7-implementation-plan.md)
- [eg8-implementation-plan.md](./eg8-implementation-plan.md)
- [../storage/v1-storage-consumption-contract.md](../../storage/v1-storage-consumption-contract.md)

## Scope

`EG9` owns:

- proving retired crates are absent from the filesystem, workspace metadata,
  manifests, lockfile, and production source
- tightening the final graph guards so production crates above engine cannot
  regain storage or retired runtime-crate dependencies
- documenting the optional `embed`/inference edges separately from the default
  normal graph
- refreshing active engine/storage architecture documents so they describe the
  consolidated state rather than the migration state
- recording the final verification ledger and any known, explicitly accepted
  residual risks

`EG9` does not own:

- engine-next modular redesign
- storage-next design or implementation
- changing WAL, manifest, checkpoint, snapshot, search segment, or vector
  sidecar formats
- removing `strata-intelligence`
- removing `strata-inference`
- changing product behavior, command semantics, or model/provider behavior
- removing archive documents that intentionally preserve migration history

## Starting State

The absorbed crates are already deleted:

```text
crates/security          missing
crates/executor-legacy   missing
crates/graph             missing
crates/vector            missing
crates/search            missing
crates/core-legacy       missing
crates/concurrency       missing
crates/durability        missing
```

The current workspace members are:

```text
crates/core
crates/storage
crates/engine
crates/inference
crates/intelligence
crates/cli
crates/executor
```

The verified default normal graph is:

```text
strata-engine        -> strata-core, strata-storage
strata-intelligence  -> strata-core, strata-engine
strata-executor      -> strata-core, strata-engine, strata-intelligence
strata-cli           -> strata-executor
stratadb             -> strata-executor
```

With `--features embed`, `strata-intelligence` adds `strata-inference`.

Existing guards already enforce much of the target:

- retired security, executor-legacy, graph, vector, and search crate references
  are guarded in `tests/storage_surface_imports.rs`
- production direct storage bypasses above engine are guarded with an empty
  allowlist
- intelligence storage/retired-crate/subsystem-assembly regressions are guarded
- engine imports of intelligence/inference are guarded
- executor/CLI/root direct inference imports are guarded

Known allowed exceptions:

- `strata-engine` is the normal production consumer of `strata-storage`
- storage tests, engine tests, and root storage-facing tests may inspect storage
  directly where the test is intentionally about storage mechanics
- `strata-cli` may depend on `strata-intelligence` only through the optional
  `embed` command surface
- archive documents may mention deleted crates as historical migration context

## Load-Bearing Boundary Rules

### 1. Retired Crates Stay Deleted

These package names and directories must not return:

- `strata-security` / `crates/security`
- `strata-executor-legacy` / `crates/executor-legacy`
- `strata-graph` / `crates/graph`
- `strata-vector` / `crates/vector`
- `strata-search` / `crates/search`
- `strata-core-legacy` / `crates/core-legacy`
- `strata-concurrency` / `crates/concurrency`
- `strata-durability` / `crates/durability`

If a future compatibility shell is proposed, it must be a new architecture
decision, not an EG9 leftover.

### 2. Storage Has One Production Consumer

Normal production dependency on `strata-storage` is allowed only from
`strata-engine`.

Not allowed:

- `strata-executor -> strata-storage`
- `strata-intelligence -> strata-storage`
- `strata-cli -> strata-storage`
- `stratadb -> strata-storage`
- resurrected graph/vector/search/security/bootstrap crates depending on
  storage

### 3. Engine Does Not Point Upward

Engine must not depend on:

- `strata-intelligence`
- `strata-inference`
- executor or CLI crates
- model/provider HTTP clients through inference features

Engine owns semantic runtime behavior. Intelligence owns model/inference
adaptation above engine.

### 4. Subsystem Assembly Is Not An Upper-Layer Pattern

The low-level `OpenSpec`/`Subsystem` machinery may remain inside engine for
engine tests and engine-local runtime construction, but upper product crates
should use engine-owned product open helpers.

Upper production crates and intelligence tests must not assemble:

- `GraphSubsystem`
- `VectorSubsystem`
- `SearchSubsystem`
- `OpenSpec::with_subsystem`
- `OpenSpec::with_subsystems`

### 5. Optional Inference Stays Behind Intelligence

`strata-inference` remains optional and is reached through
`strata-intelligence`.

Allowed:

- `strata-intelligence --features embed -> strata-inference`
- executor feature forwarding to `strata-intelligence`
- CLI optional `embed` feature that enables executor/intelligence model
  commands

Not allowed:

- `strata-engine -> strata-inference`
- `strata-executor -> strata-inference`
- `strata-cli -> strata-inference`
- root `stratadb -> strata-inference`

## EG9A - Baseline Closeout Ledger

**Goal:**

Record the exact final state before changing guards or docs.

**Work:**

- run filesystem checks for deleted crate directories
- run `cargo metadata --format-version 1 --no-deps` and confirm retired
  packages are absent
- run default and feature-enabled `cargo tree` checks for engine-adjacent crates
- scan active manifests and `Cargo.lock` for retired package names
- scan production source/manifests for direct storage imports above engine
- scan active docs for "current" statements that contradict the final graph

**Acceptance:**

- deleted crate directories are absent
- workspace metadata contains no retired package
- `Cargo.lock` contains no retired package entry
- default graph matches the target graph
- `embed` graph adds inference only through intelligence
- any residual production source, manifest, or lockfile match is either
  archive/history text or a documented active exception
- active-doc statements that contradict the final graph are recorded as `EG9D`
  refresh items

**Implementation ledger, 2026-05-08:**

Deleted crate directory check passed. The following paths are absent:

```text
crates/security
crates/executor-legacy
crates/graph
crates/vector
crates/search
crates/core-legacy
crates/concurrency
crates/durability
```

Workspace metadata check passed. `cargo metadata --format-version 1 --no-deps`
contains no package named:

```text
strata-security
strata-executor-legacy
strata-graph
strata-vector
strata-search
strata-core-legacy
strata-concurrency
strata-durability
```

Active manifest and lockfile checks passed. `Cargo.lock`, the root manifest,
and active crate manifests contain no retired package entry or dependency edge.
The active production source scan has one benign historical comment:

```text
crates/engine/src/error.rs
```

That comment explicitly says engine does not depend on the retired
`strata-durability` package; it is not a live import or manifest edge.

The default normal graph matches the target:

```text
strata-storage      -> strata-core
strata-engine       -> strata-core, strata-storage
strata-intelligence -> strata-core, strata-engine
strata-executor     -> strata-core, strata-engine, strata-intelligence
strata-cli          -> strata-executor
stratadb            -> strata-executor
```

The inverse storage graph at normal depth 1 is:

```text
strata-storage -> strata-engine
```

The `embed` graph adds inference only through intelligence:

```text
strata-intelligence --features embed -> strata-inference
strata-executor --features embed     -> strata-intelligence -> strata-inference
strata-cli --features embed          -> strata-executor, strata-intelligence
```

The direct production storage-bypass scan over `src`, `crates/executor/src`,
`crates/intelligence/src`, `crates/cli/src`, and the upper-crate manifests found
no production storage imports above engine. The only storage matches in the
root manifest are expected:

- `crates/storage` as a workspace member
- root `strata-storage` as a dev-dependency for storage-facing integration tests

Active document scan findings for `EG9D`:

- [../storage/v1-storage-consumption-contract.md](../../storage/v1-storage-consumption-contract.md)
  still has current-tense transitional notes about executor importing storage
  directly and documented temporary migration shims.
- [../storage/storage-crate-map.md](../../storage/storage-crate-map.md) still says
  the incoming storage graph includes `strata-executor` as a current
  transitional storage dependent.
- [engine-consolidation-plan.md](./engine-consolidation-plan.md) still has
  active Direct Storage Rule wording that permits temporary migration shims
  named in the plan and deleted by closeout. `EG9` is closeout, so that
  permission must be removed or rewritten.
- [engine-consolidation-plan.md](./engine-consolidation-plan.md) still has
  lower-section migration/risk prose that says executor currently constructs
  storage keys or calls primitive stores directly.

Those are documentation-refresh items, not code or graph regressions. Historical
phase plans such as `eg1` through `eg8` and storage `ST*` plans intentionally
mention retired crates as migration history.

## EG9B - Final Graph And Retired-Crate Guards

**Goal:**

Make the final graph fail closed.

**Work:**

- tighten `tests/storage_surface_imports.rs` so the final guards cover:
  - retired crate directories and package names
  - retired package names in active manifests and `Cargo.lock`
  - no production direct storage imports above engine
  - no engine imports of intelligence or inference
  - no direct inference imports outside intelligence
  - no upper-layer subsystem assembly
- remove any stale transitional allowlist entry; the production storage bypass
  allowlist should remain empty
- keep root dev-dependency and storage-facing test exceptions explicit rather
  than broadening production scan exclusions
- add self-tests for any new scanner logic

**Acceptance:**

- guard tests fail if any retired crate directory or package name returns
- guard tests fail if executor, intelligence, CLI, or root product code imports
  storage
- guard tests fail if engine points to intelligence/inference
- guard tests fail if inference is imported directly outside intelligence
- guard tests fail if upper layers instantiate product runtime subsystems
- `cargo test -p stratadb --test storage_surface_imports` passes

**Implementation, 2026-05-08:**

- added a unified retired-crate closeout guard for deleted directories, active
  source, active manifests, and `Cargo.lock`
- extended retired package checks to cover `strata-security`,
  `strata-core-legacy`, `strata-concurrency`, and `strata-durability` in the
  same final scanner as graph/vector/search/executor-legacy
- kept the production storage-bypass allowlist empty and retained the explicit
  root dev-dependency exception for storage-facing tests
- added a generic upper-layer runtime assembly guard for `OpenSpec`,
  `with_subsystem`, `with_subsystems`, `dyn Subsystem`, and engine-owned
  graph/vector/search subsystem names
- added scanner self-tests for manifest/lockfile/source retired-crate markers,
  comment/literal/substring handling, whitespace-separated method calls,
  qualified or aliased `Subsystem` trait references, and feature-enabled
  `cfg(any(test, feature = "..."))` modules
- verified `cargo test -p stratadb --test storage_surface_imports`: 40 passed

## EG9C - Optional Edge Policy And Feature Surface

**Goal:**

Document and enforce the remaining optional edges so future readers do not
mistake them for storage/engine consolidation leaks.

**Work:**

- confirm default `strata-cli` has no normal `strata-intelligence` edge
- confirm `strata-cli --features embed` only reaches intelligence through the
  documented optional command surface
- confirm executor still has the intended intelligence edge for product
  command handling and inference-facing facades
- confirm no executor/CLI/root direct `strata-inference` edge exists
- document the accepted CLI optional `embed` exception in the crate map and main
  plan if not already clear

**Acceptance:**

- default `cargo tree -p strata-cli --edges normal --depth 1` shows only
  `strata-executor` as a Strata dependency
- `cargo tree -p strata-cli --features embed --edges normal --depth 3` and
  `cargo tree -p strata-cli --features embed --edges normal -i strata-inference`
  show inference only through intelligence
- direct inference import guard remains green
- active docs describe default and feature-enabled graphs separately

**Implementation, 2026-05-08:**

- added `engine_consolidation_optional_embed_edges_are_policy_bound` to
  `tests/storage_surface_imports.rs`
- root `embed`/provider features are guarded so they forward only through
  `strata-executor`, and the root package may not add a normal
  `strata-intelligence` dependency
- CLI's default feature set is guarded as empty; its direct
  `strata-intelligence` dependency must stay `optional = true`, must enable
  intelligence's `embed` feature, and must be reached only through CLI's
  `embed` feature
- executor's normal `strata-intelligence` dependency is guarded as
  non-optional, while executor `embed`/provider features may forward only to
  `strata-intelligence` and the engine marker `embed` feature
- intelligence remains the only crate whose feature table may reference
  `strata-inference`; its inference dependency must stay optional with
  `default-features = false`
- verified default CLI graph:
  `cargo tree -p strata-cli --edges normal --depth 1`
- verified feature-enabled inference path:
  `cargo tree -p strata-cli --features embed --edges normal -i strata-inference`
  reports inference only under `strata-intelligence`
- verified `cargo test -p stratadb --test storage_surface_imports`: 44 passed

## EG9D - Active Architecture Document Refresh

**Goal:**

Leave active docs in the consolidated state and archive migration details.

**Work:**

- update [engine-consolidation-plan.md](./engine-consolidation-plan.md) so
  `EG9` is the closeout phase, not a future migration bucket
- update [engine-crate-map.md](./engine-crate-map.md) with the final verified
  graph and a short closeout ledger
- review [../storage/v1-storage-consumption-contract.md](../../storage/v1-storage-consumption-contract.md)
  for stale "temporary migration shim" wording and clarify that normal
  production consumption is engine-only after EG9
- remove closeout-incompatible temporary migration shim permission from active
  engine/storage docs
- leave `docs/engine/archive/` as history, but avoid active docs saying deleted
  crates are still current
- add a short handoff note for the next architecture effort: engine-next and
  storage-next are new design phases, not continuation of EG cleanup

**Acceptance:**

- active docs agree on the default graph and optional embed graph
- active docs agree that retired peer crates are deleted
- active docs agree that storage consumption above engine is forbidden
- archive docs remain historical and do not drive current guard policy

**Implementation, 2026-05-08:**

- refreshed [engine-crate-map.md](./engine-crate-map.md) so it describes the
  post-`EG9` graph, optional-edge policy, retired compatibility-shell state, and
  v1 architecture handoff
- refreshed [../storage/storage-crate-map.md](../../storage/storage-crate-map.md)
  so `strata-engine` is the only normal production storage consumer and the
  root storage dependency is explicitly test/dev-only
- refreshed [../storage/v1-storage-consumption-contract.md](../../storage/v1-storage-consumption-contract.md)
  so engine-consolidation migration shims are no longer allowed consumers,
  executor is no longer described as a direct storage importer, and the closeout
  notes match the completed graph/vector/search/executor cleanup
- refreshed [../storage/storage-engine-ownership-audit.md](../../storage/storage-engine-ownership-audit.md)
  so accepted residue no longer claims upper crates have transitional direct
  storage dependencies
- updated [engine-consolidation-plan.md](./engine-consolidation-plan.md) to
  mark `EG9D` complete, replace stale follow-up plan names with the actual
  phase-plan files, and state that future storage-next/engine-next work is a v1
  design phase rather than continuation of `EG`

## EG9E - Final Verification And Closeout

**Goal:**

Run the final matrix and record what passed.

**Required commands:**

```bash
cargo fmt --check
cargo check --workspace --all-targets
cargo test -p stratadb --test storage_surface_imports
cargo test -p strata-engine
cargo test -p strata-executor
cargo test -p strata-intelligence
STRATA_INFERENCE_SKIP_LLAMA_CPP_BUILD_FOR_CHECK=1 cargo check -p strata-intelligence --features embed --tests
STRATA_INFERENCE_SKIP_LLAMA_CPP_BUILD_FOR_CHECK=1 cargo check -p strata-executor --features embed --tests
STRATA_INFERENCE_SKIP_LLAMA_CPP_BUILD_FOR_CHECK=1 cargo check -p strata-cli --features embed --tests
```

**Graph checks:**

```bash
cargo tree -p strata-engine --edges normal --depth 1
cargo tree -p strata-intelligence --edges normal --depth 1
cargo tree -p strata-intelligence --features embed --edges normal --depth 1
cargo tree -p strata-executor --edges normal --depth 1
cargo tree -p strata-cli --edges normal --depth 1
cargo tree -p strata-cli --features embed --edges normal --depth 3
cargo tree -p strata-cli --features embed --edges normal -i strata-inference
cargo tree -p stratadb --edges normal --depth 1
cargo tree -i strata-storage --workspace --edges normal --depth 1
```

**Retired-crate checks:**

```bash
for d in \
  crates/security \
  crates/executor-legacy \
  crates/graph \
  crates/vector \
  crates/search \
  crates/core-legacy \
  crates/concurrency \
  crates/durability
do
  if test -e "$d"; then
    echo "retired crate directory still exists: $d" >&2
    exit 1
  fi
done

metadata="$(cargo metadata --format-version 1 --no-deps)" || exit $?
rg_status=0
printf '%s\n' "$metadata" \
  | rg '"name":"strata-(security|executor-legacy|graph|vector|search|core-legacy|concurrency|durability)"|crates/(security|executor-legacy|graph|vector|search|core-legacy|concurrency|durability)' \
  || rg_status=$?
case "$rg_status" in
  0) echo "retired crate metadata entry found" >&2; exit 1 ;;
  1) ;;
  *) exit "$rg_status" ;;
esac

tracked_manifests="$(git ls-files \
    Cargo.lock \
    Cargo.toml \
    'crates/**/Cargo.toml' \
    'benchmarks/**/Cargo.toml' \
    'benchmarks/**/Cargo.lock')" || exit $?
if test -n "$tracked_manifests"; then
  rg_status=0
  printf '%s\n' "$tracked_manifests" \
    | xargs rg -n 'name = "strata-(security|executor-legacy|graph|vector|search|core-legacy|concurrency|durability)"|strata-(security|executor-legacy|graph|vector|search|core-legacy|concurrency|durability)' \
    || rg_status=$?
  case "$rg_status" in
    0) echo "retired crate manifest or lockfile reference found" >&2; exit 1 ;;
    1) ;;
    *) exit "$rg_status" ;;
  esac
fi

rg_status=0
rg -n 'strata_(security|executor_legacy|graph|vector|search)' \
  src crates/{executor,intelligence,cli} \
  -g '*.rs' \
  || rg_status=$?
case "$rg_status" in
  0) echo "retired crate Rust import found" >&2; exit 1 ;;
  1) ;;
  *) exit "$rg_status" ;;
esac
```

**Boundary checks:**

```bash
rg_status=0
rg -n "strata_storage::|use strata_storage|strata-storage|strata_storage" \
  src crates/{executor,intelligence,cli} \
  -g 'Cargo.toml' -g '*.rs' \
  || rg_status=$?
case "$rg_status" in
  0) echo "direct storage import above engine found" >&2; exit 1 ;;
  1) ;;
  *) exit "$rg_status" ;;
esac

rg_status=0
rg -n "strata_intelligence|strata-inference|strata_inference" \
  crates/engine \
  -g 'Cargo.toml' -g '*.rs' \
  || rg_status=$?
case "$rg_status" in
  0) echo "engine upward dependency marker found" >&2; exit 1 ;;
  1) ;;
  *) exit "$rg_status" ;;
esac

rg_status=0
rg -n "strata_inference|strata-inference" \
  Cargo.toml src crates/executor crates/cli \
  -g 'Cargo.toml' -g '*.rs' \
  || rg_status=$?
case "$rg_status" in
  0) echo "direct inference dependency outside intelligence found" >&2; exit 1 ;;
  1) ;;
  *) exit "$rg_status" ;;
esac

rg_status=0
rg -n "OpenSpec|GraphSubsystem|VectorSubsystem|SearchSubsystem|with_subsystem|with_subsystems" \
  crates/intelligence \
  -g '*.rs' \
  || rg_status=$?
case "$rg_status" in
  0) echo "intelligence runtime subsystem assembly found" >&2; exit 1 ;;
  1) ;;
  *) exit "$rg_status" ;;
esac
```

The no-match commands above accept only `rg` exit code `1` as success. Guard-test
source, archive documents, and ignored local generated lockfiles may mention
retired package names as test fixtures, history, or stale cache state; they are
intentionally outside these closeout source/import checks.

**Feature-enabled test note:**

Real `cargo test -p strata-intelligence --features embed` may require a
populated local inference vendor/native build and model assets. If it cannot be
run in the current workspace, record the exact missing prerequisite and rely on
the feature-enabled `cargo check` gates above for this closeout.

**Acceptance:**

- all required compile/test gates pass, or any pre-existing unrelated failure
  is named with the exact failing test and reason
- the final graph is recorded in active docs
- no temporary compatibility shell survives without being explicitly named
- EG cleanup is closed, and the next work item is a new architecture design
  phase rather than more consolidation cleanup

**Implementation, 2026-05-08:**

Compile and test gates:

- `cargo fmt --check`: passed
- `cargo check --workspace --all-targets`: failed before Rust checking because
  the `strata-inference` build script attempted to configure the local
  `llama.cpp` native source and
  `crates/inference/vendor/llama.cpp/CMakeLists.txt` is absent in this
  checkout
- `STRATA_INFERENCE_SKIP_LLAMA_CPP_BUILD_FOR_CHECK=1 cargo check --workspace --all-targets`:
  passed
- `cargo test -p stratadb --test storage_surface_imports`: passed, 44 tests
- `cargo test -p strata-engine`: passed after STAB1 stabilization; the main
  engine lib gate reported 2570 passed and 8 ignored; all integration tests
  passed; doc-tests reported 69 passed and 1 ignored
- `cargo test -p strata-executor`: passed, 116 tests
- `cargo test -p strata-intelligence`: passed; default-feature intelligence
  test targets currently contain no enabled tests
- `cargo test -p strata-intelligence --features embed`: not run because the
  feature-enabled test path requires the same missing local
  `crates/inference/vendor/llama.cpp/CMakeLists.txt` native-source prerequisite
  as the raw workspace check, plus any test-specific model assets
- `STRATA_INFERENCE_SKIP_LLAMA_CPP_BUILD_FOR_CHECK=1 cargo check -p strata-intelligence --features embed --tests`:
  passed
- `STRATA_INFERENCE_SKIP_LLAMA_CPP_BUILD_FOR_CHECK=1 cargo check -p strata-executor --features embed --tests`:
  passed
- `STRATA_INFERENCE_SKIP_LLAMA_CPP_BUILD_FOR_CHECK=1 cargo check -p strata-cli --features embed --tests`:
  passed

The initial EG9 closeout run exposed 10 shutdown/fault-injection and branch
cleanup/classification failures outside the graph/storage-boundary guards.
STAB1 fixed those failures by normalizing private engine fault-injection path
keys and tightening deterministic WAL/branch cleanup test control; the focused
shutdown suite, the two focused branch cleanup tests, and the full
`cargo test -p strata-engine` gate now pass. See
[engine-stabilization-implementation-history.md](./engine-stabilization-implementation-history.md) for the failure
ledger and fix audit trail.

Graph checks passed:

- default `strata-engine` normal graph depends on `strata-core` and
  `strata-storage`, with no upper-layer Strata crates
- default `strata-intelligence` normal graph depends on `strata-core` and
  `strata-engine`, with no `strata-storage`
- `strata-intelligence --features embed` adds `strata-inference`
- default `strata-executor` depends on `strata-core`, `strata-engine`, and
  `strata-intelligence`, with no storage or retired peer crate edge
- default `strata-cli` depends on `strata-executor` only among Strata crates
- `strata-cli --features embed` reaches inference through intelligence
- default `stratadb` depends on `strata-executor` only among Strata crates
- inverse storage graph at depth 1 is `strata-storage -> strata-engine`

Retired-crate and boundary checks passed:

- deleted crate directories remain absent
- tracked active manifests and lockfiles contain no retired crate package names
  or paths
- active upper-layer Rust source contains no retired crate imports
- executor, intelligence, CLI, and root production code have no direct storage
  imports
- engine has no intelligence/inference import or manifest edge
- executor, CLI, and root product code have no direct inference import or
  manifest edge
- intelligence does not assemble engine product runtime subsystems

An ignored local `benchmarks/Cargo.lock` in this checkout initially contained
stale `strata-search` entries. `cargo generate-lockfile` was run in
`benchmarks/` to refresh that ignored file locally. That is local non-committed
hygiene only: the committed closeout check uses `git ls-files` so ignored local
lockfile cache state cannot create a false active-manifest failure. When working
inside `benchmarks/`, regenerate that ignored lockfile before running benchmark
crate commands if stale dependency output appears.

No temporary engine-consolidation compatibility shell survives closeout. The
remaining work is a new v1 architecture design phase, not another `EG` cleanup
phase.
