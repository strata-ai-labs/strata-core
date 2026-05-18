use crate::row::{PhysicalKey, StorageRow, StorageSpaceId};
use crate::table::{
    BoundedTableCursor, CursorMergePath, MemoryTableCursor, MergeTableCursor, MutableTable,
    TableCursor, TableInternalKeyBytes, TableKeyBound, TableKeyBounds, TablePhysicalKeyBytes,
    TableRow, MERGE_HEAP_THRESHOLD,
};
use strata_core_next::{BranchId, CommitVersion, Timestamp};

fn branch(byte: u8) -> BranchId {
    BranchId::from_bytes([byte; BranchId::BYTE_LEN])
}

fn physical_key(branch_byte: u8, space_id: u8, user_key: impl Into<Vec<u8>>) -> PhysicalKey {
    PhysicalKey::new(
        branch(branch_byte),
        "default",
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
        Timestamp::from_micros(version.saturating_add(10)),
        Timestamp::EPOCH,
        value,
    )
}

fn expired_row(user_key: impl Into<Vec<u8>>, version: u64) -> StorageRow {
    StorageRow::put(
        physical_key(1, 0x20, user_key),
        CommitVersion::new(version),
        Timestamp::from_micros(version.saturating_add(10)),
        Timestamp::from_micros(1),
        b"expired".to_vec(),
    )
}

fn tombstone_row(user_key: impl Into<Vec<u8>>, version: u64) -> StorageRow {
    StorageRow::tombstone(
        physical_key(1, 0x20, user_key),
        CommitVersion::new(version),
        Timestamp::from_micros(version.saturating_add(10)),
    )
}

fn table_from_rows(rows: Vec<StorageRow>) -> MutableTable {
    let mut table = MutableTable::new();
    for row in rows {
        table.insert_row(row).expect("insert row");
    }
    table
}

fn sorted_table_rows(rows: &[StorageRow]) -> Vec<TableRow> {
    let mut rows = rows.iter().cloned().map(TableRow::new).collect::<Vec<_>>();
    rows.sort();
    rows
}

fn sorted_keys(rows: &[StorageRow]) -> Vec<Vec<u8>> {
    sorted_table_rows(rows)
        .iter()
        .map(|row| row.encoded_key().to_vec())
        .collect()
}

fn collect_keys(cursor: &mut impl TableCursor) -> Vec<Vec<u8>> {
    let mut keys = Vec::new();
    while let Some(row) = cursor.current() {
        keys.push(row.encoded_key().to_vec());
        cursor.advance().expect("advance cursor");
    }
    keys
}

fn collect_values(cursor: &mut impl TableCursor) -> Vec<Vec<u8>> {
    let mut values = Vec::new();
    while let Some(row) = cursor.current() {
        values.push(row.value().to_vec());
        cursor.advance().expect("advance cursor");
    }
    values
}

fn collect_row_facts(cursor: &mut impl TableCursor) -> Vec<(Vec<u8>, bool, Timestamp)> {
    let mut facts = Vec::new();
    while let Some(row) = cursor.current() {
        facts.push((
            row.encoded_key().to_vec(),
            row.is_tombstone(),
            row.expires_at(),
        ));
        cursor.advance().expect("advance cursor");
    }
    facts
}

fn expected_seek_key(
    rows: &[TableRow],
    target: &TableInternalKeyBytes,
) -> Option<TableInternalKeyBytes> {
    rows.iter()
        .find(|row| row.key() >= target)
        .map(|row| row.key().clone())
}

fn boxed_cursors<'a>(tables: &'a [MutableTable]) -> Vec<Box<dyn TableCursor + 'a>> {
    tables
        .iter()
        .map(|table| Box::new(table.cursor()) as Box<dyn TableCursor + 'a>)
        .collect()
}

