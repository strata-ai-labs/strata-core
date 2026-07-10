# L6B Test Plan: Branch Row Identity And Read Bounds

Status: implemented for L6B, pending recorded sensitivity probes

Parent plan:
`docs/architecture/implementation-plans/M4/L6/l6b-branch-row-identity-read-bounds-implementation-plan.md`

## Goal

Prove that L6B provides correct, storage-native branch row identity,
branch-id rewrite, and read-bound helper behavior without implementing branch
state or final MVCC reads.

The suite must fail if L6B:

1. accepts a row from the wrong branch for own-branch use;
2. rewrites branch ids by dropping or changing row metadata;
3. treats branch ids, spaces, storage-space ids, or user keys as product
   branch/key concepts;
4. loses the fork-version cap for inherited reads;
5. uses exclusive comparison where version/timestamp bounds are inclusive;
6. interprets tombstones or expiry as final live-value policy;
7. constructs old `VersionedValue` or product DTOs;
8. imports backend, filesystem, object layout, lifecycle, commit runtime, or
   engine APIs;
9. exposes public production branch API.

## Test Locations

Use these locations:

1. `crates/storage-next/src/branch/tests.rs` for module-local direct tests.
2. `crates/storage-next/tests/branch_lsm_source_guard.rs` for L6 boundary
   source scans and executable guard probes.
3. `crates/storage-next/src/testkit/branch_lsm.rs` for generated row
   identity/read-bound script checks.
4. `crates/storage-next/tests/branch_lsm_properties.rs` for generated tests
   behind the `testkit` feature.
5. `crates/storage-next/proptest-regressions/branch_lsm.txt` only if a
   generated failure captures a minimized seed.
6. `docs/architecture/implementation-plans/M4/L6/m4-l6-porting-log.md` for
   sensitivity-probe and source-map recording.

Tests must use storage-next `StorageRow`, `PhysicalKey`, `StorageSpaceId`,
`BranchId`, `CommitVersion`, and `Timestamp`. They must not use old storage
`Key`, `Value`, `Namespace`, `TypeTag`, `VersionedValue`, or engine branch
workflow types.

## Generators

### Branch IDs

Generate opaque branch ids:

1. all zero bytes;
2. all `0xff` bytes;
3. incrementing bytes;
4. one-bit differences;
5. pairs that differ in the first byte;
6. pairs that differ in the last byte;
7. repeated source/target branch ids for same-branch rewrite cases.

No generator should assign branch names or lifecycle meaning.

### Physical Keys

Generate physical keys over:

1. valid nonempty space strings;
2. space strings with shared prefixes;
3. storage-reserved nonzero `StorageSpaceId` values;
4. engine-owned `StorageSpaceId` values;
5. empty user keys;
6. user keys containing `0x00`;
7. user keys containing `0x00 0x00`;
8. high-bit user-key bytes;
9. long shared-prefix user keys.

Invalid space construction is already owned by the row layer. L6B may smoke
test row-constructor errors but should focus on branch identity.

### Storage Rows

Generate rows with:

1. put rows with empty, small, and bounded random values;
2. tombstone rows;
3. adjacent commit versions;
4. `CommitVersion::new(0)`;
5. `CommitVersion::MAX`;
6. timestamps before, equal to, and after requested bounds;
7. expiry `Timestamp::EPOCH`;
8. nonzero expiry on put rows;
9. equal timestamps across different versions;
10. timestamps that do not correlate with commit version.

Row-chain model tests should sort rows using L5 encoded internal-key order or a
local equivalent, but L6B itself should not select a final visible row.

### Read Bounds

Generate caller bounds:

1. latest;
2. exact version zero;
3. exact version one;
4. exact adjacent versions around row versions;
5. `CommitVersion::MAX`;
6. exact timestamp epoch;
7. exact timestamps around row timestamps;
8. very large timestamps.

Generate fork versions independently from requested version and timestamp so
tests cover requested version below, equal to, and above fork version.

## Required Direct Tests

### 1. Branch-Local Physical-Key Validation

1. A physical key whose branch id matches the expected branch is accepted.
2. A physical key whose branch id differs from the expected branch is rejected.
3. Branch mismatch error is `InvalidBranchRow` or the chosen typed branch-row
   error.
4. Branch mismatch display does not include value bytes or product branch
   names.
