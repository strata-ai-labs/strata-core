# Executor Arrow Import/Export Test Plan

## Purpose

Prove that Arrow import/export is a stable default executor feature and that
bulk file data movement uses normal executor commands across the current core
primitive set. The explicit no-default-features build remains covered as a
minimal opt-out configuration.

The tests must verify:

- stable command and output serialization
- feature-disabled behavior in the explicit minimal build
- Parquet, CSV, and JSONL reader/writer behavior
- column mapping compatibility with the old executor
- KV, JSON, and vector import
- KV, JSON, event, vector, and graph export
- branch and space isolation
- durable-local persistence after import
- no direct storage or engine-internal bypass

## Test Matrix

| Area | Minimal Opt-Out | Default Arrow | Cache | Durable Local |
| --- | --- | --- | --- | --- |
| Command serde | Required | Required | Required | Required |
| Output serde | Required | Required | Required | Required |
| Feature-disabled execution | Required | Not applicable | Required | Required |
| Format detection | Not applicable | Required | Unit | Unit |
| Reader/writer round trip | Not applicable | Required | File fixtures | File fixtures |
| Column mapping | Not applicable | Required | Unit | Unit |
| KV import | Not applicable | Required | Required | Required |
| JSON import | Not applicable | Required | Required | Required |
| Vector import | Not applicable | Required | Required | Required |
| KV export | Not applicable | Required | Required | Required |
| JSON export | Not applicable | Required | Required | Required |
| Event export | Not applicable | Required | Required | Required |
| Vector export | Not applicable | Required | Required | Required |
| Graph export | Not applicable | Required | Required | Required |
| Source guards | Required | Required | Static | Static |

## Suggested Test Files

- `crates/executor-next/tests/arrow_command_contract.rs`
- `crates/executor-next/tests/arrow_behavior.rs`
- `crates/executor-next/tests/error_and_guards.rs`
- `crates/executor-next/src/arrow/*` unit tests behind `#[cfg(feature = "arrow")]`

It is acceptable to keep command contract additions in the existing
`command_contract.rs` if that remains the local convention.

## Contract Tests

### Command Round Trip

Add serde round-trip coverage for:

- `ArrowImport`
- `ArrowExport`

Import command cases:

- omitted branch and space
- explicit branch and space
- target `kv`
- target `json`
- target `vector`
- explicit key column
- explicit value column
- collection set for vector
- format omitted
- format `parquet`
- format `csv`
- format `jsonl`

Export command cases:

- omitted branch and space
- explicit branch and space
- primitive `kv`
- primitive `json`
- primitive `event`
- primitive `vector`
- primitive `graph`
- format `parquet`
- format `csv`
- format `jsonl`
- prefix present
- limit present
- collection present
- graph present
- path present

Assertions:

- Serialized command uses stable snake-case type tags:
  - `arrow_import`
  - `arrow_export`
- Unknown fields fail deserialization.
- Deserialized command equals original command.
- `Command::name()` includes exactly one entry per Arrow command.

### Output Round Trip

Add serde round-trip coverage for:

- `ArrowImportResult`
- `ArrowExportResult`

Import result cases:

- zero rows imported
- imported rows with skipped rows
- multiple batches processed
- each import target

Export result cases:

- one output path
- graph node and edge output paths
- zero row export
- nonzero size bytes
- each export primitive
- each file format

Assertions:

- Output type tags are stable.
- Optional path and size fields serialize predictably.
- Graph export reports both generated paths.

## Minimal Opt-Out Tests

Run without default features.

Cases:

- `ArrowImport` command serde works.
- `ArrowExport` command serde works.
- Executing `ArrowImport` on an existing file returns a stable
  feature-disabled error.
- Executing `ArrowExport` returns a stable feature-disabled error.
- Missing input file still returns invalid input if that behavior is preserved
  from the old executor.
- Error messages do not mention storage rows, engine data modules, table names,
  WAL, lifecycle, compaction, or persistence internals.

Gate command:

```sh
cargo test -p strata-executor-next --no-default-features arrow
```

If `localfs` is needed for durable executor fixtures, use a focused no-feature
test that avoids durable open.

## Arrow Unit Tests

Run with default features.

### Format Detection

- `.parquet` detects Parquet.
- `.csv` detects CSV.
- `.jsonl` detects JSONL.
- `.json` detects JSONL.
- unknown extension is invalid input.
- missing extension is invalid input with a format hint.
- explicit format overrides extension.
- explicit unknown format fails.

### Reader Tests

For each format:

- read a single batch
- read multiple rows
- infer schema correctly
- preserve column names
- reject missing file
- reject malformed file with a stable error class

### Writer Tests

For each format:

- write a non-empty batch
- file exists afterward
- size is nonzero
- read back with the Arrow reader and assert row count
- write multiple batches
- empty batch list is invalid input
- parent directory must exist

### Column Mapping Tests

Key mapping:

- auto-detect `key`
- auto-detect `_id`
- auto-detect `id`
- explicit key column
- missing key column lists available columns
- null key row is skipped during import

KV mapping:

- auto-detect `value`
- explicit value column
- two-column shortcut
- extra columns become JSON bytes
- binary value column imports as raw bytes
- UTF-8 string value imports as string bytes
- numeric and boolean values import as UTF-8 display bytes
- generated `key_encoding` and `value_encoding` columns decode exported
  `base64` rows and pass through exported `utf8` rows

JSON mapping:

- auto-detect `document`
- auto-detect `value`
- auto-detect `doc`
- auto-detect `body`
- explicit document column
- no document column builds an object from extra columns
- document string parses as JSON object
- invalid JSON document string becomes JSON string

Vector mapping:

- auto-detect `embedding`
- auto-detect `vector`
- auto-detect `embeddings`
- auto-detect `emb`
- explicit embedding column
- `FixedSizeList<Float32>` accepted
- `FixedSizeList<Float64>` accepted and downcast to `f32`
- `List<Float32>` accepted
- `List<Float64>` accepted and downcast to `f32`
- non-list embedding column rejected
- list with non-float inner type rejected
- null embedding row skipped

### Arrow Value Conversion

- Utf8 and LargeUtf8
- Binary and LargeBinary
- signed integers
- unsigned integers
- Float32 and Float64
- bool
- null
- list fallback
- struct fallback

## Behavior Tests

Run behavior tests in both cache and durable-local fixtures unless the test is
explicitly file-format-only.

### KV Import

For CSV, JSONL, and Parquet:

- import rows into default branch and default space.
- verify keys through `KvBatchGet`.
- verify values match expected bytes.
- import with explicit branch and space.
- verify omitted branch uses executor default branch.
- verify explicit branch overrides executor default branch.
- verify null key rows are skipped and counted.
- verify duplicate keys follow normal `KvBatchPut` semantics.
- verify extra columns become JSON object bytes.
- verify binary Parquet values round-trip without UTF-8 conversion.

### JSON Import

For CSV, JSONL, and Parquet:

- import document-column rows.
- read documents through `JsonBatchGet`.
- verify nested documents survive.
- import extra-column rows and verify object construction.
- import invalid JSON document strings and verify JSON string storage.
- verify null key rows are skipped.
- verify explicit branch and space isolation.
- verify one Arrow batch maps to one executor JSON batch command using source
  guards or an instrumented test helper if available.

### Vector Import

For CSV, JSONL, and Parquet where format supports the needed list shape:

- import fixed-size list embeddings.
- import variable list embeddings.
- auto-create collection with cosine metric and first valid dimension.
- import into an existing collection.
- verify values through `VectorBatchGet`.
- verify metadata from extra columns.
- verify null embeddings are skipped.
- verify missing collection argument is invalid input.
- verify missing embedding column is invalid input.
- verify dimension mismatch uses normal vector command error mapping.

CSV may need a documented vector encoding rule if Arrow CSV cannot represent
list columns directly. If no robust CSV list representation exists, vector CSV
import should be rejected with a specific invalid-input error and tested that
way.

### KV Export

For CSV, JSONL, and Parquet:

- export empty KV space and assert row count zero.
- export non-empty KV space.
- verify file path and byte count in output.
- read exported file and assert row count.
- verify prefix and limit.
- verify version and timestamp fields when included.
- verify Parquet binary key/value fields preserve bytes.
- verify CSV/JSONL string encodings report `utf8` or `base64` correctly.
- import exported file into a fresh database and verify data, including
  non-UTF-8 bytes that require `base64` decoding.

### JSON Export

For CSV, JSONL, and Parquet:

- export empty JSON space.
- export nested documents.
- verify document column is valid JSON text.
- verify prefix and limit.
- import exported file into a fresh database and verify documents.
- verify branch and space isolation.

### Event Export

For CSV, JSONL, and Parquet:

- append multiple events.
- export all events.
- verify sequence ordering.
- verify event type.
- verify payload JSON text.
- verify timestamp, version, hash, and previous hash fields if present.
- verify limit.
- verify exported event rows do not mutate the event log.