#[test]
fn memory_cursor_state_contract_handles_empty_and_one_row_sources() {
    let empty = MutableTable::new();
    let mut cursor = empty.cursor();
    cursor.seek_to_first().expect("seek empty");
    assert!(!cursor.is_valid());
    assert!(cursor.current().is_none());
    assert!(cursor.current_key().is_none());
    cursor.advance().expect("advance empty");
    assert!(cursor.current().is_none());
    cursor
        .seek(&TableInternalKeyBytes::from_row(&put_row(
            b"missing".to_vec(),
            1,
        )))
        .expect("seek missing");
    assert!(cursor.current().is_none());

    let table = table_from_rows(vec![put_row(b"one".to_vec(), 1)]);
    let only_key = table.first_key().expect("first key").clone();
    let mut cursor = table.cursor();
    assert!(cursor.current().is_none());
    cursor.seek_to_first().expect("seek first");
    assert_eq!(cursor.current_key(), Some(&only_key));
    cursor.advance().expect("advance one");
    assert!(cursor.current().is_none());
    cursor.advance().expect("advance exhausted");
    assert!(cursor.current().is_none());
    cursor.seek_to_first().expect("re-seek first");
    assert_eq!(cursor.current_key(), Some(&only_key));
}

#[test]
fn mutable_and_frozen_cursors_seek_like_sorted_vector_model() {
    let same_key = physical_key(1, 0x20, b"same".to_vec());
    let rows = vec![
        put_row(b"charlie".to_vec(), 2),
        put_row_for_key(same_key.clone(), 3, b"older".to_vec()),
        put_row(b"alpha".to_vec(), 2),
        put_row_for_key(same_key, 9, b"newer".to_vec()),
        tombstone_row([0x00, 0x41], 4),
        expired_row(b"expired".to_vec(), 5),
        put_row_for_key(
            physical_key(2, 0x01, b"other-branch".to_vec()),
            1,
            Vec::new(),
        ),
    ];
    let model_rows = sorted_table_rows(&rows);
    let table = table_from_rows(rows);
    let frozen = table.clone().freeze();

    for target in [
        TableInternalKeyBytes::from_row(&put_row_for_key(
            physical_key(0, 0x20, b"before".to_vec()),
            1,
            Vec::new(),
        )),
        model_rows[0].key().clone(),
        model_rows[model_rows.len() / 2].key().clone(),
        TableInternalKeyBytes::from_row(&put_row_for_key(
            physical_key(1, 0x20, b"between".to_vec()),
            8,
            Vec::new(),
        )),
        model_rows[model_rows.len() - 1].key().clone(),
        TableInternalKeyBytes::from_row(&put_row_for_key(
            physical_key(9, 0x20, b"after".to_vec()),
            1,
            Vec::new(),
        )),
    ] {
        let expected = expected_seek_key(&model_rows, &target);

        let mut mutable_cursor = table.cursor();
        mutable_cursor.seek(&target).expect("seek mutable");
        assert_eq!(mutable_cursor.current_key().cloned(), expected);

        let mut frozen_cursor = frozen.cursor();
        frozen_cursor.seek(&target).expect("seek frozen");
        assert_eq!(frozen_cursor.current_key().cloned(), expected);
    }
}

