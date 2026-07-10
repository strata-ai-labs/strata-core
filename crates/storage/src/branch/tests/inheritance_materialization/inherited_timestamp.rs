#[test]
fn branch_inherited_reads_apply_fork_gate_and_child_tombstone_shadowing() {
    let source = branch_id(74);
    let child = branch_id(75);
    let visible_source = storage_row_with(
        source,
        b"gate".to_vec(),
        3,
        30,
        Timestamp::EPOCH,
        b"visible".to_vec(),
    );
    let post_fork_source = storage_row_with(
        source,
        b"gate".to_vec(),
        7,
        70,
        Timestamp::EPOCH,
        b"post-fork".to_vec(),
    );
    let layer = branch_inherited_layer_unchecked_for_fork_gate_tests(
        source,
        CommitVersion::new(5),
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            source,
            BranchLevel::ZERO,
            "fork-gated-source",
            vec![visible_source.clone(), post_fork_source],
        )]],
    );
    let mut child_state = BranchLocalState::empty(child);
    child_state
        .attach_inherited_layers(vec![layer])
        .expect("attach inherited layer");

    let child_key = physical_key(child, b"gate".to_vec());
    let view = child_state
        .capture_read_view()
        .expect("view")
        .with_timestamp_coverage(BranchTimestampCoverage::complete());
    let expected = rewrite_row_branch(&visible_source, source, child).expect("rewrite");
    assert_visible_row(
        view.latest(&child_key).expect("latest").as_ref(),
        &expected,
        BranchRowSource::Inherited {
            source_branch_id: source,
            layer_index: 0,
        },
    );
    assert_visible_row(
        view.at_version(&child_key, CommitVersion::new(7))
            .expect("bounded")
            .as_ref(),
        &expected,
        BranchRowSource::Inherited {
            source_branch_id: source,
            layer_index: 0,
        },
    );

    child_state
        .append_committed_row(tombstone_row(child, b"gate".to_vec(), 6, 60))
        .expect("child tombstone");
    let shadowed = child_state.capture_read_view().expect("shadowed view");
    assert!(
        shadowed.latest(&child_key).expect("latest").is_none(),
        "child tombstone must shadow inherited put"
    );
    assert_visible_row(
        shadowed
            .at_version(&child_key, CommitVersion::new(4))
            .expect("before tombstone")
            .as_ref(),
        &expected,
        BranchRowSource::Inherited {
            source_branch_id: source,
            layer_index: 0,
        },
    );
}

#[test]
fn branch_inherited_timestamp_reads_apply_timestamp_and_fork_gates() {
    let source = branch_id(61);
    let child = branch_id(62);
    let visible_source = storage_row_with(
        source,
        b"time-gate".to_vec(),
        3,
        30,
        Timestamp::EPOCH,
        b"visible".to_vec(),
    );
    let source_future_timestamp = storage_row_with(
        source,
        b"time-gate".to_vec(),
        4,
        70,
        Timestamp::EPOCH,
        b"future-time".to_vec(),
    );
    let source_after_fork_with_old_timestamp = storage_row_with(
        source,
        b"time-gate".to_vec(),
        7,
        20,
        Timestamp::EPOCH,
        b"after-fork".to_vec(),
    );
    let layer = branch_inherited_layer_unchecked_for_fork_gate_tests(
        source,
        CommitVersion::new(5),
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            source,
            BranchLevel::ZERO,
            "inherited-time-gate",
            vec![
                visible_source.clone(),
                source_future_timestamp,
                source_after_fork_with_old_timestamp,
            ],
        )]],
    );
    let mut child_state = BranchLocalState::empty(child);
    child_state
        .attach_inherited_layers(vec![layer])
        .expect("attach inherited layer");
    let child_key = physical_key(child, b"time-gate".to_vec());
    let view = child_state
        .capture_read_view()
        .expect("view")
        .with_timestamp_coverage(BranchTimestampCoverage::complete());
    assert_visible_row(
        view.read_point(
            &child_key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(40)),
        )
        .expect("timestamp inherited read")
        .as_ref(),
        &rewrite_row_branch(&visible_source, source, child).expect("visible rewrite"),
        BranchRowSource::Inherited {
            source_branch_id: source,
            layer_index: 0,
        },
    );
    assert_eq!(
        view.read_point(
            &child_key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(25)),
        )
        .expect("below visible timestamp"),
        None,
        "post-fork source row with an old timestamp remains hidden by fork version",
    );

    let child_expired = storage_row_with(
        child,
        b"time-gate".to_vec(),
        6,
        35,
        Timestamp::from_micros(39),
        b"child-expired".to_vec(),
    );
    child_state
        .append_committed_row(child_expired)
        .expect("append child expired row");
    let shadowed = child_state.capture_read_view().expect("shadowed view");
    assert_eq!(
        shadowed
            .read_point(
                &child_key,
                BranchReadBound::at_timestamp(Timestamp::from_micros(40)),
            )
            .expect("timestamp expired child read"),
        None,
        "selected child-local expired row suppresses inherited fallback",
    );
}