5. Validation ignores space name except preserving it in the key.
6. Validation ignores storage-space id except preserving it in the key.
7. Validation accepts storage-owned and engine-owned nonzero storage-space ids.
8. Validation accepts empty user-key bytes.
9. Validation accepts user-key bytes containing `0x00`.
10. Validation treats branch id bytes as opaque and does not normalize them.

### 2. Branch-Local Row Validation

1. A put row in the expected branch is accepted.
2. A tombstone row in the expected branch is accepted.
3. A wrong-branch put row is rejected before any state mutation.
4. A wrong-branch tombstone row is rejected before any state mutation.
5. Row validation preserves commit version.
6. Row validation preserves commit timestamp.
7. Row validation preserves expiry.
8. Row validation preserves value bytes for put rows.
9. Row validation preserves tombstone shape.
10. Row validation does not interpret TTL or tombstone visibility.

### 3. Physical-Key Branch Rewrite

1. Rewriting a physical key changes only branch id.
2. Rewriting preserves space string exactly.
3. Rewriting preserves storage-space id exactly.
4. Rewriting preserves user-key bytes exactly, including embedded zero bytes.
5. Rewriting to the same branch returns an equal key.
6. Rewriting source -> target -> source returns the original key.
7. Rewritten key sorts under the target branch id when encoded by L5/L3
   helpers.
8. Rewritten key shares the same logical space/storage-space/user-key facts as
   the source key.

### 4. Storage-Row Branch Rewrite

1. Rewriting a put row changes only the physical-key branch id.
2. Rewriting a tombstone row changes only the physical-key branch id.
3. Rewriting preserves commit version.
4. Rewriting preserves commit timestamp.
5. Rewriting preserves expiry.
6. Rewriting preserves put value bytes, including empty values.
7. Rewriting preserves tombstone flag and does not invent value bytes.
8. Rewriting rejects a row whose source branch does not match the supplied
   source branch id.
9. Rewriting to the same branch is accepted.
10. Rewriting source -> target -> source returns an equal row for valid rows.
11. Rewritten rows with the same logical key can group with child-local rows
    after encoding.

### 5. Own-Branch Effective Read Bounds

1. Latest produces no version cap and no timestamp cap.
2. `AtVersion(v)` produces version cap `v` and no timestamp cap.
3. `AtTimestamp(t)` produces timestamp cap `t` and no version cap.
4. Version cap accepts row version exactly equal to `v`.
5. Version cap rejects row version greater than `v`.
6. Version cap accepts row version below `v`.
7. Timestamp cap accepts row timestamp exactly equal to `t`.
8. Timestamp cap rejects row timestamp greater than `t`.
9. Timestamp cap accepts row timestamp below `t`.
10. `CommitVersion::new(0)` and `Timestamp::EPOCH` caps work as exact bounds.

### 6. Inherited Effective Read Bounds

1. Inherited latest produces version cap `fork_version`.
2. Inherited latest accepts row version equal to `fork_version`.
3. Inherited latest rejects row version above `fork_version`.
4. Inherited `AtVersion(v)` where `v < fork_version` caps at `v`.
5. Inherited `AtVersion(v)` where `v == fork_version` caps at `v`.
6. Inherited `AtVersion(v)` where `v > fork_version` caps at `fork_version`.
7. Inherited `AtTimestamp(t)` preserves timestamp cap `t`.
8. Inherited `AtTimestamp(t)` also applies version cap `fork_version`.
9. A row for inherited `AtTimestamp(t)` must satisfy both timestamp and fork
   version caps.
10. The effective bound type can represent both caps at the same time.

### 7. Row Bound-Match Facts

1. Candidate fact construction records whether version is in bound.
2. Candidate fact construction records whether timestamp is in bound.
3. Combined effective bound reports match only when both caps match.
4. Latest own-branch bound matches every generated row by version/timestamp.
5. Candidate facts preserve source enum exactly.
6. Candidate facts preserve physical key.
7. Candidate facts preserve commit version.
8. Candidate facts preserve commit timestamp.
9. Candidate facts preserve expiry.
10. Candidate facts preserve tombstone flag.
11. Candidate facts do not hide a tombstone that is in bound.
12. Candidate facts do not hide an expired-looking put row that is in bound.

### 8. Row-Chain Model Checks

Build generated chains for one logical physical key with many versions.

1. Filtering by effective version cap leaves only rows with
   `commit_version <= cap`.