#[test]
fn bounded_cursor_respects_exact_range_and_physical_prefix_bounds() {
    let prefix_key = physical_key(1, 0x20, b"same".to_vec());
    let rows = vec![
        put_row(b"alpha".to_vec(), 1),
        tombstone_row(b"bravo".to_vec(), 2),
        expired_row(b"charlie".to_vec(), 3),
        put_row_for_key(prefix_key.clone(), 9, b"newer".to_vec()),
        put_row_for_key(prefix_key.clone(), 3, b"older".to_vec()),
        put_row_for_key(
            physical_key(2, 0x20, b"same".to_vec()),
            1,
            b"other".to_vec(),
        ),
    ];
    let table = table_from_rows(rows.clone());
    let model_rows = sorted_table_rows(&rows);

    let exact_key = model_rows[model_rows.len() / 2].key().clone();
    let mut exact = BoundedTableCursor::new(
        Box::new(table.cursor()),
        TableKeyBounds::exact(exact_key.clone()),
    );
    exact.seek_to_first().expect("seek exact");
    assert_eq!(
        collect_keys(&mut exact),
        vec![exact_key.as_slice().to_vec()]
    );

    let absent = TableInternalKeyBytes::from_row(&put_row(b"absent".to_vec(), 77));
    let mut missing =
        BoundedTableCursor::new(Box::new(table.cursor()), TableKeyBounds::exact(absent));
    missing.seek_to_first().expect("seek missing exact");
    assert!(collect_keys(&mut missing).is_empty());

    let lower = model_rows[1].key().clone();
    let upper = model_rows[model_rows.len() - 2].key().clone();
    let mut closed = BoundedTableCursor::new(
        Box::new(table.cursor()),
        TableKeyBounds::closed(lower.clone(), upper.clone()).expect("closed bounds"),
    );
    closed.seek_to_first().expect("seek closed");
    let expected_closed = model_rows
        .iter()
        .filter(|row| row.key() >= &lower && row.key() <= &upper)
        .map(|row| row.encoded_key().to_vec())
        .collect::<Vec<_>>();
    assert_eq!(collect_keys(&mut closed), expected_closed);

    let mut open = BoundedTableCursor::new(
        Box::new(table.cursor()),
        TableKeyBounds::open(lower.clone(), upper.clone()).expect("open bounds"),
    );
    open.seek_to_first().expect("seek open");
    let expected_open = model_rows
        .iter()
        .filter(|row| row.key() > &lower && row.key() < &upper)
        .map(|row| row.encoded_key().to_vec())
        .collect::<Vec<_>>();
    assert_eq!(collect_keys(&mut open), expected_open);

    let prefix = TablePhysicalKeyBytes::from_physical_key(&prefix_key);
    let prefix_bytes = prefix.as_slice().to_vec();
    let mut prefixed = BoundedTableCursor::new(
        Box::new(table.cursor()),
        TableKeyBounds::prefix(prefix.as_slice().to_vec()),
    );
    prefixed.seek_to_first().expect("seek prefix");
    let expected_prefix = model_rows
        .iter()
        .filter(|row| row.encoded_key().starts_with(&prefix_bytes))
        .map(TableRow::commit_version)
        .collect::<Vec<_>>();
    let actual_prefix = collect_keys(&mut prefixed)
        .into_iter()
        .map(TableInternalKeyBytes::from_canonical_bytes)
        .collect::<Result<Vec<_>, _>>()
        .expect("canonical prefix keys")
        .into_iter()
        .map(|key| key.commit_version().expect("commit version"))
        .collect::<Vec<_>>();
    assert_eq!(
        actual_prefix, expected_prefix,
        "prefix cursor must return all raw versions in encoded order"
    );
}

