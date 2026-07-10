# Fuzz Targets

Fuzz targets in this directory are cargo-fuzz binaries. Each target should call
the hidden storage testkit surface and route arbitrary bytes through one
durable decoder family or bounded service operation script.

Current targets:

1. `format_manifest`
2. `format_quarantine`
3. `format_snapshot_envelope`
4. `format_storage_row`
5. `format_table_artifact`
6. `format_table_block`
7. `table_runtime_reader`
8. `table_runtime_cursor`
9. `table_runtime_compaction`
10. `format_wal_commit_payload`
11. `format_wal_record`
12. `service_quarantine`
13. `service_snapshot`

L5 table-runtime generated behavior is exercised through
`tests/table_runtime_properties.rs`, `table_runtime_cursor`, and
`table_runtime_compaction`; reader fail-closed byte handling is exercised by
`table_runtime_reader`; M3G byte-level table coverage is exercised by
`format_table_artifact` and `format_table_block`.

Add new targets only after the corresponding parser exists and has normal unit
or golden-vector coverage. Service targets also need normal service-level tests
for the same operation families before they are added here.
