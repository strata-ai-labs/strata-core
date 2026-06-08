use crate::format::{decode_immutable_table, TableCompression};
use crate::row::{PhysicalKey, StorageRow, StorageSpaceId};
use crate::table::{
    sort_table_rows_by_key, validate_strictly_sorted_unique_rows, BuiltTableArtifact,
    ImmutableTableReader, MutableTable, TableBuilderConfig, TableCompactionConfig,
    TableCompactionDecision, TableCompactionDropReason, TableCompactionDropSummary,
    TableCompactionInput, TableCompactionOutput, TableCompactionPolicy, TableCompactionReport,
    TableCompactionRowContext, TableCompactionSource, TableCompactionSourceId, TableCompactor,
    TableCursor, TableIdentity, TableInternalKeyBytes, TableReaderConfig, TableRow,
    TableRuntimeError,
};
use std::cell::Cell;
use std::rc::Rc;
use strata_core_next::{BranchId, CommitVersion, Timestamp};

fn keep_all_policy() -> impl TableCompactionPolicy {
    |_: &TableCompactionRowContext<'_>, _: &TableRow| Ok(TableCompactionDecision::Keep)
}

fn branch(byte: u8) -> BranchId {
    BranchId::from_bytes([byte; BranchId::BYTE_LEN])
}

fn physical_key(branch_byte: u8, space_id: u8, user_key: impl Into<Vec<u8>>) -> PhysicalKey {
    PhysicalKey::new(
        branch(branch_byte),
        "compaction",
        StorageSpaceId::from_raw(space_id).expect("storage space id"),
        user_key,
    )
    .expect("physical key")
}

fn put_row(user_key: impl Into<Vec<u8>>, version: u64) -> StorageRow {
    put_row_for_key(
        physical_key(1, 0x20, user_key),
        version,
        version.to_le_bytes().to_vec(),
    )
}

fn put_row_for_key(key: PhysicalKey, version: u64, value: Vec<u8>) -> StorageRow {
    StorageRow::put(
        key,
        CommitVersion::new(version),
        Timestamp::from_micros(version.saturating_add(100)),
        Timestamp::EPOCH,
        value,
    )
}

fn expired_row(user_key: impl Into<Vec<u8>>, version: u64) -> StorageRow {
    StorageRow::put(
        physical_key(1, 0x20, user_key),
        CommitVersion::new(version),
        Timestamp::from_micros(version.saturating_add(100)),
        Timestamp::from_micros(1),
        b"expired".to_vec(),
    )
}

fn tombstone_row(user_key: impl Into<Vec<u8>>, version: u64) -> StorageRow {
    StorageRow::tombstone(
        physical_key(1, 0x20, user_key),
        CommitVersion::new(version),
        Timestamp::from_micros(version.saturating_add(100)),
    )
}

fn sorted_table_rows(rows: &[StorageRow]) -> Vec<TableRow> {
    let mut rows = rows.iter().cloned().map(TableRow::new).collect::<Vec<_>>();
    sort_table_rows_by_key(&mut rows);
    rows
}

fn source(name: &str, rows: &[StorageRow]) -> TableCompactionSource {
    TableCompactionSource::from_rows(source_id(name), sorted_table_rows(rows)).expect("source")
}

fn source_id(name: &str) -> TableCompactionSourceId {
    TableCompactionSourceId::new(name).expect("source id")
}

fn identity(name: &'static str) -> TableIdentity {
    TableIdentity::new(name).expect("identity")
}

fn compactor(target_output_bytes: u64, max_output_tables: usize) -> TableCompactor {
    TableCompactor::new(
        TableCompactionConfig::new(target_output_bytes, max_output_tables).expect("config"),
        TableBuilderConfig::new(512, 2, TableCompression::Uncompressed).expect("builder config"),
    )
    .expect("compactor")
}

fn zstd_compactor(target_output_bytes: u64, max_output_tables: usize) -> TableCompactor {
    TableCompactor::new(
        TableCompactionConfig::new(target_output_bytes, max_output_tables).expect("config"),
        TableBuilderConfig::new(256, 1, TableCompression::Zstd).expect("builder config"),
    )
    .expect("compactor")
}

fn output_storage_rows(output: &TableCompactionOutput) -> Vec<StorageRow> {
    output
        .artifacts()
        .iter()
        .flat_map(|artifact| {
            ImmutableTableReader::open_bytes(
                artifact.facts().identity().clone(),
                artifact.bytes().to_vec(),
                crate::table::TableReaderConfig::default(),
            )
            .expect("open output artifact")
            .rows()
            .iter()
            .map(|row| row.row().clone())
            .collect::<Vec<_>>()
        })
        .collect()
}

fn sorted_storage_rows(rows: &[StorageRow]) -> Vec<StorageRow> {
    sorted_table_rows(rows)
        .into_iter()
        .map(TableRow::into_row)
        .collect()
}

fn output_artifact_bytes(output: &TableCompactionOutput) -> Vec<Vec<u8>> {
    output
        .artifacts()
        .iter()
        .map(|artifact| artifact.bytes().to_vec())
        .collect()
}

fn first_data_frame_compression_byte(bytes: &[u8]) -> u8 {
    const TABLE_HEADER_SIZE: usize = 64;
    const FRAME_KIND_BYTES: usize = 1;
    bytes[TABLE_HEADER_SIZE + FRAME_KIND_BYTES]
}

const ZSTD_COMPRESSION_TAG: u8 = 1;