#[test]
fn bounded_cursor_covers_unbounded_and_equal_bound_shapes() {
    let rows = vec![
        put_row(b"alpha".to_vec(), 1),
        tombstone_row(b"bravo".to_vec(), 2),
        expired_row(b"charlie".to_vec(), 3),
        put_row(b"delta".to_vec(), 4),
    ];
    let table = table_from_rows(rows.clone());
    let model_rows = sorted_table_rows(&rows);
    let all_keys = model_rows
        .iter()
        .map(|row| row.encoded_key().to_vec())
        .collect::<Vec<_>>();

    let mut unbounded =
        BoundedTableCursor::new(Box::new(table.cursor()), TableKeyBounds::unbounded());
    unbounded.seek_to_first().expect("seek unbounded");
    assert_eq!(collect_keys(&mut unbounded), all_keys);

    let singleton = model_rows[model_rows.len() / 2].key().clone();
    let mut equal_inclusive = BoundedTableCursor::new(
        Box::new(table.cursor()),
        TableKeyBounds::closed(singleton.clone(), singleton.clone()).expect("equal inclusive"),
    );
    equal_inclusive
        .seek_to_first()
        .expect("seek equal inclusive");
    assert_eq!(
        collect_keys(&mut equal_inclusive),
        vec![singleton.as_slice().to_vec()]
    );

    let mut equal_exclusive = BoundedTableCursor::new(
        Box::new(table.cursor()),
        TableKeyBounds::open(singleton.clone(), singleton.clone()).expect("equal exclusive"),
    );
    equal_exclusive
        .seek_to_first()
        .expect("seek equal exclusive");
    assert!(collect_keys(&mut equal_exclusive).is_empty());

    let upper = model_rows[model_rows.len() / 2].key().clone();
    let mut lower_unbounded = BoundedTableCursor::new(
        Box::new(table.cursor()),
        TableKeyBounds::range(
            TableKeyBound::Unbounded,
            TableKeyBound::included(upper.clone()),
        )
        .expect("lower unbounded"),
    );
    lower_unbounded
        .seek_to_first()
        .expect("seek lower unbounded");
    assert_eq!(
        collect_keys(&mut lower_unbounded),
        model_rows
            .iter()
            .filter(|row| row.key() <= &upper)
            .map(|row| row.encoded_key().to_vec())
            .collect::<Vec<_>>()
    );

    let lower = model_rows[model_rows.len() / 3].key().clone();
    let mut upper_unbounded = BoundedTableCursor::new(
        Box::new(table.cursor()),
        TableKeyBounds::range(
            TableKeyBound::excluded(lower.clone()),
            TableKeyBound::Unbounded,
        )
        .expect("upper unbounded"),
    );
    upper_unbounded
        .seek_to_first()
        .expect("seek upper unbounded");
    assert_eq!(
        collect_keys(&mut upper_unbounded),
        model_rows
            .iter()
            .filter(|row| row.key() > &lower)
            .map(|row| row.encoded_key().to_vec())
            .collect::<Vec<_>>()
    );
}

#[test]
fn bounded_cursor_seek_repositions_inside_bounds() {
    let rows = vec![
        put_row(b"alpha".to_vec(), 1),
        put_row(b"bravo".to_vec(), 1),
        put_row(b"charlie".to_vec(), 1),
        put_row(b"delta".to_vec(), 1),
        put_row(b"echo".to_vec(), 1),
    ];
    let table = table_from_rows(rows.clone());
    let model_rows = sorted_table_rows(&rows);
    let lower = model_rows[1].key().clone();
    let upper = model_rows[3].key().clone();
    let bounds = TableKeyBounds::closed(lower.clone(), upper.clone()).expect("closed bounds");

    let mut cursor = BoundedTableCursor::new(Box::new(table.cursor()), bounds.clone());
    cursor
        .seek(model_rows[0].key())
        .expect("seek before lower bound");
    assert_eq!(cursor.current_key(), Some(&lower));

    cursor
        .seek(model_rows[2].key())
        .expect("seek inside bounds");
    assert_eq!(cursor.current_key(), Some(model_rows[2].key()));

    cursor
        .seek(model_rows[4].key())
        .expect("seek after upper bound");
    assert!(cursor.current().is_none());

    let mut open = BoundedTableCursor::new(
        Box::new(table.cursor()),
        TableKeyBounds::open(lower.clone(), upper.clone()).expect("open bounds"),
    );
    open.seek(&lower).expect("seek excluded lower");
    assert_eq!(
        open.current_key(),
        Some(model_rows[2].key()),
        "open lower bound must skip the excluded endpoint"
    );
    open.seek(&upper).expect("seek excluded upper");
    assert!(open.current().is_none());
}

#[test]
fn merge_cursor_handles_zero_one_and_disjoint_sources() {
    let mut empty = MergeTableCursor::new(Vec::new());
    assert_eq!(empty.path(), CursorMergePath::Empty);
    empty.seek_to_first().expect("seek empty merge");
    assert!(empty.current().is_none());

    let single_table = table_from_rows(vec![put_row(b"a".to_vec(), 1), put_row(b"c".to_vec(), 1)]);
    let mut single = MergeTableCursor::new(vec![Box::new(single_table.cursor())]);
    assert_eq!(single.path(), CursorMergePath::Single);
    single.seek_to_first().expect("seek single merge");
    assert_eq!(
        collect_keys(&mut single),
        single_table
            .iter()
            .map(|row| row.encoded_key().to_vec())
            .collect::<Vec<_>>()
    );

    let tables = vec![
        table_from_rows(vec![put_row(b"a".to_vec(), 1), put_row(b"d".to_vec(), 1)]),
        table_from_rows(vec![put_row(b"b".to_vec(), 1), put_row(b"c".to_vec(), 1)]),
    ];
    let expected = tables
        .iter()
        .flat_map(MutableTable::iter)
        .map(|row| row.encoded_key().to_vec())
        .collect::<Vec<_>>();
    let mut expected_sorted = expected;
    expected_sorted.sort();

    let mut merge = MergeTableCursor::new(boxed_cursors(&tables));
    assert_eq!(merge.path(), CursorMergePath::Linear);
    merge.seek_to_first().expect("seek disjoint merge");
    assert_eq!(collect_keys(&mut merge), expected_sorted);
}

