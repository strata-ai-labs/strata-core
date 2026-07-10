# Storage-Next Open API Shape Plan

Status: draft cleanup plan

## Context

Before `CLN3-O4`, `storage-next` treated
`StorageOpenOptions::default()` as cache mode:

```rust
impl Default for StorageOpenOptions {
    fn default() -> Self {
        Self::cache()
    }
}
```

That made sense while the API boundary was being built because cache mode was
the only storage mode that could open without a caller-provided backend handle.
`StorageRuntime::open(options)` can manufacture an in-memory backend internally.
Durable local mode cannot: it needs a concrete directory-backed
`StorageBackend::local_fs(root)` and therefore goes through
`StorageRuntime::open_with_backend(options, backend)`.

That shape was too easy to misuse. A caller could write
`StorageOpenOptions::default()` or `StorageRuntime::open(Default::default())`
and get a volatile runtime even though most Strata databases should be backed
by an actual directory that survives the process.

The API should make persistence the normal native product path and make cache
mode explicitly ephemeral.

## Problem

Before this cleanup, there were two different defaults:

1. crate feature defaults enable `localfs` on native builds;
2. `StorageOpenOptions::default()` selected volatile cache mode.

Those defaults point in opposite directions. Native builds include the durable
local backend by default, but the no-argument open path chooses an in-memory
runtime.

The result is a product-boundary hazard:

1. production callers can accidentally open a cache-only database;
2. tests and examples can normalize the wrong mode;
3. WASM/no-filesystem constraints leak into the native default;
4. diagnostics correctly report cache mode, but only after the wrong mode has
   already opened.

## Decision

Make storage opening explicit at the low-level API, and make durable local the
normal native convenience path.

The target model is:

1. native product code opens a durable local database from a path;
2. cache mode is named as ephemeral memory/cache mode;
3. WASM/no-localfs builds expose an explicit ephemeral open path;
4. `Default` does not silently choose volatile storage for a database open.

This does not require changing durable format, commit semantics, branch
semantics, recovery semantics, or maintenance semantics. It is an API-shape and
call-site cleanup.

## Target API Shape

The preferred native durable path should be one call:

```rust
let outcome = StorageRuntime::open_local(root)?;
```

or, if durability policy must be explicit:

```rust
let outcome = StorageRuntime::open_durable_local(
    root,
    StorageDurabilityPolicy::Standard,
)?;
```

The low-level backend-injection path should remain for tests and custom
backends:

```rust
let backend = StorageBackend::local_fs(root);
let outcome = StorageRuntime::open_with_backend(
    StorageOpenOptions::durable_local(StorageDurabilityPolicy::Standard),
    &backend,
)?;
```

The ephemeral path should be visually explicit:

```rust
let outcome = StorageRuntime::open_ephemeral()?;
```

or:

```rust
let outcome = StorageRuntime::open_cache()?;
```

The name should make data loss expectations obvious. Prefer
`open_ephemeral()` for product-facing docs and `open_cache()` only if the call
site is testing the cache runtime specifically.

## Default Policy

Do not make `StorageOpenOptions::default()` mean durable local. It has no path,
so it cannot honestly construct a persistent database.

Instead, migrate away from `Default` as an open-mode selector:

1. either remove `impl Default for StorageOpenOptions`; or
2. keep it temporarily but deprecate it with a message that says it opens
   ephemeral cache mode; and
3. forbid new production call sites from using `StorageOpenOptions::default()`
   for runtime open.

The clean long-term state is no `Default` implementation for
`StorageOpenOptions`. Callers should choose one of:

1. `StorageOpenOptions::cache()` for explicit cache tests;
2. `StorageOpenOptions::durable_local(policy)` for backend-injected durable
   tests;
3. `StorageRuntime::open_local(root)` for normal native database open;
4. `StorageRuntime::open_ephemeral()` for explicit volatile storage.

## WASM And No-Localfs Builds

`wasm32` builds cannot use the `localfs` backend. The crate already rejects
`wasm32 + localfs` at compile time.