fn assert_artifact_facts_match_rows(output: &TableCompactionOutput, seed: &str) {
    let byte_sum = output
        .artifacts()
        .iter()
        .map(BuiltTableArtifact::byte_count)
        .sum::<u64>();
    assert_eq!(output.report().output_bytes(), byte_sum);
    assert_eq!(output.report().output_tables(), output.artifacts().len());
    assert_eq!(
        output.report().split_count(),
        output.artifacts().len().saturating_sub(1) as u64
    );

    for (index, artifact) in output.artifacts().iter().enumerate() {
        let identity = artifact.facts().identity().as_str();
        let prefix = format!("{seed}-");
        let suffix = format!("-{index:08x}");
        assert!(identity.starts_with(&prefix));
        assert!(identity.ends_with(&suffix));
        let fingerprint = &identity[prefix.len()..identity.len() - suffix.len()];
        assert_eq!(fingerprint.len(), 16);
        assert!(fingerprint.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert_eq!(artifact.byte_count(), artifact.bytes().len() as u64);
        assert_eq!(&artifact.bytes()[..4], b"STTB");
        assert_ne!(&artifact.bytes()[..6], b"STRAKV");

        let decoded = decode_immutable_table(artifact.bytes()).expect("decode artifact");
        assert!(!decoded.rows().is_empty());
        let reader = ImmutableTableReader::open_bytes(
            artifact.facts().identity().clone(),
            artifact.bytes().to_vec(),
            TableReaderConfig::default(),
        )
        .expect("open artifact reader");
        assert_eq!(
            reader
                .rows()
                .iter()
                .map(|row| row.row().clone())
                .collect::<Vec<_>>(),
            decoded.rows()
        );

        let rows = decoded
            .rows()
            .iter()
            .cloned()
            .map(TableRow::new)
            .collect::<Vec<_>>();
        validate_strictly_sorted_unique_rows(&rows).expect("artifact rows are sorted unique");
        assert_eq!(artifact.facts().row_count(), rows.len() as u64);
        assert_eq!(
            artifact.facts().data_block_count(),
            decoded.header().data_block_count()
        );
        assert_eq!(
            artifact.facts().key_range().first_key(),
            rows.first().expect("first row").encoded_key()
        );
        assert_eq!(
            artifact.facts().key_range().last_key(),
            rows.last().expect("last row").encoded_key()
        );
        let commit_min = decoded
            .rows()
            .iter()
            .map(StorageRow::commit_version)
            .min()
            .expect("commit min");
        let commit_max = decoded
            .rows()
            .iter()
            .map(StorageRow::commit_version)
            .max()
            .expect("commit max");
        assert_eq!(artifact.facts().commit_range().min(), commit_min);
        assert_eq!(artifact.facts().commit_range().max(), commit_max);
    }
}

fn assert_output_key_ranges_are_non_overlapping(output: &TableCompactionOutput) {
    let mut previous_last = None::<Vec<u8>>;
    for artifact in output.artifacts() {
        let first = artifact.facts().key_range().first_key();
        let last = artifact.facts().key_range().last_key();
        assert!(first <= last);
        if let Some(previous) = previous_last {
            assert!(
                previous.as_slice() < first,
                "output table key ranges overlapped or were not sorted"
            );
        }
        previous_last = Some(last.to_vec());
    }
}

fn drop_summary_rows(output: &TableCompactionOutput, reason: TableCompactionDropReason) -> u64 {
    output
        .report()
        .drop_summaries()
        .iter()
        .find(|summary| summary.reason() == reason)
        .map_or(0, |summary| summary.rows())
}

#[test]
fn compactor_validates_config_and_treats_empty_inputs_as_no_output() {
    assert_eq!(
        TableCompactionConfig::new(0, 1),
        Err(TableRuntimeError::InvalidConfig {
            field: "target_output_bytes",
            reason: "must be nonzero",
        })
    );
    assert_eq!(
        TableCompactionConfig::new(1, 0),
        Err(TableRuntimeError::InvalidConfig {
            field: "max_output_tables",
            reason: "must be nonzero",
        })
    );
    assert_eq!(
        TableCompactionSourceId::new(""),
        Err(TableRuntimeError::InvalidConfig {
            field: "compaction_source_id",
            reason: "must not be empty",
        })
    );
    assert_eq!(
        TableCompactionSourceId::new("x".repeat(129)),
        Err(TableRuntimeError::InvalidConfig {
            field: "compaction_source_id",
            reason: "is too large",
        })
    );
    assert_eq!(
        TableCompactionSourceId::new("bad\0id"),
        Err(TableRuntimeError::InvalidConfig {
            field: "compaction_source_id",
            reason: "must not contain nul bytes",
        })
    );

    let mut policy = keep_all_policy();
    assert_eq!(compactor(1024, 2).config().target_output_bytes(), 1024);
    assert_eq!(compactor(1024, 2).builder_config().rows_per_block(), 2);
    let output = compactor(1024, 2)
        .compact(&identity("empty-compact"), &[], &mut policy)
        .expect("empty compaction is a no-op");
    let (artifacts, report): (Vec<_>, TableCompactionReport) = output.into_parts();
    assert!(artifacts.is_empty());
    assert_eq!(report.input_sources(), 0);
    assert_eq!(report.input_rows(), 0);
    assert_eq!(report.kept_rows(), 0);
    assert_eq!(report.dropped_rows(), 0);
    assert_eq!(report.output_tables(), 0);
    assert_eq!(report.output_bytes(), 0);
    assert_eq!(report.split_count(), 0);

    let mut calls = 0usize;
    let mut policy = |_: &crate::table::TableCompactionRowContext<'_>, _: &TableRow| {
        calls += 1;
        Ok(TableCompactionDecision::Keep)
    };
    let empty_source =
        TableCompactionSource::from_rows(source_id("empty-source"), Vec::new()).expect("source");
    let output = compactor(1024, 2)
        .compact(&identity("one-empty-source"), &[empty_source], &mut policy)
        .expect("empty source compaction is a no-op");
    assert!(output.artifacts().is_empty());
    assert_eq!(output.report().input_sources(), 1);
    assert_eq!(output.report().input_rows(), 0);
    assert_eq!(output.report().kept_rows(), 0);
    assert_eq!(calls, 0);
}

#[test]
fn keep_all_policy_has_no_hidden_retention_rules() {
    let shared_key = physical_key(1, 0x20, b"shared-version".to_vec());
    let same_user_key = b"same-user-key".to_vec();
    let rows = vec![
        put_row_for_key(shared_key.clone(), 99, b"newest".to_vec()),
        put_row_for_key(shared_key.clone(), 10, b"older".to_vec()),
        tombstone_row(b"above-floor-looking-delete".to_vec(), 50),
        expired_row(b"expired-looking-row".to_vec(), 40),
        put_row_for_key(
            physical_key(1, 0x20, same_user_key.clone()),
            30,
            b"space-20".to_vec(),
        ),
        put_row_for_key(
            physical_key(1, 0x21, same_user_key),
            30,
            b"space-21".to_vec(),
        ),
        put_row_for_key(
            physical_key(2, 0x20, [0x65, 0x76, 0x65, 0x6e, 0x74]),
            20,
            b"event-shaped-payload".to_vec(),
        ),
    ];
    let expected = sorted_storage_rows(&rows);

    let mut policy = keep_all_policy();
    let output = compactor(64 * 1024, 4)
        .compact(
            &identity("no-hidden-retention"),
            &[source("all-row-kinds", &rows)],
            &mut policy,
        )
        .expect("keep-all compaction");

    assert_eq!(output.report().input_rows(), rows.len() as u64);
    assert_eq!(output.report().kept_rows(), rows.len() as u64);
    assert_eq!(output.report().dropped_rows(), 0);
    assert_eq!(output_storage_rows(&output), expected);
    assert!(output_storage_rows(&output)
        .iter()
        .any(StorageRow::is_tombstone));
    assert!(output_storage_rows(&output)
        .iter()
        .any(|row| row.expires_at() != Timestamp::EPOCH));
}

#[test]
fn keep_all_policy_merges_sources_and_preserves_row_facts() {
    let rows = vec![
        put_row(b"bravo".to_vec(), 2),
        tombstone_row(b"charlie".to_vec(), 3),
        expired_row(b"delta".to_vec(), 4),
        put_row_for_key(
            physical_key(2, 0x21, [0x00, 0xff, 0x01]),
            5,
            vec![0, 1, 2, 3],
        ),
    ];
    let first = source("first", &rows[..2]);
    let second = source("second", &rows[2..]);
    let expected = sorted_table_rows(&rows)
        .into_iter()
        .map(TableRow::into_row)
        .collect::<Vec<_>>();

    let mut policy = keep_all_policy();
    let output = compactor(16 * 1024, 8)
        .compact(&identity("keep-all"), &[first, second], &mut policy)
        .expect("keep all compaction");

    assert_eq!(output.report().input_sources(), 2);
    assert_eq!(output.report().input_rows(), 4);
    assert_eq!(output.report().kept_rows(), 4);
    assert_eq!(output.report().dropped_rows(), 0);
    assert_eq!(output.report().output_tables(), 1);
    assert_eq!(
        output.report().output_bytes(),
        output.artifacts()[0].byte_count()
    );
    assert_eq!(output.report().drop_summaries(), &[]);
    assert_eq!(output_storage_rows(&output), expected);
    assert!(output_storage_rows(&output)
        .iter()
        .any(StorageRow::is_tombstone));
    assert!(output_storage_rows(&output)
        .iter()
        .any(|row| row.expires_at() == Timestamp::from_micros(1)));
}

