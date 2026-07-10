# V1 Engineering Standards Baseline

Status: M0TD standards baseline

## Purpose

This document records the M0TD scan of the current repository against
`docs/architecture/v1-engineering-standards.md`.

It is evidence, not permission to keep the current shape. The current
pre-V1 source tree is being replaced or cut over during M1-M10. This baseline
exists so new V1 implementation work does not accidentally preserve old
project-history names, oversized files, unclear ownership names, or weak error
handling patterns.

## Capture

| Field | Value |
|---|---|
| Branch | `v1` |
| Base commit | `35650d9b` |
| Capture date | 2026-05-11 |
| Review correction date | 2026-05-12 |
| Worktree state | Dirty M0 documentation workspace |
| Scope | `crates/`, `tests/`, workspace manifests, and selected active target docs; no `benches/` directory exists at capture time |
| Standards source | `docs/architecture/v1-engineering-standards.md` |

The dirty worktree is intentional. This baseline was captured after earlier M0
documentation slices had modified and created architecture documents. The base
commit identifies the starting point; the scan results describe the working
tree at M0TD capture time, not a clean checkout of that commit.

## Classification Rules

| Classification | Meaning | Required action |
|---|---|---|
| V1 blocker | Violation in a V1 target document, new V1 implementation, public API, durable format, error code, CLI surface, or test that is meant to survive V1. | Fix before the owning slice closes. |
| Old-code debt | Violation in current pre-V1 code or tests that will be replaced, renamed, ported, or deleted by the milestone plans. | Do not copy into V1. M0TE must classify surviving tests. |
| Historical evidence | Violation inside an archive, old cleanup document, or current-code evidence document that is explicitly not binding target architecture. | Leave only when clearly marked historical. |
| Allowed planning metadata | Roadmap labels inside architecture plans, inventories, registers, PR text, or issue labels. | Allowed; must not enter production vocabulary. |

## Commands Run

```bash
rg -n '\b(M[0-9][A-Z0-9]*|M[0-9]T[A-Z0-9]*|m[0-9][a-z0-9]*|EG[0-9][A-Z]*|STAB[0-9][A-Z]*|ESTAB[0-9A-Z]*|EX[0-9][A-Z]*|ES[0-9][A-Z]*)\b' \
  docs/architecture/storage/target-crate-shape-and-test-harness.md \
  docs/architecture/engine/target-crate-shape-and-test-harness.md \
  docs/architecture/inference-architecture.md \
  docs/architecture/intelligence-architecture.md

rg -n '\bfacade\b|\bFacade\b' \
  docs/architecture/storage/target-crate-shape-and-test-harness.md \
  docs/architecture/engine/target-crate-shape-and-test-harness.md \
  docs/architecture/inference-architecture.md \
  docs/architecture/intelligence-architecture.md

rg --files-with-matches '\b(EG[0-9][A-Z]*|STAB[0-9][A-Z]*|ESTAB[0-9A-Z]*|EX[0-9][A-Z]*|ES[0-9][A-Z]*|M[0-9][A-Z0-9]*|M[0-9]T[A-Z0-9]*|m[0-9][a-z0-9]*)\b' \
  crates tests --glob '!target/**'

rg -n '\b(EG[0-9][A-Z]*|STAB[0-9][A-Z]*|ESTAB[0-9A-Z]*|EX[0-9][A-Z]*|ES[0-9][A-Z]*|M[0-9][A-Z0-9]*|M[0-9]T[A-Z0-9]*|m[0-9][a-z0-9]*)\b|\b(Manager|Coordinator|Runtime|Context|Helper|Util|Facade|Bridge|Adapter)\b' \
  Cargo.toml crates/**/Cargo.toml --glob '!target/**'

rg --count-matches '\b(Manager|Coordinator|Runtime|Context|Helper|Util|Facade|Bridge|Adapter)\b' \
  crates tests --glob '*.rs' --glob '!target/**'

rg -n 'TODO|FIXME' \
  crates tests docs --glob '*.rs' --glob '*.md' \
  --glob '!docs/**/archive/**' \
  --glob '!docs/architecture/v1-engineering-standards-baseline.md' \
  --glob '!target/**'

rg -n '\bunwrap\(|\bexpect\(|\bpanic!\(' \
  crates tests --glob '*.rs' --glob '!target/**'

rg -n 'let _ =|\.ok\(\)|\.unwrap_or_default\(\)' \
  crates tests --glob '*.rs' --glob '!target/**'
```

Aggregate count commands for the table:

