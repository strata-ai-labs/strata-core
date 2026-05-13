# Storage-Next Fuzz Targets

This directory reserves the storage-next fuzz harness location.

The initial scaffold does not create a cargo-fuzz package because no durable
byte codecs are implemented yet. Once byte-oriented parsers exist, add a
cargo-fuzz package here with targets named for durable inputs such as
`object_name_parse`, `format_wal_record`, `format_manifest`,
`format_snapshot_envelope`, `format_commit_payload`, `format_table_block`,
`format_timeline_row`, and `recovery_object_inventory`.