#[test]
fn policy_context_reports_merged_source_and_previous_kept_rows() {
    let rows = [
        put_row(b"alpha".to_vec(), 1),
        put_row(b"bravo".to_vec(), 2),
        put_row(b"charlie".to_vec(), 3),
        put_row(b"delta".to_vec(), 4),
    ];
    let left = source("left", &[rows[0].clone(), rows[2].clone()]);
    let right = source("right", &[rows[1].clone(), rows[3].clone()]);
    let sorted_rows = sorted_table_rows(&rows);
    let mut observed = Vec::<(String, usize, usize, u64, Option<Vec<u8>>)>::new();
    let mut policy = |context: &TableCompactionRowContext<'_>, _: &TableRow| {
        observed.push((
            context.source_id().as_str().to_owned(),
            context.source_index(),
            context.source_row_index(),
            context.merged_row_index(),
            context
                .previous_kept_key()
                .map(|key| key.as_slice().to_vec()),
        ));
        Ok(TableCompactionDecision::Keep)
    };

    let output = compactor(16 * 1024, 4)
        .compact(&identity("context-output"), &[left, right], &mut policy)
        .expect("context compaction");

    assert_eq!(output_storage_rows(&output), sorted_storage_rows(&rows));
    assert_eq!(
        observed,
        vec![
            ("left".to_owned(), 0, 0, 0, None),
            (
                "right".to_owned(),
                1,
                0,
                1,
                Some(sorted_rows[0].encoded_key().to_vec())
            ),
            (
                "left".to_owned(),
                0,
                1,
                2,
                Some(sorted_rows[1].encoded_key().to_vec())
            ),
            (
                "right".to_owned(),
                1,
                1,
                3,
                Some(sorted_rows[2].encoded_key().to_vec())
            ),
        ]
    );
}

#[test]
fn compaction_policy_drops_exactly_selected_rows_and_reports_reasons() {
    let rows = [
        put_row(b"alpha".to_vec(), 1),
        tombstone_row(b"bravo".to_vec(), 2),
        expired_row(b"charlie".to_vec(), 3),
        put_row(b"delta".to_vec(), 4),
        put_row(b"echo".to_vec(), 5),
    ];
    let mut policy = |context: &crate::table::TableCompactionRowContext<'_>, row: &TableRow| {
        assert_eq!(context.source_id().as_str(), "policy-source");
        assert_eq!(context.source_index(), 0);
        assert_eq!(
            context.source_row_index(),
            usize::try_from(context.merged_row_index()).unwrap()
        );
        if row.commit_version() == CommitVersion::new(1) {
            assert!(context.previous_kept_key().is_none());
            Ok(TableCompactionDecision::Keep)
        } else if row.is_tombstone() {
            assert!(context.previous_kept_key().is_some());
            Ok(TableCompactionDecision::drop(
                TableCompactionDropReason::TombstoneElided,
            ))
        } else if row.expires_at() == Timestamp::from_micros(1) {
            Ok(TableCompactionDecision::drop(
                TableCompactionDropReason::Expired,
            ))
        } else if row.commit_version() == CommitVersion::new(4) {
            Ok(TableCompactionDecision::drop(
                TableCompactionDropReason::OlderVersion,
            ))
        } else if row.commit_version() == CommitVersion::new(5) {
            Ok(TableCompactionDecision::drop(
                TableCompactionDropReason::CallerSelected,
            ))
        } else {
            Ok(TableCompactionDecision::Keep)
        }
    };
    let policy_object: &mut dyn TableCompactionPolicy = &mut policy;

    let output = compactor(16 * 1024, 4)
        .compact(
            &identity("policy-output"),
            &[source("policy-source", &rows)],
            policy_object,
        )
        .expect("policy compaction");
    let kept = output_storage_rows(&output);

    assert_eq!(kept.len(), 1);
    assert!(kept.iter().all(|row| !row.is_tombstone()));
    assert!(kept
        .iter()
        .all(|row| row.expires_at() != Timestamp::from_micros(1)));
    assert_eq!(output.report().input_rows(), 5);
    assert_eq!(output.report().kept_rows(), 1);
    assert_eq!(output.report().dropped_rows(), 4);
    let summaries: &[TableCompactionDropSummary] = output.report().drop_summaries();
    assert_eq!(summaries.len(), 4);
    assert!(summaries.iter().any(|summary| {
        summary.reason() == TableCompactionDropReason::TombstoneElided && summary.rows() == 1
    }));
    assert!(summaries.iter().any(|summary| {
        summary.reason() == TableCompactionDropReason::Expired && summary.rows() == 1
    }));
    assert!(summaries.iter().any(|summary| {
        summary.reason() == TableCompactionDropReason::OlderVersion && summary.rows() == 1
    }));
    assert!(summaries.iter().any(|summary| {
        summary.reason() == TableCompactionDropReason::CallerSelected && summary.rows() == 1
    }));
}

#[test]
fn compaction_policy_can_drop_older_physical_key_versions_explicitly() {
    let shared_key = physical_key(1, 0x20, b"versioned".to_vec());
    let rows = [
        put_row_for_key(shared_key.clone(), 10, b"newer".to_vec()),
        put_row_for_key(shared_key, 2, b"older".to_vec()),
        put_row(b"zulu".to_vec(), 1),
    ];
    let mut policy = |context: &TableCompactionRowContext<'_>, row: &TableRow| {
        let Some(previous_key) = context.previous_kept_key() else {
            return Ok(TableCompactionDecision::Keep);
        };
        if previous_key.physical_key()? == row.physical_key().clone() {
            Ok(TableCompactionDecision::drop(
                TableCompactionDropReason::OlderVersion,
            ))
        } else {
            Ok(TableCompactionDecision::Keep)
        }
    };

    let output = compactor(16 * 1024, 4)
        .compact(
            &identity("older-version-policy"),
            &[source("versioned-source", &rows)],
            &mut policy,
        )
        .expect("older-version compaction");
    let kept = output_storage_rows(&output);

    assert_eq!(kept.len(), 2);
    assert!(kept
        .iter()
        .any(|row| row.value() == b"newer" && row.commit_version() == CommitVersion::new(10)));
    assert!(!kept
        .iter()
        .any(|row| row.value() == b"older" && row.commit_version() == CommitVersion::new(2)));
    assert!(output.report().drop_summaries().iter().any(|summary| {
        summary.reason() == TableCompactionDropReason::OlderVersion && summary.rows() == 1
    }));
}