```bash
printf 'cleanup label files: '
rg --files-with-matches '\b(EG[0-9][A-Z]*|STAB[0-9][A-Z]*|ESTAB[0-9A-Z]*|EX[0-9][A-Z]*|ES[0-9][A-Z]*|M[0-9][A-Z0-9]*|M[0-9]T[A-Z0-9]*|m[0-9][a-z0-9]*)\b' crates tests --glob '!target/**' | wc -l

printf 'cleanup label matches: '
rg -o '\b(EG[0-9][A-Z]*|STAB[0-9][A-Z]*|ESTAB[0-9A-Z]*|EX[0-9][A-Z]*|ES[0-9][A-Z]*|M[0-9][A-Z0-9]*|M[0-9]T[A-Z0-9]*|m[0-9][a-z0-9]*)\b' crates tests --glob '!target/**' | wc -l

printf 'manifest standards matches: '
rg -o '\b(EG[0-9][A-Z]*|STAB[0-9][A-Z]*|ESTAB[0-9A-Z]*|EX[0-9][A-Z]*|ES[0-9][A-Z]*|M[0-9][A-Z0-9]*|M[0-9]T[A-Z0-9]*|m[0-9][a-z0-9]*)\b|\b(Manager|Coordinator|Runtime|Context|Helper|Util|Facade|Bridge|Adapter)\b' Cargo.toml crates/**/Cargo.toml --glob '!target/**' | wc -l

printf 'avoid-name files: '
rg --files-with-matches '\b(Manager|Coordinator|Runtime|Context|Helper|Util|Facade|Bridge|Adapter)\b' crates tests --glob '*.rs' --glob '!target/**' | wc -l

printf 'avoid-name matches: '
rg -o '\b(Manager|Coordinator|Runtime|Context|Helper|Util|Facade|Bridge|Adapter)\b' crates tests --glob '*.rs' --glob '!target/**' | wc -l

printf 'todo/fixme matches: '
rg -o 'TODO|FIXME' crates tests docs --glob '*.rs' --glob '*.md' --glob '!docs/**/archive/**' --glob '!docs/architecture/v1-engineering-standards-baseline.md' --glob '!target/**' | wc -l

printf 'unwrap production matches: '
rg -o '\bunwrap\(|\bexpect\(|\bpanic!\(' crates --glob '*.rs' --glob '!target/**' | wc -l

printf 'unwrap test-tree matches: '
rg -o '\bunwrap\(|\bexpect\(|\bpanic!\(' tests --glob '*.rs' --glob '!target/**' | wc -l

printf 'unwrap total matches: '
rg -o '\bunwrap\(|\bexpect\(|\bpanic!\(' crates tests --glob '*.rs' --glob '!target/**' | wc -l

printf 'ignored production matches: '
rg -o 'let _ =|\.ok\(\)|\.unwrap_or_default\(\)' crates --glob '*.rs' --glob '!target/**' | wc -l

printf 'ignored test-tree matches: '
rg -o 'let _ =|\.ok\(\)|\.unwrap_or_default\(\)' tests --glob '*.rs' --glob '!target/**' | wc -l

printf 'ignored total matches: '
rg -o 'let _ =|\.ok\(\)|\.unwrap_or_default\(\)' crates tests --glob '*.rs' --glob '!target/**' | wc -l

printf 'rust files: '
find crates tests -type f -name '*.rs' | wc -l

printf 'rust files plus manifests: '
find crates tests -type f \( -name '*.rs' -o -name 'Cargo.toml' \) | wc -l
```

File-size scan:

```bash
python3 - <<'PY'
from pathlib import Path
roots = [Path("crates"), Path("tests")]
files = []
for root in roots:
    if not root.exists():
        continue
    for path in root.rglob("*.rs"):
        if "target" in path.parts:
            continue
        files.append((len(path.read_text(errors="ignore").splitlines()), str(path)))
for lines, path in sorted(files, reverse=True)[:30]:
    print(f"{lines}\t{path}")
PY
```

## Baseline Results

| Scan | Result | Classification |
|---|---:|---|
| Target crate-shape docs containing roadmap or cleanup labels | 0 matches | Clean |
| Target crate-shape docs containing `facade` | 0 matches | Clean |
| Workspace and crate manifests containing roadmap, cleanup, or avoid-list names | 0 matches | Clean |
| Current source/test tree files containing roadmap or cleanup labels | 17 files, 174 matches | Old-code debt |
| Current source/test files containing avoid-list ownership names | 83 files, 169 matches | Old-code debt with case-by-case V1 review |
| TODO/FIXME matches in non-archive source/docs | 5 matches | 2 old-code debt, 3 standards examples |
| `unwrap`, `expect`, or `panic!` matches in `crates/` | 11,904 matches | Old-code debt in production and in-crate tests |
| `unwrap`, `expect`, or `panic!` matches in root `tests/` | 5,727 matches | Old-code debt; M0TE classifies surviving tests |
| `unwrap`, `expect`, or `panic!` matches total | 17,631 matches | Old-code debt |
| Ignored-error patterns in `crates/` | 470 matches | Old-code debt; V1 requires justification comments |
| Ignored-error patterns in root `tests/` | 87 matches | Old-code debt; M0TE classifies surviving tests |
| Ignored-error patterns total | 557 matches | Old-code debt |
| Rust files scanned | 475 files | Baseline scope |
| Rust files plus manifests scanned | 482 files | Baseline scope |