The WASM path should not silently mimic the native durable default. It should
expose only explicit ephemeral/cache opening until an object-durable or
browser-backed durable backend exists.

Target behavior:

1. native with `localfs`: `open_local(root)` is available;
2. native without `localfs`: `open_local(root)` is not available or returns an
   unsupported-capability error behind an explicit cfg boundary;
3. wasm without `localfs`: `open_ephemeral()` is available;
4. no build silently falls back from durable local to cache.

## Non-Goals

This plan does not:

1. change durable on-disk format;
2. change WAL, manifest, table, quarantine, or snapshot layout;
3. change commit durability semantics;
4. change branch lifecycle behavior;
5. introduce object-durable or distributed storage;
6. make cache mode durable;
7. remove backend injection from tests;
8. change engine persistence contracts outside the storage open boundary.

## Migration Slices

### `CLN3-O1`: Add Explicit Open Constructors

Add native and ephemeral runtime constructors without removing existing APIs:

1. `StorageRuntime::open_ephemeral()`;
2. `StorageRuntime::open_cache()` if useful for test readability;
3. `StorageRuntime::open_durable_local_with_backend(policy, backend)` for
   explicit durable local opens under the current borrowed-backend runtime
   architecture;
4. `StorageOpenOptions::ephemeral()` as an explicit alias for cache-mode open
   options.

This slice intentionally does not add `StorageRuntime::open_local(root)`.
Durable helpers that internally create `StorageBackend::local_fs(root)` require
an owned runtime/open-handle design because the current durable runtime stores
service handles that borrow the backend.

Open question: because `StorageRuntime<'a>` currently borrows a
`StorageBackend`, durable helpers cannot return a runtime borrowing a local
backend value unless the runtime owns that backend or the helper returns an
owned open handle. `CLN3-O1` must choose one of these designs before editing:

1. add an owned-backend runtime variant for helper-created backends;
2. add a `StorageOpenHandle` that owns the backend and runtime together;
3. keep only an options-level helper and leave runtime durable open through
   explicit backend injection.

`CLN3-O1` chooses option 3 for the safe additive slice. The preferred product
shape remains an owned handle. A one-call durable open that requires the caller
to manually keep a backend value alive is not ergonomic enough for the default
product path, so that work should land as a separate ownership slice before
`StorageRuntime::open_local(root)` is introduced.

`CLN3-O5` resolves that ownership blocker by adding an internal
owned-or-borrowed backend handle. That lets `StorageRuntime::open_local(root)`
construct a localfs backend, retain it through durable service handles, and
return a durable runtime without leaking memory or relying on self-referential
storage.

### `CLN3-O2`: Rename Or Document Ephemeral Cache Mode

Make volatile storage impossible to miss:

1. add docs to `StorageMode::Cache`;
2. add docs to `StorageOpenOptions::cache()`;
3. update examples and tests that are not cache-specific to call an explicit
   helper;
4. reserve "cache" wording for runtime internals and cache-specific tests;
5. use "ephemeral" in product-facing APIs and docs.

This slice should not change behavior.

### `CLN3-O3`: Remove Default From Open Call Sites

Replace `StorageOpenOptions::default()` in storage-next tests and docs with
explicit mode constructors.

Allowed replacements:

1. cache-specific tests: `StorageOpenOptions::cache()`;
2. durable tests: `StorageOpenOptions::durable_local(policy)`;
3. product examples: the owned durable-local open helper after the ownership
   slice adds it;
4. WASM examples: `StorageRuntime::open_ephemeral()`.

This is a mechanical clarity slice. It should not change runtime behavior.

### `CLN3-O4`: Deprecate Or Remove `Default`

After call sites are explicit, choose the compatibility policy.

Chosen final state:

```rust
// no impl Default for StorageOpenOptions
```

`CLN3-O4` removes the implementation outright because storage-next call sites
are explicit after `CLN3-O3`.

If removing `Default` creates too much external churn for the current release,
deprecate it first:

```rust
#[deprecated(
    note = "StorageOpenOptions::default() opens ephemeral cache mode; choose cache(), ephemeral(), or durable_local() explicitly"
)]
impl Default for StorageOpenOptions { ... }
```

If Rust deprecation on an impl is not practical for this shape, add an
`api_source_guard` test that rejects `StorageOpenOptions::default()` in public
examples and production call sites until the implementation can be removed.

### `CLN3-O5`: Engine/Product Boundary Update

Update the engine-facing open path so native Strata databases use durable local
by default.

`CLN3-O5` implements the storage-next side of this boundary by adding
`StorageRuntime::open_local(root)` and
`StorageRuntime::open_durable_local(root, policy)`. These helpers construct and
own the local backend inside the storage runtime, so callers that have a native
database directory can open durable-local storage directly. They never fall
back to cache mode; builds without `localfs` return an explicit unsupported
capability error. The old `strata-engine` crate still uses the pre-V1 storage
crate, so product cutover to storage-next remains owned by the later
engine-next milestones.

Expected product rule:

1. if the caller gives a database directory, open durable local;
2. if the caller wants ephemeral storage, require an explicit ephemeral flag or
   constructor;
3. if durable local is unavailable in the build, return an explicit
   unsupported capability error;
4. never silently downgrade from durable local to cache.

This slice may touch engine-next or higher-level product contracts, so it
should be separate from low-level storage API cleanup.

### `CLN3-O6`: Source Guards And Documentation Closeout

Add guard coverage after the migration:

1. source guard rejecting new production use of `StorageOpenOptions::default()`;
2. source guard rejecting silent durable-to-cache fallback strings;
3. docs showing native durable open first;
4. docs showing ephemeral open only as explicit volatile storage;
5. tests for `open_local` failure when `localfs` is unavailable, if applicable.

`CLN3-O6` closes the storage-next side by documenting durable-local native
opens before explicit ephemeral opens in the crate/API rustdoc, guarding
production source against `StorageOpenOptions::default()` open paths, and
guarding API source against silent durable-to-cache fallback wording. The
no-`localfs` rejection test for `open_local` landed with `CLN3-O5`.

## Compatibility Strategy

The migration should be additive before it is subtractive.

Phase order:

1. add explicit APIs;
2. update docs and internal call sites;
3. update engine/product call sites;
4. add source guards;
5. remove or deprecate `Default`.

This avoids mixing API introduction with breaking call-site churn.

## Verification Floor

Every slice should run:

```sh
cargo check -p strata-storage-next --locked
cargo check -p strata-storage-next --locked --no-default-features
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo fmt --all -- --check
```

If the slice changes public API surface or source guards, also run:

```sh
cargo test -p strata-storage-next --locked --test api_source_guard
cargo test -p strata-storage-next --locked --test api_conformance
```

If the slice changes durable local open behavior, also run focused durable
tests under the default `localfs` feature:

```sh
cargo test -p strata-storage-next --locked --lib api::tests::diagnostics
cargo test -p strata-storage-next --locked --lib api::tests::maintenance
```

If the slice changes no-default-features behavior, add or run a focused
no-default-features check that proves cache/ephemeral opening still compiles.

## Acceptance Criteria

The work is complete when:

1. native product docs and examples open durable local from a directory;
2. volatile storage is named `ephemeral` or otherwise explicitly described;
3. no production call site uses `StorageOpenOptions::default()` to open a
   runtime;
4. no API silently falls back from durable local to cache;
5. WASM/no-localfs builds expose explicit ephemeral opening;
6. source guards prevent regression;
7. durable format and behavior tests remain unchanged except for open-path
   construction.

## Review Questions

Before implementing each slice, answer:

1. Does this change make persistence more explicit?
2. Could a caller still accidentally open cache mode when they expected a
   durable database?
3. Does this create a misleading target-specific default?
4. Does no-default-features still have an explicit path?
5. Are tests using cache because they need cache, or because it was the old
   default?

The goal is not to make every environment durable. The goal is to make storage
mode a deliberate choice and make native durable local the obvious Strata
database path.