### Vector Export

For CSV, JSONL, and Parquet where format supports the selected representation:

- create a collection.
- upsert vectors with metadata.
- export collection.
- verify key, embedding, metadata, version, timestamp, and revision fields.
- verify limit.
- read back exported file and assert row count.
- import exported file into a fresh database and verify query/get behavior.
- missing collection argument is invalid input.

If CSV cannot represent list values cleanly, export vector CSV should either
use an explicit JSON-string embedding column or return a documented
invalid-input error. The test must lock whichever contract is chosen.

### Graph Export

For CSV, JSONL, and Parquet:

- create graph.
- create nodes with properties.
- create nodes with entity bindings.
- create edges with weights and properties.
- include a self-loop.
- export graph.
- assert output reports two paths.
- read node file and assert node rows.
- read edge file and assert edge rows.
- assert self-loop appears once in the edge file.
- assert node binding fields are present.
- assert edge source, target, edge type, weight, and properties are present.
- verify limit applies consistently and is documented for nodes and edges.
- missing graph argument is invalid input.

### Branch and Space Isolation

- import into one branch and assert another branch is unchanged.
- import into one space and assert another space is unchanged.
- export from one branch and assert only that branch's rows appear.
- export from one space and assert only that space's rows appear.
- run these checks for at least KV, JSON, vector, and graph.

### Durable Reopen

For durable-local fixtures:

- import KV, JSON, and vector files.
- close executor cleanly.
- reopen same path.
- verify imported rows are still visible.
- export after reopen.
- read exported file and verify row count.

## Error Tests

- missing import file
- missing export parent directory
- unknown format
- unsupported extension
- missing key column
- missing vector collection argument
- missing graph argument
- unsupported import target
- unsupported export primitive, if represented
- malformed Parquet
- malformed CSV
- malformed JSONL
- invalid embedding type
- null key skip counting
- vector dimension mismatch
- command execution failure preserves executor error class
- public error text does not expose storage or engine internals

## Source Guard Tests

Extend `error_and_guards.rs`:

- `crates/executor-next/src/arrow/**` must not import `strata_storage_next`.
- Arrow code must not import `strata_engine_next::data`.
- Arrow code must not call product engine service methods directly.
- Arrow code must not call storage commits, storage runtime, tables, WAL,
  lifecycle, compaction, or row-key builders.
- Arrow import must contain serialized batch command construction:
  - `Command::KvBatchPut`
  - `Command::JsonBatchSet`
  - `Command::VectorBatchUpsert`
- Arrow export must contain serialized read command construction:
  - `Command::KvScan` or `Command::KvBatchGet`
  - `Command::JsonList`
  - `Command::JsonBatchGet`
  - `Command::EventRange`
  - `Command::VectorListKeys`
  - `Command::VectorBatchGet`
  - `Command::GraphListNodes`
  - `Command::GraphNeighbors`
- Arrow tests and benchmarks must not write storage rows directly.

## Regression Tests Against Old Behavior

- Format extension mapping matches old behavior.
- Key auto-detection order matches old behavior.
- JSON document auto-detection order matches old behavior.
- Vector embedding auto-detection order matches old behavior.
- Vector missing collection argument fails.
- Vector missing collection auto-create behavior matches old behavior once a
  collection argument is provided.
- Graph export creates node and edge files from one command.
- Arrow disabled feature errors include rebuild hints.

## Gate Commands

Minimal opt-out focused gates:

```sh
cargo test -p strata-executor-next --no-default-features arrow
```

Default Arrow focused gates:

```sh
cargo test -p strata-executor-next arrow
```

Executor full feature gates:

```sh
cargo test -p strata-executor-next --all-features
cargo clippy -p strata-executor-next --all-features --all-targets -- -D warnings
```

Formatting and whitespace:

```sh
cargo fmt --package strata-executor-next
git diff --check
```

If root feature wiring is updated, also run:

```sh
cargo test -p strata-executor-next
```

## Done Criteria

- Arrow commands are stable serialized executor commands.
- Arrow outputs are stable serialized executor outputs.
- The crate builds and tests with Arrow in the default feature set.
- The crate builds and tests without default features.
- KV, JSON, and vector import work through normal executor batch commands.
- KV, JSON, event, vector, and graph export work through normal executor read
  commands.
- Graph export produces separate node and edge files.
- Branch and space isolation hold for import and export.
- Durable-local imported data survives reopen.
- Source guards prove no storage or engine-internal bypass.
- Focused Arrow gates and full executor gates pass.