#[test]
fn keep_all_policy_preserves_only_tombstone_and_expired_fixtures() {
    let rows = [
        tombstone_row(b"deleted".to_vec(), 7),
        expired_row(b"expired".to_vec(), 6),
    ];
    let mut policy = keep_all_policy();
    let output = compactor(16 * 1024, 2)
        .compact(
            &identity("keep-sensitive-fixtures"),
            &[source("sensitive-fixtures", &rows)],
            &mut policy,
        )
        .expect("keep sensitive fixtures");
    let kept = output_storage_rows(&output);

    assert_eq!(kept, sorted_storage_rows(&rows));
    assert!(kept.iter().any(StorageRow::is_tombstone));
    assert!(kept
        .iter()
        .any(|row| row.expires_at() == Timestamp::from_micros(1)));
}

#[test]
fn policy_error_before_first_keep_returns_no_output() {
    let rows = [put_row(b"alpha".to_vec(), 1), put_row(b"bravo".to_vec(), 2)];
    let mut calls = 0usize;
    let mut fail_immediately = |_: &TableCompactionRowContext<'_>, _: &TableRow| {
        calls += 1;
        Err(TableRuntimeError::CompactionPolicy {
            reason: "first row rejected",
        })
    };

    let error = compactor(16 * 1024, 4)
        .compact(
            &identity("policy-error-before-output"),
            &[source("policy-error", &rows)],
            &mut fail_immediately,
        )
        .expect_err("policy error aborts before output");

    assert_eq!(
        error,
        TableRuntimeError::CompactionPolicy {
            reason: "first row rejected",
        }
    );
    assert_eq!(calls, 1);
}

#[test]
fn all_drop_and_policy_error_paths_do_not_return_partial_outputs() {
    let rows = [
        put_row(b"alpha".to_vec(), 1),
        tombstone_row(b"bravo".to_vec(), 2),
        expired_row(b"charlie".to_vec(), 3),
    ];
    let mut all_drop = |_: &crate::table::TableCompactionRowContext<'_>, row: &TableRow| {
        Ok(TableCompactionDecision::drop(if row.is_tombstone() {
            TableCompactionDropReason::TombstoneElided
        } else {
            TableCompactionDropReason::CallerSelected
        }))
    };

    let output = compactor(16 * 1024, 4)
        .compact(
            &identity("all-drop-output"),
            &[source("all-drop", &rows)],
            &mut all_drop,
        )
        .expect("all drop compaction");
    assert!(output.artifacts().is_empty());
    assert_eq!(output.report().input_sources(), 1);
    assert_eq!(output.report().input_rows(), 3);
    assert_eq!(output.report().kept_rows(), 0);
    assert_eq!(output.report().dropped_rows(), 3);
    assert_eq!(output.report().output_tables(), 0);
    assert_eq!(output.report().output_bytes(), 0);
    assert_eq!(output.report().split_count(), 0);
    assert_eq!(
        output
            .report()
            .drop_summaries()
            .iter()
            .map(|summary| summary.rows())
            .sum::<u64>(),
        3
    );

    let mut calls = 0usize;
    let mut fail_after_first_keep = |_: &crate::table::TableCompactionRowContext<'_>,
                                     _: &TableRow| {
        calls += 1;
        if calls == 1 {
            Ok(TableCompactionDecision::Keep)
        } else {
            Err(TableRuntimeError::CompactionPolicy {
                reason: "injected policy failure",
            })
        }
    };
    let error = compactor(16 * 1024, 4)
        .compact(
            &identity("policy-error-output"),
            &[source("policy-error", &rows)],
            &mut fail_after_first_keep,
        )
        .expect_err("policy error aborts compaction");
    assert_eq!(
        error,
        TableRuntimeError::CompactionPolicy {
            reason: "injected policy failure",
        }
    );
    assert_eq!(calls, 2);
}

#[test]
fn source_validation_and_global_duplicate_rejection_are_typed() {
    let rows = sorted_table_rows(&[put_row(b"alpha".to_vec(), 1), put_row(b"bravo".to_vec(), 2)]);
    let unsorted = vec![rows[1].clone(), rows[0].clone()];
    assert!(matches!(
        TableCompactionSource::from_rows(source_id("unsorted"), unsorted),
        Err(TableRuntimeError::InvalidRowOrder { .. })
    ));
    assert!(matches!(
        TableCompactionSource::from_rows(
            source_id("duplicate-local"),
            vec![rows[0].clone(), rows[0].clone()]
        ),
        Err(TableRuntimeError::DuplicateInternalKey { .. })
    ));

    let mut policy = keep_all_policy();
    let duplicate = put_row(b"same".to_vec(), 7);
    let duplicate_again = duplicate.clone();
    let err = compactor(16 * 1024, 4)
        .compact(
            &identity("duplicate-global"),
            &[
                source("left", std::slice::from_ref(&duplicate)),
                source("right", &[duplicate_again]),
            ],
            &mut policy,
        )
        .expect_err("global duplicate rejected");
    assert!(matches!(
        err,
        TableRuntimeError::DuplicateInternalKey { .. }
    ));
}

#[test]
fn global_duplicate_rejection_runs_before_policy() {
    let duplicate = put_row(b"same".to_vec(), 7);
    let rows = [put_row(b"alpha".to_vec(), 1), duplicate.clone()];
    let mut calls = 0_u64;
    let mut fail_on_call = |_: &TableCompactionRowContext<'_>, _: &TableRow| {
        calls = calls.saturating_add(1);
        Err(TableRuntimeError::CompactionPolicy {
            reason: "policy must not run before duplicate validation",
        })
    };

    let err = compactor(16 * 1024, 4)
        .compact(
            &identity("duplicate-before-policy"),
            &[
                source("left", &rows),
                source("right", std::slice::from_ref(&duplicate)),
            ],
            &mut fail_on_call,
        )
        .expect_err("global duplicate rejected before policy");

    assert!(matches!(
        err,
        TableRuntimeError::DuplicateInternalKey { .. }
    ));
    assert_eq!(calls, 0);
}

#[test]
fn exact_max_output_table_count_is_accepted() {
    let rows = [
        put_row(b"alpha".to_vec(), 1),
        put_row(b"bravo".to_vec(), 2),
        put_row(b"charlie".to_vec(), 3),
    ];
    let mut policy = keep_all_policy();

    let output = compactor(1, 3)
        .compact(
            &identity("exact-output-limit"),
            &[source("exact-limit", &rows)],
            &mut policy,
        )
        .expect("exact output table limit is accepted");

    assert_eq!(output.report().output_tables(), 3);
    assert_eq!(output.report().split_count(), 2);
    assert_artifact_facts_match_rows(&output, "exact-output-limit");
    assert_eq!(output_storage_rows(&output), sorted_storage_rows(&rows));
}

