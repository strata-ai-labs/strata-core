# strata-nodesdk — work order (D9, D1)

Prereq reading: `00-shared-contracts.md`. Mirror of the Python work order: the
full cutover is the M9 milestone (the current `@stratadb/core` binds the old
architecture); below is the V1 requirement set plus what can start now.

## Can start now

1. **D1 sweep**: package.json already points at `stratalab` — verify README and
   docs links agree (`stratalab/strata-core` for the engine).
2. **README pointer**: the standard orientation block — Strata is embedded
   (SQLite-shaped, six primitives, branches, time travel); after any binary
   install, `strata agents guide` is the complete offline reference.
3. **Prebuild matrix audit** (napi-rs, exists — carries over): linux gnu+musl
   (x86_64, aarch64), macOS (arm64, x86_64), Windows x86_64. **No toolchain
   required to `npm install @stratadb/core`.**

## At M9 cutover (the V1 package)

### Agent surfaces (the D9 core)

1. **`agentsGuide(): string`** export — the same generated markdown guide as
   `strata agents guide`, embedded at build time from the same engine version.
2. **npm README** — full quickstart inline (agents read `node_modules` and
   registry metadata before the web); taught commands mirror the binary's
   `strata init --json` → `next_steps`.
3. **Complete `.d.ts`** (exists — regenerate for V1): every public method with a
   one-line example in its doc comment, generated from the executor command
   catalog (same metadata as `strata agents commands --json`). Wire shapes are
   discriminated unions on `type` — keep them exactly the executor's envelopes.
4. **Errors teach**: thrown errors expose `code`, `errorClass`, `hint`, and
   `ref` (`https://stratadb.org/e/<code>`), plus `retryPolicy`/`retryable`,
   mapped from the shared error envelope. No `[CODE] message` string parsing —
   that was the old SDK's pattern; V1 errors are structured. Tests assert on
   `code`/`errorClass`, never message text.

### Semantics that must match the binary (P6)

- **Same verbs, same shapes**: one canonical name per operation, executor wire
  shapes, opaque cursors passed back verbatim; Bytes surfaces as
  `Buffer`/`Uint8Array` in the typed API (base64 only on the raw JSON path).
- **Targeting**: `new Strata(path)` is explicit; never open cwd implicitly.
- **Version**: package version equals the engine version (single release train,
  D7); prebuilds published by the release dispatch at the same tag.
- Known M9 design question (flagged in the executor review): the executor is
  `&mut self`, so concurrent Promises serialize per handle — the V1 binding
  must make that explicit (per-handle queue) rather than accidental.

### Golden-path transcript (CI, post-publish)

```
npm install @stratadb/core
node -e "
const { Strata } = require('@stratadb/core');
const db = new Strata('./app-data');
db.kv.put('greeting', 'hello');
console.log(db.kv.get('greeting'));
"
# → hello
```

Run on the full prebuild matrix against the real registry after publish. Add one
error-path assertion (unknown branch → error with
`code === 'not_found.engine.branch'` and a `stratadb.org/e/` ref).

## Acceptance

- `agentsGuide()` output is byte-identical to `strata agents guide` for the
  same version.
- Errors carry code/class/hint/ref; transcript passes on the full matrix.
- Package version == engine version; zero non-`stratalab` GitHub references.