#[test]
fn branch_inherited_timestamp_reads_pick_nearest_layer_for_exact_ties() {
    let (child_state, fixture) = inherited_timestamp_shadow_fixture();
    let inherited_view = child_state.capture_read_view().expect("view");
    assert_visible_row(
        inherited_view
            .read_point(
                &fixture.child_key,
                BranchReadBound::at_timestamp(Timestamp::from_micros(40)),
            )
            .expect("nearest inherited timestamp read")
            .as_ref(),
        &fixture.expected_nearest,
        BranchRowSource::Inherited {
            source_branch_id: fixture.nearest_source,
            layer_index: 0,
        },
    );
}

#[test]
fn branch_inherited_timestamp_reads_apply_local_put_and_tombstone_shadows() {
    let (mut child_state, fixture) = inherited_timestamp_shadow_fixture();
    let child_put = storage_row_with(
        fixture.child,
        fixture.key.clone(),
        4,
        35,
        Timestamp::EPOCH,
        b"child-put".to_vec(),
    );
    child_state
        .append_committed_row(child_put.clone())
        .expect("append child put");
    assert_visible_row(
        child_state
            .capture_read_view()
            .expect("put view")
            .read_point(
                &fixture.child_key,
                BranchReadBound::at_timestamp(Timestamp::from_micros(40)),
            )
            .expect("child put timestamp read")
            .as_ref(),
        &child_put,
        BranchRowSource::Active,
    );

    child_state
        .append_committed_row(tombstone_row(fixture.child, fixture.key, 5, 45))
        .expect("append child tombstone");
    assert_eq!(
        child_state
            .capture_read_view()
            .expect("tombstone view")
            .read_point(
                &fixture.child_key,
                BranchReadBound::at_timestamp(Timestamp::from_micros(50)),
            )
            .expect("child tombstone timestamp read"),
        None,
        "child-local tombstone at timestamp shadows inherited puts",
    );
}

struct InheritedTimestampShadowFixture {
    nearest_source: BranchId,
    child: BranchId,
    key: Vec<u8>,
    child_key: PhysicalKey,
    expected_nearest: StorageRow,
}

fn inherited_timestamp_shadow_fixture() -> (BranchLocalState, InheritedTimestampShadowFixture) {
    let nearest_source = branch_id(63);
    let farther_source = branch_id(64);
    let child = branch_id(65);
    let key = b"time-shadow".to_vec();
    let nearest_row = storage_row_with(
        nearest_source,
        key.clone(),
        3,
        30,
        Timestamp::EPOCH,
        b"nearest".to_vec(),
    );
    let farther_row = storage_row_with(
        farther_source,
        key.clone(),
        3,
        30,
        Timestamp::EPOCH,
        b"farther".to_vec(),
    );
    let nearest_layer = branch_inherited_layer(
        nearest_source,
        CommitVersion::new(5),
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            nearest_source,
            BranchLevel::ZERO,
            "nearest-timestamp-tie",
            vec![nearest_row.clone()],
        )]],
    );
    let farther_layer = branch_inherited_layer(
        farther_source,
        CommitVersion::new(5),
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            farther_source,
            BranchLevel::ZERO,
            "farther-timestamp-tie",
            vec![farther_row],
        )]],
    );
    let mut child_state = BranchLocalState::empty(child);
    child_state
        .attach_inherited_layers(vec![nearest_layer, farther_layer])
        .expect("attach inherited layers");
    let child_key = physical_key(child, key.clone());
    let expected_nearest =
        rewrite_row_branch(&nearest_row, nearest_source, child).expect("nearest rewrite");
    (
        child_state,
        InheritedTimestampShadowFixture {
            nearest_source,
            child,
            key,
            child_key,
            expected_nearest,
        },
    )
}

#[test]
fn branch_inherited_timestamp_view_is_pinned_after_source_mutation() {
    let source = branch_id(66);
    let child = branch_id(67);
    let mut source_state = BranchLocalState::empty(source);
    let inherited = storage_row_with(
        source,
        b"source-pinned-ts".to_vec(),
        3,
        30,
        Timestamp::EPOCH,
        b"inherited".to_vec(),
    );
    source_state
        .install_l0_table(branch_owned_table(
            source,
            BranchLevel::ZERO,
            "source-pinned-ts-base",
            vec![inherited.clone()],
        ))
        .expect("install base source table");
    let (child_state, _) = source_state
        .fork_into_empty_child(child)
        .expect("fork child");
    let pinned = child_state.capture_read_view().expect("pinned child view");

    source_state
        .install_l0_table(branch_owned_table(
            source,
            BranchLevel::ZERO,
            "source-pinned-ts-later",
            vec![storage_row_with(
                source,
                b"source-pinned-ts-later".to_vec(),
                4,
                40,
                Timestamp::EPOCH,
                b"later".to_vec(),
            )],
        ))
        .expect("install later source table");

    let expected = rewrite_row_branch(&inherited, source, child).expect("rewrite inherited");
    assert_visible_row(
        pinned
            .read_point(
                &physical_key(child, b"source-pinned-ts".to_vec()),
                BranchReadBound::at_timestamp(Timestamp::from_micros(50)),
            )
            .expect("pinned inherited timestamp read")
            .as_ref(),
        &expected,
        BranchRowSource::Inherited {
            source_branch_id: source,
            layer_index: 0,
        },
    );
    assert_eq!(
        pinned
            .read_point(
                &physical_key(child, b"source-pinned-ts-later".to_vec()),
                BranchReadBound::at_timestamp(Timestamp::from_micros(50)),
            )
            .expect("pinned inherited later read"),
        None,
        "captured child timestamp view must not observe later source mutation",
    );
}

