# Strata-Core: Claude Code Instructions

## Status

Strata is in the **V1 architecture rewrite**. Active branch: `v1` (integration line). `main` is frozen for old-architecture work — do not extend it on the V1 line.

The V1 line is a clean break. No compatibility shims between old and new code, no migration tooling for pre-V1 databases, no parallel old/new paths held alive indefinitely.

## V1 Stack

```text
core
  → storage
  → engine
  → intelligence → executor / CLI / SDK / Strata AI
  → inference
```

- **core** — smallest shared atoms (`BranchId`, `CommitVersion`, timestamp, type-local validation errors). No `Value`, no `EntityRef`, no storage transaction IDs.
- **storage** — generic persistence mechanics, L1-L9 layered. Knows nothing about KV/JSON/event/vector/graph semantics.
- **engine** — product semantics, data capabilities, branches, time travel, retrieval, IPC classification, clone artifacts, derived-state manifests. Owns adapter traits used by intelligence.
- **intelligence** — autoembedding, query expansion, reranking, RAG, generation orchestration. Consumes engine surfaces; never imports storage; never speaks provider HTTP.
- **inference** — provider execution and model artifact resolution. `Generator` / `Embedder` / `Reranker` traits. Knows nothing about Strata databases.
- **executor / CLI / SDK / Strata AI** — consume engine and intelligence. Never import storage directly. Never import inference directly.

## Where To Read Before Working On A Slice

1. **Roadmap** — `docs/architecture/strata-v1-implementation-roadmap.md`
2. **Current milestone plan** — `docs/architecture/implementation-plans/m{N}-m{N}t-implementation-plan.md`
3. **Layer architecture** — `docs/architecture/{layer}-architecture.md`
4. **Contracts** — `docs/architecture/engine/<contract>.md` or `docs/architecture/storage/<layer>.md` (docs keep their design-phase names)
5. **Test inventory** — `docs/architecture/v1-existing-test-inventory-and-porting-plan.md`
6. **Engineering standards** — `docs/architecture/v1-engineering-standards.md`
7. **Error contract** — `docs/architecture/v1-error-and-diagnostics-contract.md`
8. **Storage format spec** — `docs/spec/strata-storage-format-v1.md`

Architecture docs are authoritative. This file restates only the hard invariants needed during slice work — when in doubt, the contract wins.

## Hard Rules

### Dependency direction (CI-enforced)

```text
core  ← storage  ← engine  ← intelligence  ← executor / CLI / SDK
                            ← inference   ←
```

1. Only engine imports storage, and only inside `persistence/`.
2. Engine never imports intelligence or inference.
3. Inference imports nothing from the Strata workspace.
4. Intelligence-next imports engine and inference only.
5. Executor and CLI consume intelligence; never import inference directly.
6. The dependency DAG is enforced by a workspace guard test on every PR.

### Authority

7. Engine owns semantics. Executor is a thin transport/session adapter.
8. One canonical path per operation. No two public surfaces expose the same behavior.
9. No process-global semantic state. Per-database state only.
10. Engine owns adapter traits (`QueryExpander`, `ResultReranker`, `RagGenerator`, embedding contracts). Intelligence installs implementations per database.

### Storage substrate

11. Branch-aware MVCC KV row is the only physical storage primitive. KV / JSON / event / vector / graph are engine capabilities layered over it.
12. WAL writer halts on fsync failure. Recovery via explicit resume.
13. Codec is uniform across WAL, snapshots, manifest, and table blocks. Durable format is frozen at M3 and gated by golden vectors.
14. Cache mode is non-durable by design — no WAL, manifest, snapshot, checkpoint, durable table, quarantine, or lock objects.

### Branch and capability

15. One canonical `BranchId` lives in core; derivation lives in engine.
16. Branch generations are monotonic, scoped per branch name.
17. Every capability declares lifecycle, branch adapter, search adapter, relationship adapter, and derived-state hooks.
18. Cross-branch references are rejected.
19. Empty branch creation is required.
20. Branch merge is strict refusal on divergent concurrent history (V1).
21. JSON merge is document-level (V1).

### Retrieval and derived state

22. Engine owns deterministic retrieval, recipes, derived-state manifests, and source validation.
23. Intelligence owns model-dependent stages. Engine never calls model providers.
24. Embedding-model mismatch is detected by engine retrieval and surfaces `failed_precondition.embedding_model_mismatch`.
25. Shadow vectors are engine-owned derived rows. Intelligence decides what to embed; engine owns the row.
26. Source rows are authoritative — derived state may accelerate retrieval, never replace it.

### Errors and diagnostics

27. Error codes use `<class>.<area>.<detail>` format. See `v1-error-and-diagnostics-contract.md` for the registry.
28. Public error enums are `#[non_exhaustive]`.
29. Tests assert on error class and code, never on display text.
30. Storage errors do not contain product wording.
31. Provider keys, signed URLs, prompts, and document contents are redacted by default.

### Public surface

32. Engine D4 public surface is documented in `engine-architecture.md`. New public types require reviewer approval.
33. `pub(crate)` by default; `pub` only for D4 items.
34. `unreachable_pub` denies after visibility tightening.
35. Newtypes use `#[repr(transparent)]` + `#[serde(transparent)]` for wire stability.

### Quality