#[test]
fn merge_cursor_handles_empty_children_and_preserves_raw_row_shapes() {
    let rows = vec![
        tombstone_row(b"delete".to_vec(), 8),
        expired_row(b"expired".to_vec(), 7),
        put_row(b"live".to_vec(), 6),
        put_row_for_key(
            physical_key(1, 0x20, b"versioned".to_vec()),
            10,
            b"newer".to_vec(),
        ),
        put_row_for_key(
            physical_key(1, 0x20, b"versioned".to_vec()),
            2,
            b"older".to_vec(),
        ),
    ];
    let nonempty = table_from_rows(rows.clone());
    let tables = vec![MutableTable::new(), nonempty, MutableTable::new()];
    let expected = sorted_table_rows(&rows)
        .iter()
        .map(|row| {
            (
                row.encoded_key().to_vec(),
                row.is_tombstone(),
                row.expires_at(),
            )
        })
        .collect::<Vec<_>>();

    let mut merge = MergeTableCursor::new(boxed_cursors(&tables));
    assert_eq!(merge.path(), CursorMergePath::Linear);
    merge.seek_to_first().expect("seek mixed empty merge");
    assert_eq!(collect_row_facts(&mut merge), expected);

    let empty_tables = vec![MutableTable::new(), MutableTable::new()];
    let mut empty_merge = MergeTableCursor::new(boxed_cursors(&empty_tables));
    empty_merge.seek_to_first().expect("seek empty children");
    assert!(empty_merge.current().is_none());
}

#[test]
fn merge_cursor_preserves_equal_keys_in_source_order_for_linear_and_heap_paths() {
    let shared_key = physical_key(1, 0x20, b"shared".to_vec());

    for source_count in [3usize, MERGE_HEAP_THRESHOLD + 1] {
        let tables = (0..source_count)
            .map(|index| {
                table_from_rows(vec![put_row_for_key(
                    shared_key.clone(),
                    9,
                    vec![u8::try_from(index).expect("small source index")],
                )])
            })
            .collect::<Vec<_>>();

        let mut merge = MergeTableCursor::new(boxed_cursors(&tables));
        if source_count <= MERGE_HEAP_THRESHOLD {
            assert_eq!(merge.path(), CursorMergePath::Linear);
        } else {
            assert_eq!(merge.path(), CursorMergePath::Heap);
        }
        merge.seek_to_first().expect("seek equal-key merge");

        let expected = (0..source_count)
            .map(|index| vec![u8::try_from(index).expect("small source index")])
            .collect::<Vec<_>>();
        assert_eq!(collect_values(&mut merge), expected);
    }
}

#[test]
fn heap_merge_covers_sixteen_sources_with_shared_key_order() {
    let shared_key = physical_key(1, 0x20, b"shared-sixteen".to_vec());
    let tables = (0..16usize)
        .map(|index| {
            table_from_rows(vec![put_row_for_key(
                shared_key.clone(),
                5,
                vec![u8::try_from(index).expect("small source index")],
            )])
        })
        .collect::<Vec<_>>();

    let mut merge = MergeTableCursor::new(boxed_cursors(&tables));
    assert_eq!(merge.path(), CursorMergePath::Heap);
    merge.seek_to_first().expect("seek sixteen-source merge");
    assert_eq!(
        collect_values(&mut merge),
        (0..16usize)
            .map(|index| vec![u8::try_from(index).expect("small source index")])
            .collect::<Vec<_>>()
    );
}

