use super::super::key::encode_internal_key;
use super::artifact::{decode_immutable_table, encode_immutable_table};
use super::TableCompression;
use crate::row::{InternalKey, PhysicalKey, StorageRow, StorageSpaceId};
use proptest::collection::vec;
use proptest::prelude::{any, prop_oneof, Just, Strategy};
use proptest::prop_assert_eq;
use proptest::test_runner::{Config, FileFailurePersistence, TestRunner};
use strata_core::{BranchId, CommitVersion, Timestamp};

#[test]
fn table_format_model_round_trips_generated_rows() {
    let strategy = vec(row_input_strategy(), 1..=128);
    let mut runner = TestRunner::new(Config {
        failure_persistence: Some(Box::new(FileFailurePersistence::Direct(
            "proptest-regressions/table_format.txt",
        ))),
        ..Config::default()
    });

    runner
        .run(&strategy, |inputs| {
            let rows = sorted_rows_from_inputs(&inputs);
            for compression in [TableCompression::Uncompressed, TableCompression::Zstd] {
                for rows_per_block in table_split_cases(rows.len()) {
                    for target_data_block_size in table_target_data_block_size_cases() {
                        assert_table_matches_model(
                            &rows,
                            rows_per_block,
                            target_data_block_size,
                            compression,
                        )?;
                    }
                }
            }
            Ok(())
        })
        .expect("table format model property");
}

#[derive(Clone, Debug)]
struct RowInput {
    user_key: Vec<u8>,
    value: Vec<u8>,
    tombstone: bool,
}

fn assert_table_matches_model(
    rows: &[StorageRow],
    rows_per_block: usize,
    target_data_block_size: u32,
    compression: TableCompression,
) -> Result<(), proptest::test_runner::TestCaseError> {
    let bytes = encode_immutable_table(rows, target_data_block_size, rows_per_block, compression)
        .expect("encode generated table");
    let decoded = decode_immutable_table(&bytes).expect("decode generated table");
    prop_assert_eq!(decoded.rows(), rows);
    prop_assert_eq!(
        decoded.header().target_data_block_size(),
        target_data_block_size
    );
    prop_assert_eq!(decoded.header().row_count(), rows.len() as u64);
    let expected_block_count = rows.chunks(rows_per_block).count();
    prop_assert_eq!(
        decoded.header().data_block_count(),
        u32::try_from(expected_block_count).expect("block count")
    );
    prop_assert_eq!(decoded.data_blocks().len(), expected_block_count);
    prop_assert_eq!(decoded.index().entry_count(), expected_block_count);
    prop_assert_eq!(
        decoded.properties().row_count(),
        u64::try_from(rows.len()).expect("row count")
    );
    prop_assert_eq!(
        decoded.properties().data_block_count() as usize,
        decoded.data_blocks().len()
    );
    prop_assert_eq!(
        decoded.properties().commit_min(),
        rows.iter()
            .map(StorageRow::commit_version)
            .min()
            .expect("min commit")
    );
    prop_assert_eq!(
        decoded.properties().commit_max(),
        rows.iter()
            .map(StorageRow::commit_version)
            .max()
            .expect("max commit")
    );
    assert_properties_key_range_matches_rows(rows, &decoded)?;
    assert_index_entries_match_rows(rows, rows_per_block, &decoded)?;
    Ok(())
}

fn assert_properties_key_range_matches_rows(
    rows: &[StorageRow],
    decoded: &super::artifact::ImmutableTable,
) -> Result<(), proptest::test_runner::TestCaseError> {
    let expected_min_key = internal_key_for_row(rows.first().expect("first row"));
    let expected_max_key = internal_key_for_row(rows.last().expect("last row"));
    let expected_min_key_bytes = encode_internal_key(&expected_min_key);
    let expected_max_key_bytes = encode_internal_key(&expected_max_key);
    prop_assert_eq!(decoded.properties().min_key(), &expected_min_key);
    prop_assert_eq!(decoded.properties().max_key(), &expected_max_key);
    prop_assert_eq!(
        decoded.properties().min_key_bytes(),
        expected_min_key_bytes.as_slice()
    );
    prop_assert_eq!(
        decoded.properties().max_key_bytes(),
        expected_max_key_bytes.as_slice()
    );
    Ok(())
}