#[test]
fn output_splitting_respects_limits_and_keeps_physical_key_groups_together() {
    let same_key = physical_key(1, 0x20, b"group".to_vec());
    let rows = [
        put_row_for_key(same_key.clone(), 3, vec![1; 256]),
        put_row_for_key(same_key, 5, vec![2; 256]),
    ];
    let mut policy = keep_all_policy();
    let output = compactor(1, 4)
        .compact(
            &identity("grouped-output"),
            &[source("grouped", &rows)],
            &mut policy,
        )
        .expect("same physical key group can exceed target");

    assert_eq!(output.report().output_tables(), 1);
    assert_eq!(output.report().split_count(), 0);
    assert_eq!(output.report().kept_rows(), 2);

    let different_keys = [
        put_row(b"alpha".to_vec(), 1),
        put_row(b"bravo".to_vec(), 2),
        put_row(b"charlie".to_vec(), 3),
    ];
    let output = compactor(1, 8)
        .compact(
            &identity("split-output"),
            &[source("split", &different_keys)],
            &mut policy,
        )
        .expect("split output");
    assert!(output.report().output_tables() > 1);
    assert_eq!(
        output.report().split_count(),
        output.report().output_tables().saturating_sub(1) as u64
    );
    let mut identities = output
        .artifacts()
        .iter()
        .map(|artifact| artifact.facts().identity().as_str().to_owned())
        .collect::<Vec<_>>();
    identities.sort();
    identities.dedup();
    assert_eq!(identities.len(), output.artifacts().len());
    assert_eq!(
        output_storage_rows(&output),
        sorted_table_rows(&different_keys)
            .into_iter()
            .map(TableRow::into_row)
            .collect::<Vec<_>>()
    );

    let err = compactor(1, 1)
        .compact(
            &identity("split-limit"),
            &[source("split-limit", &different_keys)],
            &mut policy,
        )
        .expect_err("max output table limit");
    assert_eq!(
        err,
        TableRuntimeError::InvalidRange {
            field: "max_output_tables",
        }
    );
}

#[test]
fn dropped_rows_do_not_drive_output_splitting() {
    let rows = [
        put_row(b"alpha".to_vec(), 1),
        put_row_for_key(
            physical_key(1, 0x20, b"large-dropped".to_vec()),
            2,
            vec![9; 16 * 1024],
        ),
        put_row(b"omega".to_vec(), 3),
    ];
    let mut policy = |_: &TableCompactionRowContext<'_>, row: &TableRow| {
        if row.value().len() > 1024 {
            Ok(TableCompactionDecision::drop(
                TableCompactionDropReason::CallerSelected,
            ))
        } else {
            Ok(TableCompactionDecision::Keep)
        }
    };

    let output = compactor(4 * 1024, 4)
        .compact(
            &identity("drop-before-split"),
            &[source("drop-large", &rows)],
            &mut policy,
        )
        .expect("drop large row");

    assert_eq!(output.report().input_rows(), 3);
    assert_eq!(output.report().kept_rows(), 2);
    assert_eq!(output.report().dropped_rows(), 1);
    assert_eq!(output.report().output_tables(), 1);
    assert_eq!(
        output_storage_rows(&output),
        sorted_storage_rows(&[rows[0].clone(), rows[2].clone()])
    );
}

#[test]
fn compaction_outputs_valid_zstd_table_artifacts() {
    let rows = [
        put_row(b"alpha".to_vec(), 1),
        put_row(b"bravo".to_vec(), 2),
        put_row(b"charlie".to_vec(), 3),
    ];
    let mut policy = keep_all_policy();

    let output = zstd_compactor(64 * 1024, 2)
        .compact(
            &identity("zstd-output"),
            &[source("zstd-source", &rows)],
            &mut policy,
        )
        .expect("zstd compaction");

    assert_artifact_facts_match_rows(&output, "zstd-output");
    assert_eq!(output.artifacts().len(), 1);
    let artifact = &output.artifacts()[0];
    let decoded = decode_immutable_table(artifact.bytes()).expect("decode zstd artifact");
    assert_eq!(decoded.header().target_data_block_size(), 256);
    assert_eq!(
        decoded.header().data_block_count(),
        u32::try_from(rows.len()).expect("bounded row count")
    );
    assert_eq!(
        first_data_frame_compression_byte(artifact.bytes()),
        ZSTD_COMPRESSION_TAG
    );
    assert_eq!(output_storage_rows(&output), sorted_storage_rows(&rows));
}

#[test]
fn compaction_output_is_deterministic_across_runs_and_source_groupings() {
    let rows = [
        put_row(b"alpha".to_vec(), 1),
        put_row(b"bravo".to_vec(), 2),
        put_row(b"charlie".to_vec(), 3),
        put_row(b"delta".to_vec(), 4),
    ];
    let first_grouping = [
        source("left", &[rows[0].clone(), rows[2].clone()]),
        source("right", &[rows[1].clone(), rows[3].clone()]),
    ];
    let second_grouping = [
        source("first", &[rows[0].clone()]),
        source("middle", &[rows[1].clone(), rows[2].clone()]),
        source("last", &[rows[3].clone()]),
    ];

    let mut first_policy = keep_all_policy();
    let first = compactor(1, 8)
        .compact(
            &identity("deterministic-output"),
            &first_grouping,
            &mut first_policy,
        )
        .expect("first compaction");
    let mut repeat_policy = keep_all_policy();
    let repeat = compactor(1, 8)
        .compact(
            &identity("deterministic-output"),
            &first_grouping,
            &mut repeat_policy,
        )
        .expect("repeat compaction");
    let mut regrouped_policy = keep_all_policy();
    let regrouped = compactor(1, 8)
        .compact(
            &identity("deterministic-output"),
            &second_grouping,
            &mut regrouped_policy,
        )
        .expect("regrouped compaction");

    assert_eq!(
        output_artifact_bytes(&first),
        output_artifact_bytes(&repeat)
    );
    assert_eq!(
        output_artifact_bytes(&first),
        output_artifact_bytes(&regrouped)
    );
    assert_eq!(output_storage_rows(&first), sorted_storage_rows(&rows));
    assert_artifact_facts_match_rows(&first, "deterministic-output");

    let changed_rows = [
        put_row(b"alpha".to_vec(), 1),
        put_row(b"bravo".to_vec(), 2),
        put_row(b"echo".to_vec(), 5),
    ];
    let mut changed_policy = keep_all_policy();
    let changed = compactor(1, 8)
        .compact(
            &identity("deterministic-output"),
            &[source("changed", &changed_rows)],
            &mut changed_policy,
        )
        .expect("changed compaction");
    let first_identities = first
        .artifacts()
        .iter()
        .map(|artifact| artifact.facts().identity().as_str().to_owned())
        .collect::<Vec<_>>();
    let changed_identities = changed
        .artifacts()
        .iter()
        .map(|artifact| artifact.facts().identity().as_str().to_owned())
        .collect::<Vec<_>>();
    assert_ne!(first_identities, changed_identities);
}

#[test]
fn source_can_be_collected_from_raw_cursor() {
    let rows = [
        put_row(b"alpha".to_vec(), 1),
        put_row(b"bravo".to_vec(), 2),
        put_row(b"charlie".to_vec(), 3),
    ];
    let mut table = MutableTable::new();
    for row in rows.clone() {
        table.insert_row(row).expect("insert row");
    }
    let mut cursor = table.cursor();
    let source = TableCompactionSource::from_cursor(source_id("cursor-source"), &mut cursor)
        .expect("source from cursor");
    assert_eq!(source.len(), rows.len());
    assert!(!source.is_empty());
    assert!(cursor.current().is_none());

    let mut policy = keep_all_policy();
    let output = compactor(16 * 1024, 4)
        .compact(&identity("cursor-output"), &[source], &mut policy)
        .expect("compact cursor source");
    assert_eq!(
        output_storage_rows(&output),
        sorted_table_rows(&rows)
            .into_iter()
            .map(TableRow::into_row)
            .collect::<Vec<_>>()
    );
}