36. `[workspace.lints]` is the single source of truth for lint config.
37. `#![deny(unsafe_code)]` on safe crates: core, storage (above backend FFI), engine, intelligence.
38. Inference denies unsafe outside `local/`; audited unsafe is allowed only inside `local/`.
39. Typed reason enums replace string-factory error methods.

### Cutover

40. No permanent compatibility layer between old storage and new engine, or old engine and new storage.
41. No migration tooling for pre-V1 development databases.
42. Pre-V1 databases are rejected after cutover with structured format/layout errors.
43. Crates shed the `-next` suffix in M9B before V1 promotion to `main`.

## Milestone Nomenclature

Slice codes follow the roadmap:

```text
M{milestone}{epic-letter}{slice-number}        e.g., M3B2
M{milestone}T{test-epic-letter}{slice-number}  e.g., M3TB2
```

Every PR title includes its slice code. Every milestone has both an implementation track (M*) and a test track (M*T); the milestone closes only when both pass their exit gates.

Milestones:

| Code | Title |
|---|---|
| M0 | Architecture freeze and tracking |
| M1 | Core |
| M2 | Storage testkit and crate skeleton |
| M3 | Storage backend, layout, format, durable services |
| M4 | Storage table, branch, commit, recovery, L9 API |
| M5 | Engine persistence adapter and control plane |
| M6 | Engine product semantics |
| M7 | Inference hardening |
| M8 | Intelligence-next orchestration |
| M9 | Executor, CLI, SDK, tests, benches, docs cutover |
| M10 | V1 readiness hardening |

M7 may run in parallel with M2-M6 once M1 ships (inference has no dependency on storage or engine).

## PR Discipline

Every PR:

1. One slice code in the title (e.g., `M3B2: implement object layout`).
2. One owner per changed behavior.
3. Implementation work and matching test work converge within the milestone.
4. Old competing path deleted or explicitly marked transitional with a deletion condition.
5. PR description states the change class (refactor / cutover / intentional semantic change) and assurance class (S4 / S3 / S2).
6. Tests use error codes and classes, not prose messages.
7. No `let _ = ...`, `.ok()`, or `.unwrap_or_default()` without a rationale comment.
8. Aim for ≤1,500 LOC net change per slice. Split larger slices before opening the PR.

Never:

- Add business logic to executor.
- Add a second public way to do the same operation.
- Keep old and new implementations alive without a cutover boundary.
- Mix unrelated changes or multiple milestones in one PR.
- Add migration machinery for pre-V1 databases.
- Skip the matching test slice.
- Mark items `pub` outside the D4 surface without reviewer approval.

Prefer:

- Authority clarity over flexibility.
- Shorter canonical paths over more options.
- Deletion over documentation of obsolete code.
- Moving semantics into engine over wrapping in executor.
- Explicit enums over boolean-control APIs.
- `pub(crate)` by default.
- Integrated behavioral tests over unit tests of internal helpers.

## Workspace Commands

The V1 branch progressively breaks old commands as milestones land. Use what's available for the milestone you're in:

```bash
# Build (workspace may not build cleanly during transition slices)
cargo build -p strata-core             # M1+
cargo build -p strata-storage          # M2+
cargo build -p strata-engine           # M5+
cargo build -p strata-inference        # M7+
cargo build -p strata-intelligence     # M8+

# Test
cargo test -p <crate>
cargo test -p <crate> --test <integration-target>

# Lint
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check

# Feature matrix
cargo hack check -p <crate> --feature-powerset --depth 2

# Conformance harnesses (per milestone)
cargo test -p strata-storage --test format_goldens             # M3+
cargo test -p strata-engine --test product_pathways            # M6+
cargo test -p strata-intelligence --test fake_provider_paths   # M8+
```

Benchmark suites and threshold policy will be re-baselined in M9F/M10D. The old `strata-benchmarks` regression harness still exists but its thresholds apply to the pre-V1 architecture only.

## Out Of V1 Scope

- **Strata Foundry** (SwiftUI macOS app) — on ice during V1. The FFI bridge will be revisited post-V1 once engine APIs stabilize. Do not couple V1 implementation slices to Foundry.
- Network server mode.
- Cross-machine sync / fleet management. StrataHub V1 substrate is metadata-only; sync is post-V1.
- Migration of pre-V1 development databases.
- OpenAI-compatible on-prem endpoint adapter (vLLM, NIM, Ollama, LM Studio, llama.cpp server) — extension point reserved, adapter post-V1.
- Streaming generation — post-V1 unless product pulls it forward.
- Autosearch optimizer — substrate preserved, optimizer post-V1.
- Follower mode (removed).
- Public manual transaction sessions (removed).
- Disk-backed cache mode (removed).
- Branch bundles — replaced by clone artifacts.
- Tags and notes (removed).
- User-facing `strata compact` / `strata checkpoint` / similar manual maintenance commands (removed).

## Skills

| Skill | When to use |
|-------|------------|
| `/implement` | TDD-driven feature implementation from a GitHub issue |
| `/epic-implement` | Execute slices from a milestone implementation plan |
| `/epic-verify` | Verify slice changes — quick (pre-commit) or full (pre-PR) |
| `/audit-fix` | Fix a bug found during a formal audit (pass the issue number) |
| `/review` | General code review |
| `/ultrareview` | Multi-agent cloud review of the current branch |

## Help And Feedback

- `/help` — get help with using Claude Code.
- Issues — https://github.com/anthropics/claude-code/issues