fn assert_index_entries_match_rows(
    rows: &[StorageRow],
    rows_per_block: usize,
    decoded: &super::artifact::ImmutableTable,
) -> Result<(), proptest::test_runner::TestCaseError> {
    for (index_entry, expected_rows) in decoded
        .index()
        .entries()
        .iter()
        .zip(rows.chunks(rows_per_block))
    {
        let first_key = internal_key_for_row(&expected_rows[0]);
        let last_key = internal_key_for_row(&expected_rows[expected_rows.len() - 1]);
        let first_key_bytes = encode_internal_key(&first_key);
        let last_key_bytes = encode_internal_key(&last_key);
        prop_assert_eq!(index_entry.first_key(), &first_key);
        prop_assert_eq!(index_entry.last_key(), &last_key);
        prop_assert_eq!(index_entry.first_key_bytes(), first_key_bytes.as_slice());
        prop_assert_eq!(index_entry.last_key_bytes(), last_key_bytes.as_slice());
        prop_assert_eq!(
            index_entry.row_count(),
            u32::try_from(expected_rows.len()).expect("block row count")
        );
    }
    Ok(())
}

fn row_input_strategy() -> impl Strategy<Value = RowInput> {
    let user_key = prop_oneof![
        1 => Just(b"duplicate".to_vec()),
        3 => vec(any::<u8>(), 0..=64),
    ];
    (user_key, vec(any::<u8>(), 0..=512), any::<bool>()).prop_map(|(user_key, value, tombstone)| {
        RowInput {
            user_key,
            value,
            tombstone,
        }
    })
}

fn sorted_rows_from_inputs(inputs: &[RowInput]) -> Vec<StorageRow> {
    let mut rows = inputs
        .iter()
        .enumerate()
        .map(|(index, input)| {
            let version = u64::try_from(inputs.len() - index).expect("version");
            row_for_parts(
                index,
                input.user_key.clone(),
                version,
                input.tombstone,
                input.value.clone(),
            )
        })
        .collect::<Vec<_>>();

    rows.sort_by_key(|row| encode_internal_key(&internal_key_for_row(row)));
    rows
}

fn table_split_cases(row_count: usize) -> Vec<usize> {
    if row_count == 1 {
        vec![1]
    } else {
        vec![row_count, row_count.div_ceil(3).max(1)]
    }
}

fn table_target_data_block_size_cases() -> [u32; 2] {
    [128, 4096]
}

fn row_for_parts(
    index: usize,
    user_key: Vec<u8>,
    version: u64,
    tombstone: bool,
    value: Vec<u8>,
) -> StorageRow {
    let physical_key = PhysicalKey::new(
        branch_id(),
        "default",
        StorageSpaceId::engine(0x20).expect("engine id"),
        disambiguated_user_key(index, user_key),
    )
    .expect("physical key");
    let version = CommitVersion::new(version);
    let timestamp = Timestamp::from_micros(1_700_000_000_000_000 + version.as_u64());
    if tombstone {
        StorageRow::tombstone(physical_key, version, timestamp)
    } else {
        StorageRow::put(physical_key, version, timestamp, Timestamp::EPOCH, value)
    }
}

fn internal_key_for_row(row: &StorageRow) -> InternalKey {
    InternalKey::new(row.physical_key().clone(), row.commit_version())
}

fn disambiguated_user_key(index: usize, user_key: Vec<u8>) -> Vec<u8> {
    if user_key == b"duplicate" {
        return user_key;
    }
    let mut key = format!("{index:04}:").into_bytes();
    key.extend_from_slice(&user_key);
    key
}

fn branch_id() -> BranchId {
    BranchId::from_bytes([0x33; BranchId::BYTE_LEN])
}