#[derive(Clone, Copy)]
enum CursorFault {
    SeekToFirst,
    Advance,
}

struct FaultingCursor {
    rows: Vec<TableRow>,
    position: Option<usize>,
    fault: CursorFault,
    advance_calls: usize,
}

impl FaultingCursor {
    fn new(rows: &[StorageRow], fault: CursorFault) -> Self {
        Self {
            rows: sorted_table_rows(rows),
            position: None,
            fault,
            advance_calls: 0,
        }
    }
}

impl TableCursor for FaultingCursor {
    fn seek_to_first(&mut self) -> Result<(), TableRuntimeError> {
        if matches!(self.fault, CursorFault::SeekToFirst) {
            return Err(TableRuntimeError::source_read("seek failed"));
        }
        self.position = Some(0);
        Ok(())
    }

    fn seek(&mut self, _target: &TableInternalKeyBytes) -> Result<(), TableRuntimeError> {
        self.seek_to_first()
    }

    fn advance(&mut self) -> Result<(), TableRuntimeError> {
        self.advance_calls = self.advance_calls.saturating_add(1);
        if matches!(self.fault, CursorFault::Advance) && self.advance_calls == 1 {
            return Err(TableRuntimeError::source_read("advance failed"));
        }
        self.position = self.position.and_then(|position| {
            let next = position.saturating_add(1);
            (next < self.rows.len()).then_some(next)
        });
        Ok(())
    }

    fn current(&self) -> Option<&TableRow> {
        self.position.and_then(|position| self.rows.get(position))
    }
}

struct TrackedCompactionInput {
    id: TableCompactionSourceId,
    rows: Vec<TableRow>,
    open_calls: Rc<Cell<usize>>,
    advance_calls: Rc<Cell<usize>>,
}

impl TrackedCompactionInput {
    fn new(name: &str, rows: &[StorageRow]) -> Self {
        Self {
            id: source_id(name),
            rows: sorted_table_rows(rows),
            open_calls: Rc::new(Cell::new(0)),
            advance_calls: Rc::new(Cell::new(0)),
        }
    }

    fn from_table_rows(name: &str, rows: Vec<TableRow>) -> Self {
        Self {
            id: source_id(name),
            rows,
            open_calls: Rc::new(Cell::new(0)),
            advance_calls: Rc::new(Cell::new(0)),
        }
    }

    fn open_calls(&self) -> usize {
        self.open_calls.get()
    }

    fn advance_calls(&self) -> usize {
        self.advance_calls.get()
    }
}

fn assert_cursor_input_streaming_is_bounded(input: &TrackedCompactionInput, logical_rows: usize) {
    let open_calls = input.open_calls();
    assert_eq!(open_calls, 1, "cursor input was opened {open_calls} times");
    assert!(
        input.advance_calls() >= logical_rows,
        "cursor input advanced fewer times than its logical rows"
    );
    assert!(
        input.advance_calls() <= logical_rows.saturating_mul(2),
        "cursor input advanced more than once per logical row per validation/build pass"
    );
}

impl TableCompactionInput for TrackedCompactionInput {
    fn id(&self) -> &TableCompactionSourceId {
        &self.id
    }

    fn open_cursor(&self) -> Result<Box<dyn TableCursor + '_>, TableRuntimeError> {
        self.open_calls.set(self.open_calls.get().saturating_add(1));
        Ok(Box::new(TrackedCompactionCursor {
            rows: &self.rows,
            position: None,
            advance_calls: self.advance_calls.clone(),
        }))
    }
}

struct TrackedCompactionCursor<'a> {
    rows: &'a [TableRow],
    position: Option<usize>,
    advance_calls: Rc<Cell<usize>>,
}

impl TableCursor for TrackedCompactionCursor<'_> {
    fn seek_to_first(&mut self) -> Result<(), TableRuntimeError> {
        self.position = (!self.rows.is_empty()).then_some(0);
        Ok(())
    }

    fn seek(&mut self, target: &TableInternalKeyBytes) -> Result<(), TableRuntimeError> {
        let index = match self.rows.binary_search_by(|row| row.key().cmp(target)) {
            Ok(index) | Err(index) => index,
        };
        self.position = (index < self.rows.len()).then_some(index);
        Ok(())
    }

    fn advance(&mut self) -> Result<(), TableRuntimeError> {
        self.advance_calls
            .set(self.advance_calls.get().saturating_add(1));
        self.position = self.position.and_then(|position| {
            let next = position.saturating_add(1);
            (next < self.rows.len()).then_some(next)
        });
        Ok(())
    }

    fn current(&self) -> Option<&TableRow> {
        self.position.and_then(|position| self.rows.get(position))
    }
}

#[test]
fn cursor_source_errors_are_preserved_before_compaction() {
    let rows = [put_row(b"alpha".to_vec(), 1), put_row(b"bravo".to_vec(), 2)];

    let mut seek_failure = FaultingCursor::new(&rows, CursorFault::SeekToFirst);
    assert_eq!(
        TableCompactionSource::from_cursor(source_id("seek-failure"), &mut seek_failure),
        Err(TableRuntimeError::source_read("seek failed"))
    );
    assert!(seek_failure.current().is_none());

    let mut advance_failure = FaultingCursor::new(&rows, CursorFault::Advance);
    assert_eq!(
        TableCompactionSource::from_cursor(source_id("advance-failure"), &mut advance_failure),
        Err(TableRuntimeError::source_read("advance failed"))
    );
    assert_eq!(advance_failure.advance_calls, 1);
}

#[test]
fn compaction_streams_cursor_inputs_without_collecting_sources() {
    let rows = [
        put_row(b"alpha".to_vec(), 1),
        put_row(b"bravo".to_vec(), 2),
        put_row(b"charlie".to_vec(), 3),
        put_row(b"delta".to_vec(), 4),
    ];
    let left = TrackedCompactionInput::new("stream-left", &[rows[0].clone(), rows[2].clone()]);
    let right = TrackedCompactionInput::new("stream-right", &[rows[1].clone(), rows[3].clone()]);
    let inputs: [&dyn TableCompactionInput; 2] = [&left, &right];
    let mut observed = Vec::new();
    let mut policy = |context: &TableCompactionRowContext<'_>, row: &TableRow| {
        observed.push((
            context.source_id().as_str().to_owned(),
            context.source_index(),
            context.source_row_index(),
            row.row().clone(),
        ));
        Ok(TableCompactionDecision::Keep)
    };

    let output = compactor(16 * 1024, 4)
        .compact_inputs(&identity("stream-cursor-input"), &inputs, &mut policy)
        .expect("stream cursor input compaction");

    assert_eq!(output_storage_rows(&output), sorted_storage_rows(&rows));
    assert_cursor_input_streaming_is_bounded(&left, 2);
    assert_cursor_input_streaming_is_bounded(&right, 2);
    assert_eq!(
        observed
            .iter()
            .map(|(_, _, _, row)| row.clone())
            .collect::<Vec<_>>(),
        sorted_storage_rows(&rows)
    );
    assert_eq!(observed[0].0, "stream-left");
    assert_eq!(observed[1].0, "stream-right");
    assert_eq!(observed[2].2, 1);
    assert_eq!(observed[3].2, 1);
}

