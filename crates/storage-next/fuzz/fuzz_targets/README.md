# Fuzz Targets

Fuzz targets in this directory are cargo-fuzz binaries. Each target should call
the hidden storage-next testkit surface and route arbitrary bytes through one
durable decoder family.

Current targets:

1. `format_manifest`
2. `format_snapshot_envelope`
3. `format_storage_row`
4. `format_wal_record`

Add new targets only after the corresponding parser exists and has normal unit
or golden-vector coverage.
