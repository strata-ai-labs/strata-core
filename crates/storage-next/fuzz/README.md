# Storage-Next Fuzz Targets

This directory contains the storage-next cargo-fuzz package.

The first targets exercise the current durable byte decoders through the hidden
`testkit` surface. They are fail-closed parser fuzzers: arbitrary bytes may
decode or reject, but they must not panic, allocate without decoder limits, or
accept malformed checksums as valid. Service targets route generated operation
scripts through hidden L4 testkit harnesses and assert model invariants after
each step.

Useful local commands:

```bash
cargo install cargo-fuzz --locked
cargo +nightly fuzz run format_manifest
cargo +nightly fuzz run format_snapshot_envelope
cargo +nightly fuzz run format_storage_row
cargo +nightly fuzz run format_wal_record
cargo +nightly fuzz run service_snapshot
```

The fuzz package uses `default-features = false` so parser fuzzing also covers
the memory/cache-compatible build surface. Format targets should stay named for
the byte-oriented durable input they fuzz, such as `object_name_parse`,
`format_table_block`, `format_timeline_row`, and `recovery_object_inventory`.
Service targets should stay named for the service family they script, such as
`service_snapshot`.