#[test]
fn streaming_compaction_zero_cursor_sources_produce_no_outputs() {
    let inputs: [&dyn TableCompactionInput; 0] = [];
    let mut calls = 0usize;
    let mut policy = |_: &TableCompactionRowContext<'_>, _: &TableRow| {
        calls = calls.saturating_add(1);
        Ok(TableCompactionDecision::Keep)
    };

    let output = compactor(16 * 1024, 4)
        .compact_inputs(&identity("stream-empty-inputs"), &inputs, &mut policy)
        .expect("empty cursor-input compaction");

    assert!(output.artifacts().is_empty());
    assert_eq!(output.report().input_sources(), 0);
    assert_eq!(output.report().input_rows(), 0);
    assert_eq!(output.report().kept_rows(), 0);
    assert_eq!(output.report().dropped_rows(), 0);
    assert_eq!(output.report().output_tables(), 0);
    assert_eq!(output.report().split_count(), 0);
    assert_eq!(calls, 0);
}

#[test]
fn streaming_compaction_many_disjoint_cursor_sources_merge_in_order() {
    let rows = [
        put_row(b"alpha".to_vec(), 1),
        put_row(b"bravo".to_vec(), 2),
        put_row(b"charlie".to_vec(), 3),
        put_row(b"delta".to_vec(), 4),
        put_row(b"echo".to_vec(), 5),
    ];
    let first = TrackedCompactionInput::new("many-0", std::slice::from_ref(&rows[3]));
    let second = TrackedCompactionInput::new("many-1", std::slice::from_ref(&rows[1]));
    let third = TrackedCompactionInput::new("many-2", std::slice::from_ref(&rows[4]));
    let fourth = TrackedCompactionInput::new("many-3", std::slice::from_ref(&rows[0]));
    let fifth = TrackedCompactionInput::new("many-4", std::slice::from_ref(&rows[2]));
    let inputs: [&dyn TableCompactionInput; 5] = [&first, &second, &third, &fourth, &fifth];
    let mut observed = Vec::new();
    let mut policy = |context: &TableCompactionRowContext<'_>, row: &TableRow| {
        observed.push((
            context.source_index(),
            context.source_row_index(),
            row.row().clone(),
        ));
        Ok(TableCompactionDecision::Keep)
    };

    let output = compactor(16 * 1024, 8)
        .compact_inputs(&identity("stream-many-sources"), &inputs, &mut policy)
        .expect("many cursor-input compaction");

    assert_eq!(output.report().input_sources(), 5);
    assert_eq!(output.report().input_rows(), rows.len() as u64);
    assert_eq!(output.report().kept_rows(), rows.len() as u64);
    assert_eq!(output.report().dropped_rows(), 0);
    assert_eq!(output_storage_rows(&output), sorted_storage_rows(&rows));
    assert_eq!(
        observed
            .iter()
            .map(|(_, _, row)| row.clone())
            .collect::<Vec<_>>(),
        sorted_storage_rows(&rows)
    );
    assert!(observed
        .iter()
        .all(|(_, source_row_index, _)| *source_row_index == 0));
    for input in [&first, &second, &third, &fourth, &fifth] {
        assert_cursor_input_streaming_is_bounded(input, 1);
    }
}

#[test]
fn streaming_compaction_single_cursor_source_copies_rows_when_policy_keeps_all() {
    let rows = [
        put_row(b"alpha".to_vec(), 1),
        tombstone_row(b"bravo".to_vec(), 2),
        expired_row(b"charlie".to_vec(), 3),
        put_row(b"delta".to_vec(), 4),
    ];
    let input = TrackedCompactionInput::new("single-source", &rows);
    let inputs: [&dyn TableCompactionInput; 1] = [&input];
    let mut policy = keep_all_policy();

    let output = compactor(16 * 1024, 4)
        .compact_inputs(&identity("stream-single-source"), &inputs, &mut policy)
        .expect("single cursor-input compaction");
    let kept = output_storage_rows(&output);

    assert_eq!(kept, sorted_storage_rows(&rows));
    assert_eq!(output.report().input_sources(), 1);
    assert_eq!(output.report().input_rows(), rows.len() as u64);
    assert_eq!(output.report().kept_rows(), rows.len() as u64);
    assert_eq!(output.report().dropped_rows(), 0);
    assert_eq!(output.report().drop_summaries(), &[]);
    assert!(kept.iter().any(StorageRow::is_tombstone));
    assert!(kept
        .iter()
        .any(|row| row.expires_at() == Timestamp::from_micros(1)));
    assert_cursor_input_streaming_is_bounded(&input, rows.len());
}

#[test]
fn streaming_compaction_preserves_cross_source_physical_key_versions() {
    let shared_key = physical_key(1, 0x20, b"shared-across-sources".to_vec());
    let newer = put_row_for_key(shared_key.clone(), 30, b"newer".to_vec());
    let older = put_row_for_key(shared_key, 7, b"older".to_vec());
    let neighbor = put_row(b"neighbor".to_vec(), 11);
    let left = TrackedCompactionInput::new("version-left", &[older.clone(), neighbor.clone()]);
    let right = TrackedCompactionInput::new("version-right", std::slice::from_ref(&newer));
    let inputs: [&dyn TableCompactionInput; 2] = [&left, &right];
    let mut policy = keep_all_policy();

    let output = compactor(16 * 1024, 4)
        .compact_inputs(
            &identity("stream-cross-source-versions"),
            &inputs,
            &mut policy,
        )
        .expect("cross-source version compaction");
    let kept = output_storage_rows(&output);

    assert_eq!(
        kept,
        sorted_storage_rows(&[newer.clone(), older.clone(), neighbor])
    );
    assert_eq!(output.report().input_rows(), 3);
    assert_eq!(output.report().kept_rows(), 3);
    assert_eq!(output.report().dropped_rows(), 0);
    assert!(kept.iter().any(|row| row.value() == b"newer"));
    assert!(kept.iter().any(|row| row.value() == b"older"));
}

#[test]
fn streaming_policy_can_drop_older_physical_key_versions() {
    let shared_key = physical_key(1, 0x20, b"versioned-stream".to_vec());
    let newer = put_row_for_key(shared_key.clone(), 30, b"newer".to_vec());
    let older = put_row_for_key(shared_key, 7, b"older".to_vec());
    let neighbor = put_row(b"neighbor".to_vec(), 11);
    let left =
        TrackedCompactionInput::new("older-version-left", &[older.clone(), neighbor.clone()]);
    let right = TrackedCompactionInput::new("older-version-right", std::slice::from_ref(&newer));
    let inputs: [&dyn TableCompactionInput; 2] = [&left, &right];
    let mut policy = |context: &TableCompactionRowContext<'_>, row: &TableRow| {
        let Some(previous_key) = context.previous_kept_key() else {
            return Ok(TableCompactionDecision::Keep);
        };
        if previous_key.physical_key()? == row.physical_key().clone() {
            Ok(TableCompactionDecision::drop(
                TableCompactionDropReason::OlderVersion,
            ))
        } else {
            Ok(TableCompactionDecision::Keep)
        }
    };

    let output = compactor(16 * 1024, 4)
        .compact_inputs(
            &identity("stream-older-version-policy"),
            &inputs,
            &mut policy,
        )
        .expect("older-version cursor-input compaction");
    let kept = output_storage_rows(&output);

    assert_eq!(kept, sorted_storage_rows(&[newer.clone(), neighbor]));
    assert!(kept.iter().any(|row| row.value() == b"newer"));
    assert!(!kept.iter().any(|row| row.value() == b"older"));
    assert_eq!(output.report().input_rows(), 3);
    assert_eq!(output.report().kept_rows(), 2);
    assert_eq!(output.report().dropped_rows(), 1);
    assert_eq!(
        drop_summary_rows(&output, TableCompactionDropReason::OlderVersion),
        1
    );
}