2. Filtering by effective timestamp cap leaves only rows with
   `commit_timestamp <= cap`.
3. Filtering by both caps is the intersection of version and timestamp
   filtering.
4. Wrong-branch rows are rejected before bound filtering.
5. Same-branch rows with tombstones remain in the candidate set when in bound.
6. Same-branch expired-looking rows remain in the candidate set when in bound.
7. Candidate rows remain sorted newest-first for the same physical key when
   input is sorted by L5 internal key bytes.
8. The model never collapses many row versions into a single visible result.

### 9. Boundary And Source Guards

Extend or maintain `branch_lsm_source_guard.rs` coverage:

1. production branch code does not import `crate::commit`;
2. production branch code does not import `crate::lifecycle`;
3. production branch code does not import `crate::api`;
4. production branch code does not import engine crates;
5. production branch code does not import `crate::backend`;
6. production branch code does not call backend operation names in bare-call or
   method-call form;
7. production branch code does not call filesystem/path/env APIs;
8. production branch code does not use product DTO vocabulary;
9. production branch code remains `pub(crate)`;
10. source guard self-tests catch at least one new L6B-forbidden mutation, such
    as `VersionedValue` in a row helper or `read_object(row)`.

The L6A scaffold-only premature behavior guard should not block L6B pure
helpers. It should still reject premature read/materialization/compaction
entrypoints in scaffold files.

### 10. Generated Testkit Coverage

The branch-lsm testkit route should produce nonzero counters for:

1. matching branch-row validations;
2. mismatching branch-row validations;
3. physical-key rewrites;
4. storage-row rewrites;
5. own latest bounds;
6. own version bounds;
7. own timestamp bounds;
8. inherited latest bounds;
9. inherited version bounds;
10. inherited timestamp plus fork-version bounds;
11. candidate facts for put rows;
12. candidate facts for tombstone rows;
13. storage-owned ids and empty user-key edge rows;
14. rewritten inherited rows grouping with child-local rows after encoding;
15. multi-version row-chain filtering without visible-row collapse;
16. inherited fork edge cases below, at, and above the fork version.

The integration property test should assert the route is not a placeholder by
checking these counters, not only that the function returns `Ok`.

## Sensitivity Probes

Before closing L6B, run temporary local mutations and verify test failures:

1. change branch validation to always return success;
2. change row rewrite to use target branch but drop value bytes;
3. change tombstone rewrite into a put row;
4. remove source-branch preflight from row rewrite;
5. change version comparison from `<=` to `<`;
6. change timestamp comparison from `<=` to `<`;
7. remove inherited fork-version cap for latest;
8. remove inherited fork-version cap for timestamp reads;
9. make candidate facts exclude tombstones;
10. make candidate facts exclude expired-looking put rows;
11. add `VersionedValue` to production branch code;
12. add a bare `read_object(...)` call to production branch code.

Record the probe outcomes in the L6B porting-log entry.

## Verification Commands

Mandatory L6B commands:

```bash
cargo test -p strata-storage-next --locked --lib branch
cargo test -p strata-storage-next --locked --test branch_lsm_source_guard
cargo test -p strata-storage-next --features testkit --locked --test branch_lsm_properties
cargo test -p strata-storage-next --no-default-features --features testkit --locked --test branch_lsm_properties
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo fmt --package strata-storage-next --check
git diff --check
```

Run this when L6B touches feature-gated testkit exports:

```bash
cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked
```

Run this before merge if L6B changes shared row/table helpers:

```bash
cargo test -p strata-storage-next --locked
```

## Exit Criteria

L6B test coverage is complete when:

1. direct tests cover branch matching, mismatch rejection, rewrite, effective
   bounds, and candidate facts;
2. generated tests exercise both matching and mismatching rows;
3. generated tests exercise own and inherited bound construction;
4. generated tests exercise combined timestamp plus fork-version caps;
5. generated tests exercise storage-owned and empty-key rows;
6. generated tests exercise encoded-key grouping after inherited row rewrite;
7. generated tests exercise row-chain filtering without collapse;
8. generated tests exercise fork-boundary caps below, at, and above fork;
9. source guards prove L6B remains storage-owned and backend-free;
10. sensitivity probes have been run and recorded;
11. all mandatory commands pass;
12. no test relies on product branch names, product DTOs, wall-clock time,
   backend IO, or lifecycle scheduling.