#[test]
fn heap_merge_preserves_tie_break_after_advancing_selected_child() {
    let key_a = physical_key(1, 0x20, b"a".to_vec());
    let key_b = physical_key(1, 0x20, b"b".to_vec());
    let tables = vec![
        table_from_rows(vec![
            put_row_for_key(key_a, 5, b"source-0-a".to_vec()),
            put_row_for_key(key_b.clone(), 5, b"source-0-b".to_vec()),
        ]),
        table_from_rows(vec![put_row_for_key(
            key_b.clone(),
            5,
            b"source-1-b".to_vec(),
        )]),
        table_from_rows(vec![put_row_for_key(
            physical_key(1, 0x20, b"c".to_vec()),
            5,
            b"source-2-c".to_vec(),
        )]),
        table_from_rows(vec![put_row_for_key(
            physical_key(1, 0x20, b"d".to_vec()),
            5,
            b"source-3-d".to_vec(),
        )]),
        table_from_rows(vec![put_row_for_key(
            physical_key(1, 0x20, b"e".to_vec()),
            5,
            b"source-4-e".to_vec(),
        )]),
    ];

    let mut merge = MergeTableCursor::new(boxed_cursors(&tables));
    assert_eq!(merge.path(), CursorMergePath::Heap);
    merge.seek_to_first().expect("seek heap merge");

    assert_eq!(
        collect_values(&mut merge),
        vec![
            b"source-0-a".to_vec(),
            b"source-0-b".to_vec(),
            b"source-1-b".to_vec(),
            b"source-2-c".to_vec(),
            b"source-3-d".to_vec(),
            b"source-4-e".to_vec(),
        ]
    );
}

#[test]
fn merge_cursor_current_is_stable_until_advance_and_only_selected_child_advances() {
    let tables = vec![
        table_from_rows(vec![
            put_row(b"alpha".to_vec(), 1),
            put_row(b"zulu".to_vec(), 1),
        ]),
        table_from_rows(vec![put_row(b"bravo".to_vec(), 1)]),
    ];
    let alpha = TableInternalKeyBytes::from_row(&put_row(b"alpha".to_vec(), 1));
    let bravo = TableInternalKeyBytes::from_row(&put_row(b"bravo".to_vec(), 1));

    let mut merge = MergeTableCursor::new(boxed_cursors(&tables));
    merge.seek_to_first().expect("seek merge");
    assert_eq!(merge.current_key(), Some(&alpha));
    assert_eq!(
        merge.current_key(),
        Some(&alpha),
        "current row must remain stable until advance"
    );

    merge.advance().expect("advance selected child");
    assert_eq!(
        merge.current_key(),
        Some(&bravo),
        "advancing the selected child must not advance every child"
    );
}

#[test]
fn merge_cursor_seek_and_reseek_repositions_all_children() {
    let rows_a = vec![put_row(b"alpha".to_vec(), 1), put_row(b"delta".to_vec(), 1)];
    let rows_b = vec![
        put_row(b"bravo".to_vec(), 1),
        put_row(b"charlie".to_vec(), 1),
    ];
    let tables = vec![
        table_from_rows(rows_a.clone()),
        table_from_rows(rows_b.clone()),
    ];
    let all_rows = [rows_a, rows_b].concat();
    let all_model = sorted_table_rows(&all_rows);

    let charlie = TableInternalKeyBytes::from_row(&put_row(b"charlie".to_vec(), 1));
    let mut merge = MergeTableCursor::new(boxed_cursors(&tables));
    merge.seek(&charlie).expect("seek charlie");
    assert_eq!(merge.current_key(), Some(&charlie));
    merge.advance().expect("advance once");
    assert_eq!(
        merge.current_key(),
        Some(TableInternalKeyBytes::from_row(&put_row(
            b"delta".to_vec(),
            1
        )))
        .as_ref()
    );

    merge.seek_to_first().expect("re-seek first");
    assert_eq!(
        collect_keys(&mut merge),
        all_model
            .iter()
            .map(|row| row.encoded_key().to_vec())
            .collect::<Vec<_>>()
    );

    let after = TableInternalKeyBytes::from_row(&put_row_for_key(
        physical_key(9, 0x20, b"after".to_vec()),
        1,
        Vec::new(),
    ));
    merge.seek(&after).expect("seek after");
    assert!(merge.current().is_none());
    merge.seek_to_first().expect("re-seek after exhaustion");
    assert_eq!(
        merge.current_key(),
        Some(all_model.first().expect("first model row").key())
    );
}