#[test]
fn streaming_policy_controls_tombstone_and_expired_row_drops() {
    let keep_delete = tombstone_row(b"keep-delete".to_vec(), 10);
    let drop_delete = tombstone_row(b"drop-delete".to_vec(), 9);
    let expired = expired_row(b"drop-expired".to_vec(), 8);
    let live = put_row(b"live".to_vec(), 7);
    let left = TrackedCompactionInput::new("policy-left", &[keep_delete.clone(), expired.clone()]);
    let right = TrackedCompactionInput::new("policy-right", &[drop_delete.clone(), live.clone()]);
    let inputs: [&dyn TableCompactionInput; 2] = [&left, &right];
    let mut policy = |_: &TableCompactionRowContext<'_>, row: &TableRow| {
        if row.is_tombstone() && row.row().physical_key().user_key() == b"drop-delete" {
            Ok(TableCompactionDecision::drop(
                TableCompactionDropReason::TombstoneElided,
            ))
        } else if row.expires_at() != Timestamp::EPOCH {
            Ok(TableCompactionDecision::drop(
                TableCompactionDropReason::Expired,
            ))
        } else {
            Ok(TableCompactionDecision::Keep)
        }
    };

    let output = compactor(16 * 1024, 4)
        .compact_inputs(&identity("stream-policy-sensitive"), &inputs, &mut policy)
        .expect("policy-controlled cursor-input compaction");
    let kept = output_storage_rows(&output);

    assert_eq!(
        kept,
        sorted_storage_rows(&[keep_delete.clone(), live.clone()])
    );
    assert!(kept.iter().any(StorageRow::is_tombstone));
    assert!(!kept
        .iter()
        .any(|row| row.physical_key().user_key() == b"drop-delete"));
    assert!(!kept
        .iter()
        .any(|row| row.physical_key().user_key() == b"drop-expired"));
    assert_eq!(output.report().input_rows(), 4);
    assert_eq!(output.report().kept_rows(), 2);
    assert_eq!(output.report().dropped_rows(), 2);
    assert_eq!(
        drop_summary_rows(&output, TableCompactionDropReason::TombstoneElided),
        1
    );
    assert_eq!(
        drop_summary_rows(&output, TableCompactionDropReason::Expired),
        1
    );
}

#[test]
fn streaming_policy_error_aborts_without_output_success() {
    let rows = [
        put_row(b"alpha".to_vec(), 1),
        put_row(b"bravo".to_vec(), 2),
        put_row(b"charlie".to_vec(), 3),
    ];
    let input = TrackedCompactionInput::new("stream-error-source", &rows);
    let inputs: [&dyn TableCompactionInput; 1] = [&input];
    let mut calls = 0usize;
    let mut policy = |_: &TableCompactionRowContext<'_>, _: &TableRow| {
        calls = calls.saturating_add(1);
        if calls == 1 {
            Ok(TableCompactionDecision::Keep)
        } else {
            Err(TableRuntimeError::CompactionPolicy {
                reason: "streaming policy failure",
            })
        }
    };

    let err = compactor(16 * 1024, 4)
        .compact_inputs(&identity("stream-policy-error"), &inputs, &mut policy)
        .expect_err("streaming policy error aborts compaction");

    assert_eq!(
        err,
        TableRuntimeError::CompactionPolicy {
            reason: "streaming policy failure",
        }
    );
    assert_eq!(calls, 2);
    assert_cursor_input_streaming_is_bounded(&input, rows.len());
}

#[test]
fn cursor_input_duplicate_rejection_runs_before_policy() {
    let duplicate = put_row(b"same".to_vec(), 7);
    let left = TrackedCompactionInput::new("dup-left", std::slice::from_ref(&duplicate));
    let right = TrackedCompactionInput::new("dup-right", &[duplicate]);
    let inputs: [&dyn TableCompactionInput; 2] = [&left, &right];
    let mut calls = 0usize;
    let mut policy = |_: &TableCompactionRowContext<'_>, _: &TableRow| {
        calls = calls.saturating_add(1);
        Ok(TableCompactionDecision::Keep)
    };

    let err = compactor(16 * 1024, 4)
        .compact_inputs(&identity("cursor-duplicate"), &inputs, &mut policy)
        .expect_err("duplicate cursor input rejected");

    assert!(matches!(
        err,
        TableRuntimeError::DuplicateInternalKey { .. }
    ));
    assert_eq!(calls, 0);
    assert_eq!(left.open_calls(), 1);
    assert_eq!(right.open_calls(), 1);
}

#[test]
fn cursor_input_invalid_order_is_rejected_before_policy() {
    let rows = sorted_table_rows(&[put_row(b"alpha".to_vec(), 1), put_row(b"bravo".to_vec(), 2)]);
    let input = TrackedCompactionInput::from_table_rows(
        "unsorted-cursor",
        vec![rows[1].clone(), rows[0].clone()],
    );
    let inputs: [&dyn TableCompactionInput; 1] = [&input];
    let mut calls = 0usize;
    let mut policy = |_: &TableCompactionRowContext<'_>, _: &TableRow| {
        calls = calls.saturating_add(1);
        Ok(TableCompactionDecision::Keep)
    };

    let err = compactor(16 * 1024, 4)
        .compact_inputs(&identity("cursor-invalid-order"), &inputs, &mut policy)
        .expect_err("invalid cursor order rejected");

    assert!(matches!(err, TableRuntimeError::InvalidRowOrder { .. }));
    assert_eq!(calls, 0);
    assert_eq!(input.open_calls(), 1);
    assert_eq!(input.advance_calls(), 1);
}

#[test]
fn streaming_compaction_output_ranges_are_sorted_and_non_overlapping() {
    let rows = [
        put_row(b"alpha".to_vec(), 1),
        put_row(b"bravo".to_vec(), 2),
        put_row(b"charlie".to_vec(), 3),
        put_row(b"delta".to_vec(), 4),
        put_row(b"echo".to_vec(), 5),
    ];
    let left = TrackedCompactionInput::new(
        "range-left",
        &[rows[0].clone(), rows[2].clone(), rows[4].clone()],
    );
    let right = TrackedCompactionInput::new("range-right", &[rows[1].clone(), rows[3].clone()]);
    let inputs: [&dyn TableCompactionInput; 2] = [&left, &right];
    let mut policy = keep_all_policy();

    let output = compactor(1, 8)
        .compact_inputs(&identity("stream-output-ranges"), &inputs, &mut policy)
        .expect("streaming split compaction");

    assert!(output.artifacts().len() > 1);
    assert_output_key_ranges_are_non_overlapping(&output);
    assert_artifact_facts_match_rows(&output, "stream-output-ranges");
    assert_eq!(output_storage_rows(&output), sorted_storage_rows(&rows));
}
