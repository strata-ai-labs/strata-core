# Storage-Next Test Harnesses

This directory contains the integration harness entry points for storage.
The harnesses stay outside the production API surface and reach crate-private
storage services through the hidden `testkit` feature. They cover backend
selection, durable local reopen recovery, L4 service fault windows, bounded
stress scripts, table-format generated artifacts, and feature boundaries.

Useful local commands:

```bash
cargo test -p strata-storage --locked
cargo test -p strata-storage --features testkit,fault-injection --locked
cargo test -p strata-storage --features testkit --test backend_conformance --locked
cargo test -p strata-storage --features fault-injection --lib service_fault_window_harness_exercises_every_l4_conformance_case --locked
cargo test -p strata-storage --features testkit,fault-injection --test service_fault_windows --locked
cargo test -p strata-storage --features testkit,fault-injection --test commit_runtime_faults --locked
cargo test -p strata-storage --features testkit,fault-injection --test crash_recovery -- --test-threads=1 --nocapture
cargo test -p strata-storage --features testkit,fault-injection --test stress -- --nocapture
```

Feature matrix gate:

```bash
cargo hack -p strata-storage --feature-powerset --depth 2 --locked check --all-targets
cargo test -p strata-storage --no-default-features --features testkit --test backend_conformance --locked
```

The matrix command requires `cargo-hack`. Install it with:

```bash
cargo install cargo-hack --locked
```

This gate checks the no-default memory/cache build, default local filesystem
build, each declared feature, and pairwise feature combinations. `cargo-hack`
runs these combinations with `--no-default-features`, so the default build
appears in the matrix as the `default` feature.

The backend conformance command also proves backend selection fails loudly when
an environment-selected backend is not available in the current feature set.
For example, `localfs` requires the `localfs` feature.

WASM memory/cache compile gate:

```bash
rustup target add wasm32-unknown-unknown
cargo check -p strata-storage --no-default-features --target wasm32-unknown-unknown --all-targets --locked
cargo test -p strata-storage --test testkit_boundary --locked localfs_feature_is_rejected_for_wasm_builds
```

The WASM gate is compile-only. The check command proves the memory/cache build
excludes the local filesystem backend. The boundary test proves a wasm build
with default features fails clearly because `localfs` is not supported on
`wasm32`.

Fuzz package compile gate:

```bash
cargo check --manifest-path crates/storage/fuzz/Cargo.toml --locked --bins
```

Run cargo-fuzz targets locally with nightly:

```bash
cd crates/storage
cargo install cargo-fuzz --locked
cargo +nightly fuzz run format_manifest
cargo +nightly fuzz run format_table_manifest
cargo +nightly fuzz run format_snapshot_envelope
cargo +nightly fuzz run format_storage_row
cargo +nightly fuzz run format_wal_record
```

Supported local environment variables:

| Variable | Purpose |
|---|---|
| `STRATA_STORAGE_TEST_BACKEND` | Select `memory`, `localfs`, or a future backend for conformance tests. `localfs` requires the `localfs` feature. |
| `STRATA_STORAGE_TEST_ROOT` | Override temp root for local filesystem and crash tests. |
| `STRATA_STORAGE_KEEP_TEST_DIR` | Keep temporary directories after a failure. |
| `STRATA_STORAGE_CRASH_CASES` | Limit or expand crash-case count. |
| `STRATA_STORAGE_STRESS_SEED` | Set the stress-test seed. |
| `STRATA_STORAGE_STRESS_SECONDS` | Set the stress-test duration. |

Invalid or empty environment values fail at harness startup instead of being
silently ignored.