#[test]
fn heap_merge_reseek_after_partial_consumption_and_exhaustion() {
    let tables = (0..(MERGE_HEAP_THRESHOLD + 2))
        .map(|index| {
            table_from_rows(vec![
                put_row_for_key(
                    physical_key(1, 0x20, format!("k-{index:02}-a").into_bytes()),
                    1,
                    vec![u8::try_from(index).expect("small source index"), 0],
                ),
                put_row_for_key(
                    physical_key(1, 0x20, format!("k-{index:02}-b").into_bytes()),
                    1,
                    vec![u8::try_from(index).expect("small source index"), 1],
                ),
            ])
        })
        .collect::<Vec<_>>();
    let expected = tables
        .iter()
        .flat_map(MutableTable::iter)
        .map(|row| row.encoded_key().to_vec())
        .collect::<Vec<_>>();
    let mut expected_sorted = expected;
    expected_sorted.sort();
    let seek_key = TableInternalKeyBytes::from_canonical_bytes(
        expected_sorted[expected_sorted.len() / 2].clone(),
    )
    .expect("canonical seek key");

    let mut merge = MergeTableCursor::new(boxed_cursors(&tables));
    assert_eq!(merge.path(), CursorMergePath::Heap);
    merge.seek(&seek_key).expect("seek heap merge");
    assert_eq!(merge.current_key(), Some(&seek_key));
    merge.advance().expect("partial advance");

    merge.seek(&seek_key).expect("re-seek heap merge");
    assert_eq!(merge.current_key(), Some(&seek_key));

    let after = TableInternalKeyBytes::from_row(&put_row_for_key(
        physical_key(9, 0x20, b"after".to_vec()),
        1,
        Vec::new(),
    ));
    merge.seek(&after).expect("seek heap after end");
    assert!(merge.current().is_none());

    merge
        .seek_to_first()
        .expect("re-seek heap after exhaustion");
    assert_eq!(collect_keys(&mut merge), expected_sorted);
}

#[test]
fn bounded_cursor_starts_unpositioned_even_when_inner_is_prepositioned() {
    let rows = vec![put_row(b"alpha".to_vec(), 1), put_row(b"omega".to_vec(), 1)];
    let table = table_from_rows(rows);
    let alpha = TableInternalKeyBytes::from_row(&put_row(b"alpha".to_vec(), 1));
    let omega = TableInternalKeyBytes::from_row(&put_row(b"omega".to_vec(), 1));

    let mut inner_in_bounds = table.cursor();
    inner_in_bounds
        .seek(&omega)
        .expect("position inner inside bounds");
    let mut in_bounds = BoundedTableCursor::new(
        Box::new(inner_in_bounds),
        TableKeyBounds::exact(omega.clone()),
    );
    assert!(
        in_bounds.current().is_none(),
        "bounded cursor must not leak a pre-positioned in-bounds inner row before seek"
    );
    in_bounds.advance().expect("advance before seek is inert");
    assert!(in_bounds.current().is_none());
    in_bounds.seek_to_first().expect("seek in-bounds wrapper");
    assert_eq!(in_bounds.current_key(), Some(&omega));

    let mut inner = table.cursor();
    inner.seek(&alpha).expect("position inner outside bounds");

    let bounded = BoundedTableCursor::new(Box::new(inner), TableKeyBounds::exact(omega));
    assert!(
        bounded.current().is_none(),
        "bounded cursor must not leak a pre-positioned out-of-bounds inner row"
    );
}

