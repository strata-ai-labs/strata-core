# strata-python — work order (D9, D1)

Prereq reading: `00-shared-contracts.md`. The full SDK cutover is the **M9
milestone** in strata-core's V1 roadmap (the wheel currently binds the old
architecture — that stays until cutover). This document covers (a) what the V1
wheel must ship for the agent-first story, and (b) what can start now.

## Can start now

1. **D1 sweep**: all GitHub URLs → `stratalab/…` (`stratalab/strata-core` for
   the engine), in `pyproject.toml`, README, docs links.
2. **README pointer**: add the standard orientation block — Strata is embedded
   (SQLite-shaped, six primitives, branches, time travel); after any binary
   install, `strata agents guide` is the complete offline reference.
3. **Wheel matrix audit** (carries over unchanged to V1): manylinux + musllinux
   (x86_64, aarch64), macOS (arm64, x86_64), Windows x86_64; abi3 so one wheel
   per platform. **No Rust toolchain ever required to `pip install stratadb`.**

## At M9 cutover (the V1 wheel)

### Agent surfaces (the D9 core)

1. **`stratadb.agents_guide() -> str`** — returns the same generated markdown
   guide as `strata agents guide`, embedded in the wheel at build time from the
   same engine version. Version-locked: the guide can never disagree with the
   bound engine.
2. **PyPI long-description** — embed the full quickstart inline (agents read
   `site-packages` and package metadata before the web). The canonical taught
   commands mirror the binary's `strata init --json` → `next_steps`.
3. **`py.typed` + complete stubs** — agents read type stubs before docs. Every
   public method carries a docstring with a one-line example; docstrings for
   generated methods come from the executor command catalog (same metadata as
   `strata agents commands --json`).
4. **Errors teach**: exceptions expose `code`, `error_class`, `hint`
   (suggested_fix), and `ref` (`https://stratadb.org/e/<code>`), mapped from the
   shared error envelope (§4 of shared contracts). Tests assert on `code`/
   `error_class`, never message text. Retry metadata (`retry_policy`,
   `retryable`) is available for agent retry loops.

### Semantics that must match the binary (P6)

- **Same verbs, same shapes**: the Python surface mirrors the executor command
  surface — one canonical name per operation, the same JSON value shapes, the
  same pagination model (opaque cursors passed back verbatim).
- **Targeting**: `stratadb.Strata(path)` is explicit by nature; honor
  `STRATA_DB` only for an explicit no-arg convenience *if* offered, and never
  open cwd implicitly.
- **Version**: the wheel version equals the engine version (single release
  train, D7); wheels are built and published by the release dispatch at the
  same tag.

### Golden-path transcript (CI, post-publish)

```
uv add stratadb   # and: pip install stratadb
python -c "
import stratadb
db = stratadb.Strata('./app-data')
db.kv.put('greeting', 'hello')
print(db.kv.get('greeting'))
"
# → hello
```

Run on every platform in the wheel matrix **against the real registry** after
publish, not only pre-publish. Add one error-path assertion (unknown branch →
exception with `code == "not_found.engine.branch"` and a `stratadb.org/e/` ref).

## Notes from the current repo's architecture memory

- The PyO3 `_Strata` class has no `__dict__`; the Python `Strata` wraps it by
  composition with `__getattr__` delegation, and namespace objects (`db.kv`,
  `db.state`, …) are lazily cached. Whether this shape survives M9 is an M9
  design decision — the D9 requirements above are shape-independent.

## Acceptance

- `stratadb.agents_guide()` output is byte-identical to `strata agents guide`
  for the same version.
- Exceptions carry code/class/hint/ref; transcript passes on the full matrix.
- Wheel version == engine version; zero non-`stratalab` GitHub references.