## Findings

### V1 Blockers

No V1 blockers were found in the target crate-shape and standards-alignment
documents checked by M0TD.

The absence of blockers here is narrow. It means the current target-shape docs
do not contain obvious forbidden roadmap labels or the previously rejected
`facade` vocabulary. It does not mean the old source tree satisfies the V1
standards.

The full active documentation tree intentionally contains roadmap labels in
roadmaps, implementation plans, inventories, and registers. Those are allowed
planning metadata under the engineering standards. M0TD checks the target
crate-shape docs for leakage because those documents are closest to future code
vocabulary.

### Old-Code Debt

The current source and test tree, including fixture files under `tests/`,
still contains milestone-shaped names and historical cleanup vocabulary.
Examples include:

1. `tests/intelligence/m6_search_request.rs`
2. `tests/intelligence/m6_search_response.rs`
3. `tests/intelligence/m6_hybrid_search.rs`
4. `tests/intelligence/m6_budget_propagation.rs`
5. `tests/intelligence/m6_rrf_fusion.rs`
6. Current source files whose literals or fixtures contain old short labels
   such as `ES` or `M6`.

These are not fixed in M0TD. M0TE owns the test inventory that decides whether
each current test is kept, rewritten, archived, or deleted. Any test kept for
V1 must be renamed around behavior, not milestone history.

The avoid-list ownership scan is intentionally noisy. Some current names are
domain-valid in context, while others reflect unclear boundaries. V1 slices
must apply the concept-budget test before introducing any public or crate-wide
type whose name ends in `Manager`, `Coordinator`, `Runtime`, `Context`,
`Helper`, `Util`, `Facade`, `Bridge`, or `Adapter`.

The error-handling scans are also old-code debt. V1 implementation code must
not normalize the current density of `unwrap`, `expect`, `panic!`, `.ok()`,
`let _ =`, or `.unwrap_or_default()` in production paths. The split counts show
that production source and in-crate tests under `crates/` carry most of the
matches, while root integration tests carry a smaller but still large share.
Tests may use unwrapping when the failure is not the behavior under test, but
new tests should prefer clear failure messages where the setup is non-trivial.

### File-Size Baseline

The largest current files exceed the V1 review thresholds by a wide margin.
The top examples at capture time were:

| Lines | File |
|---:|---|
| 7,693 | `tests/integration/branching.rs` |
| 6,212 | `crates/engine/src/branch_ops/mod.rs` |
| 5,838 | `crates/storage/src/segmented/mod.rs` |
| 5,208 | `crates/engine/src/search/index.rs` |
| 4,737 | `crates/storage/src/segment.rs` |
| 4,180 | `crates/engine/src/vector/store/mod.rs` |
| 3,922 | `tests/executor/session_transactions.rs` |
| 3,856 | `crates/engine/src/error.rs` |
| 3,823 | `crates/engine/src/primitives/json/mod.rs` |
| 3,423 | `crates/engine/src/branch_ops/branch_control_store.rs` |

This is expected for the old architecture and is not a request to split those
files immediately. It is a baseline that V1 implementation slices must not
repeat. When behavior is ported, the new files should follow the thresholds in
`docs/architecture/v1-engineering-standards.md` or record a local exception.

## M0TD Closure

M0TD is closed when:

1. This baseline document is present in the V1 document inventory.
2. The M0 implementation plan names this document as the M0TD deliverable.
3. Target crate-shape docs have no standards blockers from the M0TD scans.
4. Current source/test violations are classified as old-code debt, not as
   acceptable V1 patterns.
5. M0TE is explicitly responsible for deciding the fate of milestone-named and
   behavior-freezing tests.

## Use During Implementation

For each future V1 slice:

1. Run the relevant standards scans against changed files.
2. Treat any new production occurrence of roadmap labels as a blocker.
3. Treat any kept test with milestone-shaped naming as a blocker until renamed.
4. Justify avoid-list ownership names at the point where they are introduced.
5. Record file-size exceptions in the slice review or local module comment.