#[test]
fn single_merge_cursor_starts_unpositioned_even_when_child_is_prepositioned() {
    let table = table_from_rows(vec![
        put_row(b"alpha".to_vec(), 1),
        put_row(b"omega".to_vec(), 1),
    ]);
    let first_key = table.first_key().expect("first key").clone();
    let mut child = table.cursor();
    child.seek_to_first().expect("pre-position child");

    let mut merge = MergeTableCursor::new(vec![Box::new(child)]);
    assert_eq!(merge.path(), CursorMergePath::Single);
    assert!(
        merge.current().is_none(),
        "single-source merge cursor must start unpositioned even if its child was positioned"
    );
    merge.advance().expect("advance before seek is inert");
    assert!(merge.current().is_none());
    merge.seek_to_first().expect("seek single merge");
    assert_eq!(merge.current_key(), Some(&first_key));
}

#[test]
fn multi_merge_cursors_start_unpositioned_even_when_children_are_prepositioned() {
    for source_count in [2usize, MERGE_HEAP_THRESHOLD + 1] {
        let tables = (0..source_count)
            .map(|index| {
                table_from_rows(vec![put_row_for_key(
                    physical_key(1, 0x20, format!("source-{index}").into_bytes()),
                    1,
                    vec![u8::try_from(index).expect("small source index")],
                )])
            })
            .collect::<Vec<_>>();
        let expected_first = tables[0].first_key().expect("first key").clone();

        let mut children = boxed_cursors(&tables);
        for child in &mut children {
            child.seek_to_first().expect("pre-position child");
        }

        let mut merge = MergeTableCursor::new(children);
        if source_count <= MERGE_HEAP_THRESHOLD {
            assert_eq!(merge.path(), CursorMergePath::Linear);
        } else {
            assert_eq!(merge.path(), CursorMergePath::Heap);
        }
        assert!(
            merge.current().is_none(),
            "multi-source merge cursor must not expose pre-positioned children before seek"
        );
        merge.advance().expect("advance before seek is inert");
        assert!(merge.current().is_none());
        merge.seek_to_first().expect("seek multi-source merge");
        assert_eq!(merge.current_key(), Some(&expected_first));
    }
}

#[test]
fn memory_cursor_from_sorted_rows_rejects_invalid_order_and_duplicates() {
    let rows = vec![put_row(b"a".to_vec(), 1), put_row(b"b".to_vec(), 1)];
    let table_rows = sorted_table_rows(&rows);
    let mut cursor =
        MemoryTableCursor::from_sorted_rows(table_rows.iter()).expect("sorted unique rows");
    cursor.seek_to_first().expect("seek rows");
    assert_eq!(collect_keys(&mut cursor), sorted_keys(&rows));

    let unsorted = [table_rows[1].clone(), table_rows[0].clone()];
    assert!(MemoryTableCursor::from_sorted_rows(unsorted.iter()).is_err());

    let duplicate = [table_rows[0].clone(), table_rows[0].clone()];
    assert!(MemoryTableCursor::from_sorted_rows(duplicate.iter()).is_err());
}

#[test]
fn merge_cursor_selects_expected_path_by_source_count() {
    let tables = (0..=MERGE_HEAP_THRESHOLD)
        .map(|_| MutableTable::new())
        .collect::<Vec<_>>();

    assert_eq!(
        MergeTableCursor::new(Vec::new()).path(),
        CursorMergePath::Empty
    );
    assert_eq!(
        MergeTableCursor::new(boxed_cursors(&tables[..1])).path(),
        CursorMergePath::Single
    );
    assert_eq!(
        MergeTableCursor::new(boxed_cursors(&tables[..2])).path(),
        CursorMergePath::Linear
    );
    assert_eq!(
        MergeTableCursor::new(boxed_cursors(&tables[..MERGE_HEAP_THRESHOLD])).path(),
        CursorMergePath::Linear
    );
    assert_eq!(
        MergeTableCursor::new(boxed_cursors(&tables[..=MERGE_HEAP_THRESHOLD])).path(),
        CursorMergePath::Heap
    );
}
