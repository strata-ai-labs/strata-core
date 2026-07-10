use super::*;
use strata_core::CommitVersion;

fn open_seeded_service<'a>(backend: &'a StoredWalBackend, records: &[WalRecord]) -> WalService<'a> {
    let segment = ObjectLayout::wal_segment(1).expect("segment one");
    backend
        .write_object(&segment, &segment_bytes(1, records))
        .expect("seed WAL segment");
    WalService::open(
        backend,
        database_id(),
        1,
        DurabilityPolicy::Standard,
        WalServiceConfig::default(),
    )
    .expect("open WAL")
}

#[test]
fn read_after_zero_commit_version_returns_all_records() {
    let backend = StoredWalBackend::new();
    let records = vec![
        record(1, b"first".to_vec()),
        record(2, b"second".to_vec()),
        record(3, b"third".to_vec()),
    ];
    let service = open_seeded_service(&backend, &records);

    let read = service
        .read_after_commit_version(CommitVersion::ZERO)
        .expect("read after zero watermark");

    assert_eq!(read.records(), records.as_slice());
    assert_eq!(read.truncation(), None);
}

#[test]
fn read_after_max_commit_version_returns_no_records() {
    let backend = StoredWalBackend::new();
    let records = vec![record(1, b"first".to_vec()), record(2, b"second".to_vec())];
    let service = open_seeded_service(&backend, &records);

    let read = service
        .read_after_commit_version(CommitVersion::MAX)
        .expect("read after max watermark");

    assert!(read.records().is_empty());
    assert_eq!(read.truncation(), None);
}

#[test]
fn read_all_preserves_multi_row_commit_payload_order() {
    let backend = StoredWalBackend::new();
    let record = multi_row_record(5, &[b"first", b"second", b"third"]);
    let service = open_seeded_service(&backend, std::slice::from_ref(&record));

    let read = service.read_all().expect("read multi-row record");

    assert_eq!(read.records(), std::slice::from_ref(&record));
    let rows = read.records()[0].commit_payload().rows();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].value(), b"first");
    assert_eq!(rows[1].value(), b"second");
    assert_eq!(rows[2].value(), b"third");
    assert_eq!(read.truncation(), None);
}

#[test]
fn read_after_duplicate_commit_versions_filters_by_version() {
    let backend = StoredWalBackend::new();
    let records = vec![
        record(1, b"version one".to_vec()),
        record(2, b"version two a".to_vec()),
        record(2, b"version two b".to_vec()),
        record(3, b"version three".to_vec()),
    ];
    let service = open_seeded_service(&backend, &records);

    let after_one = service
        .read_after_commit_version(CommitVersion::new(1))
        .expect("read duplicate versions after one");
    let after_two = service
        .read_after_commit_version(CommitVersion::new(2))
        .expect("read duplicate versions after two");

    assert_eq!(
        after_one.records(),
        &[records[1].clone(), records[2].clone(), records[3].clone()]
    );
    assert_eq!(after_two.records(), std::slice::from_ref(&records[3]));
}

#[test]
fn read_after_out_of_order_commit_versions_keeps_append_order() {
    let backend = StoredWalBackend::new();
    let records = vec![
        record(7, b"first appended".to_vec()),
        record(2, b"second appended".to_vec()),
        record(5, b"third appended".to_vec()),
        record(3, b"fourth appended".to_vec()),
    ];
    let service = open_seeded_service(&backend, &records);

    let all = service.read_all().expect("read all out-of-order records");
    let after_three = service
        .read_after_commit_version(CommitVersion::new(3))
        .expect("read out-of-order records after three");

    assert_eq!(all.records(), records.as_slice());
    assert_eq!(
        after_three.records(),
        &[records[0].clone(), records[2].clone()]
    );
}

#[test]
fn read_after_preserves_records_from_different_branch_ids() {
    let backend = StoredWalBackend::new();
    let other_branch = alternate_branch_id();
    let records = vec![
        record(1, b"default branch".to_vec()),
        record_for_branch(2, other_branch, b"other branch".to_vec()),
        record(3, b"default branch again".to_vec()),
    ];
    let service = open_seeded_service(&backend, &records);

    let read = service
        .read_after_commit_version(CommitVersion::new(1))
        .expect("read after branch-mixed records");

    // WAL is a physical log; branch interpretation belongs to commit replay.
    assert_eq!(read.records(), &[records[1].clone(), records[2].clone()]);
    assert_eq!(read.records()[0].branch_id(), other_branch);
    assert_eq!(
        read.records()[0].commit_payload().rows()[0]
            .physical_key()
            .branch_id(),
        other_branch
    );
    assert_eq!(read.truncation(), None);
}
