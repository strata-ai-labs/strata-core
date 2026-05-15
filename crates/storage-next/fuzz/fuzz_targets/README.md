# Fuzz Targets

Fuzz targets in this directory are cargo-fuzz binaries. Each target should call
the hidden storage-next testkit surface and route arbitrary bytes through one
durable decoder family or bounded service operation script.

Current targets:

1. `format_manifest`
2. `format_snapshot_envelope`
3. `format_storage_row`
4. `format_wal_record`
5. `service_snapshot`

Add new targets only after the corresponding parser exists and has normal unit
or golden-vector coverage. Service targets also need normal service-level tests
for the same operation families before they are added here.
