#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct ModelBranchStore {
    branches: Vec<ModelStoreBranch>,
}

impl ModelBranchStore {
    fn add_branch(&mut self, branch: BranchId) {
        if self.branches.iter().all(|stored| stored.branch != branch) {
            self.branches.push(ModelStoreBranch {
                branch,
                own_rows: Vec::new(),
                inherited_layers: Vec::new(),
            });
        }
    }

    fn branch_mut(&mut self, branch: BranchId) -> Result<&mut ModelStoreBranch, TestkitError> {
        self.branches
            .iter_mut()
            .find(|stored| stored.branch == branch)
            .ok_or_else(|| TestkitError::new("model branch missing"))
    }

    fn push_own(&mut self, row: StorageRow) -> Result<(), TestkitError> {
        let branch = row.physical_key().branch_id();
        self.branch_mut(branch)?.push_own(row)
    }

    fn attach_layer(
        &mut self,
        branch: BranchId,
        layer: ModelInheritedLayer,
    ) -> Result<(), TestkitError> {
        self.branch_mut(branch)?.inherited_layers.push(layer);
        Ok(())
    }

    fn visible(
        &self,
        branch: BranchId,
        key: &PhysicalKey,
        bound: BranchReadBound,
    ) -> Result<Option<StorageRow>, TestkitError> {
        let read_timestamp = match bound {
            BranchReadBound::Latest | BranchReadBound::AtVersion(_) => None,
            BranchReadBound::AtTimestamp(timestamp) => Some(timestamp),
        };
        let mut candidates = self
            .branch(branch)?
            .candidates(branch)?
            .into_iter()
            .filter(|candidate| candidate.row.physical_key() == key)
            .filter(|candidate| model_row_matches_bound(&candidate.row, bound))
            .collect::<Vec<_>>();
        sort_model_candidates_newest_first(&mut candidates);
        Ok(candidates.into_iter().next().and_then(|candidate| {
            if candidate.row.is_tombstone()
                || model_row_is_expired_at(&candidate.row, read_timestamp)
            {
                None
            } else {
                Some(candidate.row)
            }
        }))
    }

    fn history(
        &self,
        branch: BranchId,
        key: &PhysicalKey,
    ) -> Result<Vec<StorageRow>, TestkitError> {
        let mut candidates = self
            .branch(branch)?
            .candidates(branch)?
            .into_iter()
            .filter(|candidate| candidate.row.physical_key() == key)
            .collect::<Vec<_>>();
        sort_model_candidates_newest_first(&mut candidates);
        Ok(candidates
            .into_iter()
            .map(|candidate| candidate.row)
            .collect())
    }

    fn scan_prefix(
        &self,
        branch: BranchId,
        prefix: &[u8],
        bound: BranchReadBound,
    ) -> Result<Vec<StorageRow>, TestkitError> {
        self.scan(branch, |key| key.starts_with(prefix), bound)
    }

    fn scan_range(
        &self,
        branch: BranchId,
        lower: &[u8],
        upper: &[u8],
        bound: BranchReadBound,
    ) -> Result<Vec<StorageRow>, TestkitError> {
        self.scan(branch, |key| lower <= key && key <= upper, bound)
    }

    fn scan(
        &self,
        branch: BranchId,
        key_matches: impl Fn(&[u8]) -> bool,
        bound: BranchReadBound,
    ) -> Result<Vec<StorageRow>, TestkitError> {
        let mut keys = Vec::<PhysicalKey>::new();
        for candidate in self.branch(branch)?.candidates(branch)? {
            let key = candidate.row.physical_key();
            if key_matches(key.user_key()) && !keys.iter().any(|stored| stored == key) {
                keys.push(key.clone());
            }
        }
        keys.sort_by(|left, right| left.user_key().cmp(right.user_key()));
        let mut rows = Vec::new();
        for key in keys {
            if let Some(row) = self.visible(branch, &key, bound)? {
                rows.push(row);
            }
        }
        Ok(rows)
    }

    fn branch(&self, branch: BranchId) -> Result<&ModelStoreBranch, TestkitError> {
        self.branches
            .iter()
            .find(|stored| stored.branch == branch)
            .ok_or_else(|| TestkitError::new("model branch missing"))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ModelStoreBranch {
    branch: BranchId,
    own_rows: Vec<StorageRow>,
    inherited_layers: Vec<ModelInheritedLayer>,
}

impl ModelStoreBranch {
    fn push_own(&mut self, row: StorageRow) -> Result<(), TestkitError> {
        if row.physical_key().branch_id() != self.branch {
            return Err(TestkitError::new("model own row branch drifted"));
        }
        if self.own_rows.iter().any(|existing| {
            TableInternalKeyBytes::from_row(existing) == TableInternalKeyBytes::from_row(&row)
        }) {
            return Err(TestkitError::new("model duplicate own internal key"));
        }
        self.own_rows.push(row);
        Ok(())
    }

    fn candidates(&self, target: BranchId) -> Result<Vec<ModelCandidate>, TestkitError> {
        let mut candidates = self
            .own_rows
            .iter()
            .cloned()
            .map(|row| ModelCandidate {
                row,
                source_rank: 0,
            })
            .collect::<Vec<_>>();
        for (layer_index, layer) in self.inherited_layers.iter().enumerate() {
            for row in &layer.rows {
                if row.commit_version().as_u64() > layer.fork_version.as_u64() {
                    continue;
                }
                let rewritten =
                    rewrite_row_branch(row, layer.source_branch_id, target).map_err(|err| {
                        TestkitError::new(format!("model inherited rewrite failed: {err}"))
                    })?;
                candidates.push(ModelCandidate {
                    row: rewritten,
                    source_rank: layer_index.saturating_add(1),
                });
            }
        }
        Ok(candidates)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ModelInheritedLayer {
    source_branch_id: BranchId,
    fork_version: CommitVersion,
    rows: Vec<StorageRow>,
}

impl ModelInheritedLayer {
    fn new(source_branch_id: BranchId, fork_version: CommitVersion, rows: Vec<StorageRow>) -> Self {
        Self {
            source_branch_id,
            fork_version,
            rows,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ModelCandidate {
    row: StorageRow,
    source_rank: usize,
}

fn sort_model_candidates_newest_first(candidates: &mut [ModelCandidate]) {
    candidates.sort_by(|left, right| {
        right
            .row
            .commit_version()
            .as_u64()
            .cmp(&left.row.commit_version().as_u64())
            .then_with(|| left.source_rank.cmp(&right.source_rank))
    });
}

#[allow(clippy::too_many_lines)]
fn check_model_backed_inheritance_script(script: &[u8]) -> Result<(), TestkitError> {
    let far_source = branch_id(0x91);
    let near_source = branch_id(0x92);
    let child = branch_id(0x93);
    let fork_version = CommitVersion::new(18);
    let mut near_rows = Vec::new();
    let mut far_rows = Vec::new();
    let mut child_rows = Vec::new();

    for step in 0..16_u64 {
        let opcode = script_byte(script, 96 + usize::try_from(step).expect("step fits usize"));
        let key = format!("model-script-key-{:02}", opcode % 6).into_bytes();
        let timestamp = 10 + step;
        match opcode % 7 {
            0 => near_rows.push(storage_row_with(
                near_source,
                key,
                1 + step,
                timestamp,
                Timestamp::EPOCH,
                vec![opcode],
            )?),
            1 => near_rows.push(tombstone_row(near_source, key, 1 + step, timestamp)?),
            2 => near_rows.push(storage_row_with(
                near_source,
                key,
                1 + step,
                timestamp,
                Timestamp::from_micros(timestamp + 3),
                vec![opcode],
            )?),
            3 => far_rows.push(storage_row_with(
                far_source,
                key,
                1 + step,
                timestamp,
                Timestamp::EPOCH,
                vec![opcode],
            )?),
            4 => near_rows.push(storage_row_with(
                near_source,
                key,
                30 + step,
                100 + step,
                Timestamp::EPOCH,
                b"post-fork".to_vec(),
            )?),
            5 => child_rows.push(storage_row_with(
                child,
                key,
                40 + step,
                140 + step,
                Timestamp::EPOCH,
                vec![opcode],
            )?),
            _ => child_rows.push(tombstone_row(child, key, 40 + step, 140 + step)?),
        }
    }

    if near_rows.is_empty() {
        near_rows.push(storage_row_with(
            near_source,
            b"model-script-key-00".to_vec(),
            1,
            10,
            Timestamp::EPOCH,
            b"near".to_vec(),
        )?);
    }
    if far_rows.is_empty() {
        far_rows.push(storage_row_with(
            far_source,
            b"model-script-key-01".to_vec(),
            2,
            20,
            Timestamp::EPOCH,
            b"far".to_vec(),
        )?);
    }

    let near_layer = branch_inherited_layer_unchecked_for_fork_gate_checks(
        near_source,
        fork_version,
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            near_source,
            BranchLevel::ZERO,
            "model-script-near",
            near_rows.clone(),
        )?]],
    );
    let far_layer = branch_inherited_layer_unchecked_for_fork_gate_checks(
        far_source,
        fork_version,
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            far_source,
            BranchLevel::ZERO,
            "model-script-far",
            far_rows.clone(),
        )?]],
    );
    let mut state = BranchLocalState::empty(child);
    state
        .attach_inherited_layers(vec![near_layer, far_layer])
        .map_err(|err| TestkitError::new(format!("model script attach failed: {err}")))?;
    for row in &child_rows {
        state
            .append_committed_row(row.clone())
            .map_err(|err| TestkitError::new(format!("model script child append failed: {err}")))?;
    }

    let mut model = ModelBranchStore::default();
    model.add_branch(child);
    model.attach_layer(
        child,
        ModelInheritedLayer::new(near_source, fork_version, near_rows),
    )?;
    model.attach_layer(
        child,
        ModelInheritedLayer::new(far_source, fork_version, far_rows),
    )?;
    for row in child_rows {
        model.push_own(row)?;
    }

    let keys = [
        b"model-script-key-00".as_slice(),
        b"model-script-key-01".as_slice(),
        b"model-script-key-02".as_slice(),
        b"model-script-key-03".as_slice(),
        b"model-script-key-04".as_slice(),
        b"model-script-key-05".as_slice(),
    ];
    let view = state
        .capture_read_view()
        .map_err(|err| TestkitError::new(format!("model script view failed: {err}")))?
        .with_timestamp_coverage(BranchTimestampCoverage::complete());
    assert_model_store_read_surface(
        &view,
        &model,
        child,
        &keys,
        b"model-script-key-",
        Some((b"model-script-key-00", b"model-script-key-05")),
    )
}

#[allow(clippy::too_many_lines)]
fn check_model_backed_inheritance(script: &[u8]) -> Result<(), TestkitError> {
    let far_source = branch_id(0xa1);
    let near_source = branch_id(0xa2);
    let child = branch_id(0xa3);
    let shared_far = storage_row_with(
        far_source,
        b"model-inherit-shared".to_vec(),
        3,
        30,
        Timestamp::EPOCH,
        vec![script_byte(script, 16)],
    )?;
    let shared_near = storage_row_with(
        near_source,
        b"model-inherit-shared".to_vec(),
        3,
        35,
        Timestamp::EPOCH,
        vec![script_byte(script, 17)],
    )?;
    let post_fork = storage_row_with(
        near_source,
        b"model-inherit-post-fork".to_vec(),
        9,
        90,
        Timestamp::EPOCH,
        b"post".to_vec(),
    )?;
    let shadow_put = storage_row_with(
        near_source,
        b"model-inherit-shadow-put".to_vec(),
        2,
        20,
        Timestamp::EPOCH,
        b"inherited-put".to_vec(),
    )?;
    let shadow_delete = storage_row_with(
        near_source,
        b"model-inherit-shadow-delete".to_vec(),
        2,
        20,
        Timestamp::EPOCH,
        b"inherited-delete".to_vec(),
    )?;
    let scan_row = storage_row_with(
        far_source,
        b"model-inherit-scan-b".to_vec(),
        4,
        40,
        Timestamp::EPOCH,
        vec![script_byte(script, 18)],
    )?;
    let expiring = storage_row_with(
        near_source,
        b"model-inherit-expiring".to_vec(),
        4,
        40,
        Timestamp::from_micros(45),
        vec![script_byte(script, 19)],
    )?;
    let far_layer_rows = vec![shared_far, scan_row];
    let near_layer_rows = vec![shared_near, post_fork, shadow_put, shadow_delete, expiring];
    let far_layer = branch_inherited_layer_unchecked_for_fork_gate_checks(
        far_source,
        CommitVersion::new(4),
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            far_source,
            BranchLevel::ZERO,
            "model-inherit-far",
            far_layer_rows.clone(),
        )?]],
    );
    let near_layer = branch_inherited_layer_unchecked_for_fork_gate_checks(
        near_source,
        CommitVersion::new(5),
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            near_source,
            BranchLevel::ZERO,
            "model-inherit-near",
            near_layer_rows.clone(),
        )?]],
    );
    let mut state = BranchLocalState::empty(child);
    state
        .attach_inherited_layers(vec![near_layer, far_layer])
        .map_err(|err| TestkitError::new(format!("model inheritance attach failed: {err}")))?;
    let child_shadow = storage_row_with(
        child,
        b"model-inherit-shadow-put".to_vec(),
        6,
        60,
        Timestamp::EPOCH,
        b"child-put".to_vec(),
    )?;
    let child_tombstone = tombstone_row(child, b"model-inherit-shadow-delete".to_vec(), 6, 60)?;
    for row in [child_shadow.clone(), child_tombstone.clone()] {
        state
            .append_committed_row(row)
            .map_err(|err| TestkitError::new(format!("model inheritance append failed: {err}")))?;
    }

    let mut model = ModelBranchStore::default();
    model.add_branch(child);
    model.attach_layer(
        child,
        ModelInheritedLayer::new(near_source, CommitVersion::new(5), near_layer_rows),
    )?;
    model.attach_layer(
        child,
        ModelInheritedLayer::new(far_source, CommitVersion::new(4), far_layer_rows),
    )?;
    model.push_own(child_shadow)?;
    model.push_own(child_tombstone)?;

    let view = state
        .capture_read_view()
        .map_err(|err| TestkitError::new(format!("model inheritance view failed: {err}")))?
        .with_timestamp_coverage(BranchTimestampCoverage::complete());
    assert_model_store_read_surface(
        &view,
        &model,
        child,
        &[
            b"model-inherit-shared".as_slice(),
            b"model-inherit-post-fork".as_slice(),
            b"model-inherit-shadow-put".as_slice(),
            b"model-inherit-shadow-delete".as_slice(),
            b"model-inherit-expiring".as_slice(),
            b"model-inherit-scan-b".as_slice(),
        ],
        b"model-inherit-",
        Some((
            b"model-inherit-expiring".as_slice(),
            b"model-inherit-shadow-put".as_slice(),
        )),
    )
}

#[allow(clippy::too_many_lines)]
fn check_model_backed_materialization(script: &[u8]) -> Result<(), TestkitError> {
    let source = branch_id(0xb1);
    let child = branch_id(0xb2);
    let visible = storage_row_with(
        source,
        b"model-materialize-a".to_vec(),
        2,
        20,
        Timestamp::EPOCH,
        vec![script_byte(script, 32)],
    )?;
    let historical = storage_row_with(
        source,
        b"model-materialize-history".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        vec![script_byte(script, 33)],
    )?;
    let deleted_put = storage_row_with(
        source,
        b"model-materialize-deleted".to_vec(),
        2,
        20,
        Timestamp::EPOCH,
        b"deleted-put".to_vec(),
    )?;
    let expiring = storage_row_with(
        source,
        b"model-materialize-expiring".to_vec(),
        3,
        30,
        Timestamp::from_micros(35),
        vec![script_byte(script, 34)],
    )?;
    let post_fork = storage_row_with(
        source,
        b"model-materialize-post-fork".to_vec(),
        9,
        90,
        Timestamp::EPOCH,
        b"post".to_vec(),
    )?;
    let layer_rows = vec![
        visible,
        historical.clone(),
        deleted_put,
        expiring,
        post_fork,
    ];
    let layer = branch_inherited_layer_unchecked_for_fork_gate_checks(
        source,
        CommitVersion::new(5),
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            source,
            BranchLevel::ZERO,
            "model-materialize-source",
            layer_rows.clone(),
        )?]],
    );
    let mut state = BranchLocalState::empty(child);
    state
        .attach_inherited_layers(vec![layer])
        .map_err(|err| TestkitError::new(format!("model materialization attach failed: {err}")))?;
    let child_newer = storage_row_with(
        child,
        b"model-materialize-history".to_vec(),
        6,
        60,
        Timestamp::EPOCH,
        b"child-newer".to_vec(),
    )?;
    let child_tombstone = tombstone_row(child, b"model-materialize-deleted".to_vec(), 7, 70)?;
    for row in [child_newer.clone(), child_tombstone.clone()] {
        state.append_committed_row(row).map_err(|err| {
            TestkitError::new(format!("model materialization append failed: {err}"))
        })?;
    }

    let mut model = ModelBranchStore::default();
    model.add_branch(child);
    model.attach_layer(
        child,
        ModelInheritedLayer::new(source, CommitVersion::new(5), layer_rows),
    )?;
    model.push_own(child_newer)?;
    model.push_own(child_tombstone)?;
    let before = state
        .capture_read_view()
        .map_err(|err| {
            TestkitError::new(format!("model materialization before view failed: {err}"))
        })?
        .with_timestamp_coverage(BranchTimestampCoverage::complete());
    assert_model_store_read_surface(
        &before,
        &model,
        child,
        &[
            b"model-materialize-a".as_slice(),
            b"model-materialize-history".as_slice(),
            b"model-materialize-deleted".as_slice(),
            b"model-materialize-expiring".as_slice(),
            b"model-materialize-post-fork".as_slice(),
        ],
        b"model-materialize-",
        Some((
            b"model-materialize-a".as_slice(),
            b"model-materialize-history".as_slice(),
        )),
    )?;

    state
        .materialize_inherited_layer(
            &BranchMaterializationRequest::new(child, 0, "model-materialize").map_err(|err| {
                TestkitError::new(format!("model materialization request failed: {err}"))
            })?,
        )
        .map_err(|err| TestkitError::new(format!("model materialization failed: {err}")))?;
    let after = state
        .capture_read_view()
        .map_err(|err| {
            TestkitError::new(format!("model materialization after view failed: {err}"))
        })?
        .with_timestamp_coverage(BranchTimestampCoverage::complete());
    assert_model_store_read_surface(
        &after,
        &model,
        child,
        &[
            b"model-materialize-a".as_slice(),
            b"model-materialize-history".as_slice(),
            b"model-materialize-deleted".as_slice(),
            b"model-materialize-expiring".as_slice(),
            b"model-materialize-post-fork".as_slice(),
        ],
        b"model-materialize-",
        Some((
            b"model-materialize-a".as_slice(),
            b"model-materialize-history".as_slice(),
        )),
    )
}

#[allow(clippy::too_many_lines)]
fn check_model_backed_install_script(script: &[u8]) -> Result<(), TestkitError> {
    let first = branch_id(0x94);
    let second = branch_id(0x95);
    let compact_branch = branch_id(0x96);
    let mut first_rows = Vec::new();
    let mut second_rows = Vec::new();
    let mut compact_even_rows = Vec::new();
    let mut compact_odd_rows = Vec::new();

    for step in 0..12_u64 {
        let opcode = script_byte(
            script,
            128 + usize::try_from(step).expect("step fits usize"),
        );
        let key = format!("model-install-key-{:02}", opcode % 5).into_bytes();
        let timestamp = 20 + step;
        let snapshot_row = if opcode % 4 == 0 {
            tombstone_row(first, key.clone(), 1 + step, timestamp)?
        } else if opcode % 2 == 0 {
            storage_row_with(
                first,
                key.clone(),
                1 + step,
                timestamp,
                Timestamp::from_micros(timestamp + 5),
                vec![opcode],
            )?
        } else {
            storage_row_with(
                second,
                key.clone(),
                1 + step,
                timestamp,
                Timestamp::EPOCH,
                vec![opcode],
            )?
        };
        if snapshot_row.physical_key().branch_id() == first {
            first_rows.push(snapshot_row);
        } else {
            second_rows.push(snapshot_row);
        }

        let compact_row = if opcode % 5 == 0 {
            tombstone_row(compact_branch, key, 30 + step, 130 + step)?
        } else {
            storage_row_with(
                compact_branch,
                key,
                30 + step,
                130 + step,
                Timestamp::EPOCH,
                vec![opcode],
            )?
        };
        if step % 2 == 0 {
            compact_even_rows.push(compact_row);
        } else {
            compact_odd_rows.push(compact_row);
        }
    }

    if first_rows.is_empty() {
        first_rows.push(storage_row_with(
            first,
            b"model-install-key-00".to_vec(),
            1,
            20,
            Timestamp::EPOCH,
            b"first".to_vec(),
        )?);
    }
    if second_rows.is_empty() {
        second_rows.push(storage_row_with(
            second,
            b"model-install-key-01".to_vec(),
            2,
            21,
            Timestamp::EPOCH,
            b"second".to_vec(),
        )?);
    }

    let first_rows = sorted_snapshot_rows(first_rows);
    let second_rows = sorted_snapshot_rows(second_rows);
    let request = BranchSnapshotInstallRequest::new(
        "model-install-script",
        vec![
            BranchSnapshotInstallGroup::new(first, first_rows.clone()),
            BranchSnapshotInstallGroup::new(second, second_rows.clone()),
        ],
    )
    .map_err(|err| TestkitError::new(format!("model install script request failed: {err}")))?
    .with_missing_branch_policy(BranchSnapshotMissingBranchPolicy::Create {
        config: BranchRuntimeConfig::default(),
    });
    let mut branches = Vec::new();
    install_snapshot_rows_into_branches(&mut branches, &request)
        .map_err(|err| TestkitError::new(format!("model install script snapshot failed: {err}")))?;

    let mut model = ModelBranchStore::default();
    for (branch, rows) in [(first, first_rows), (second, second_rows)] {
        model.add_branch(branch);
        for row in rows {
            model.push_own(row)?;
        }
        let view = branch_state_by_id(&branches, branch)?
            .capture_read_view()
            .map_err(|err| TestkitError::new(format!("model install script view failed: {err}")))?
            .with_timestamp_coverage(BranchTimestampCoverage::complete());
        assert_model_store_read_surface(
            &view,
            &model,
            branch,
            &[
                b"model-install-key-00".as_slice(),
                b"model-install-key-01".as_slice(),
                b"model-install-key-02".as_slice(),
                b"model-install-key-03".as_slice(),
                b"model-install-key-04".as_slice(),
            ],
            b"model-install-key-",
            Some((b"model-install-key-00", b"model-install-key-04")),
        )?;
    }

    if compact_even_rows.is_empty() {
        compact_even_rows.push(storage_row_with(
            compact_branch,
            b"model-install-key-00".to_vec(),
            30,
            130,
            Timestamp::EPOCH,
            b"compact-even".to_vec(),
        )?);
    }
    if compact_odd_rows.is_empty() {
        compact_odd_rows.push(storage_row_with(
            compact_branch,
            b"model-install-key-01".to_vec(),
            31,
            131,
            Timestamp::EPOCH,
            b"compact-odd".to_vec(),
        )?);
    }
    let compact_rows = compact_even_rows
        .iter()
        .chain(compact_odd_rows.iter())
        .cloned()
        .collect::<Vec<_>>();
    let mut compact_state = compaction_state_with_l0_tables(
        compact_branch,
        "model-install-compact",
        vec![compact_even_rows, compact_odd_rows],
    )?;
    let mut compact_model = ModelBranchStore::default();
    compact_model.add_branch(compact_branch);
    for row in compact_rows {
        compact_model.push_own(row)?;
    }
    compact_state
        .compact_branch_owned_tables(&branch_compaction_request(
            compact_branch,
            BranchCompactionKind::CompactL0,
            "model-install-compact-output",
        )?)
        .map_err(|err| {
            TestkitError::new(format!("model install script compaction failed: {err}"))
        })?;
    let compact_view = compact_state.capture_read_view().map_err(|err| {
        TestkitError::new(format!("model install script compact view failed: {err}"))
    })?;
    assert_model_store_read_surface(
        &compact_view,
        &compact_model,
        compact_branch,
        &[
            b"model-install-key-00".as_slice(),
            b"model-install-key-01".as_slice(),
            b"model-install-key-02".as_slice(),
            b"model-install-key-03".as_slice(),
            b"model-install-key-04".as_slice(),
        ],
        b"model-install-key-",
        Some((b"model-install-key-00", b"model-install-key-04")),
    )
}

fn check_model_backed_snapshot_install(script: &[u8]) -> Result<(), TestkitError> {
    let first = branch_id(0xc1);
    let second = branch_id(0xc2);
    let first_rows = sorted_snapshot_rows(vec![
        storage_row_with(
            first,
            b"model-snapshot-a".to_vec(),
            2,
            20,
            Timestamp::EPOCH,
            vec![script_byte(script, 48)],
        )?,
        tombstone_row(first, b"model-snapshot-deleted".to_vec(), 3, 30)?,
    ]);
    let second_rows = sorted_snapshot_rows(vec![
        storage_row_with(
            second,
            b"model-snapshot-b".to_vec(),
            4,
            40,
            Timestamp::EPOCH,
            vec![script_byte(script, 49)],
        )?,
        storage_row_with(
            second,
            b"model-snapshot-expiring".to_vec(),
            5,
            50,
            Timestamp::from_micros(55),
            vec![script_byte(script, 50)],
        )?,
    ]);
    let request = BranchSnapshotInstallRequest::new(
        "model-snapshot",
        vec![
            BranchSnapshotInstallGroup::new(first, first_rows.clone()),
            BranchSnapshotInstallGroup::new(second, second_rows.clone()),
        ],
    )
    .map_err(|err| TestkitError::new(format!("model snapshot request failed: {err}")))?
    .with_missing_branch_policy(BranchSnapshotMissingBranchPolicy::Create {
        config: BranchRuntimeConfig::default(),
    });
    let mut branches = Vec::new();
    install_snapshot_rows_into_branches(&mut branches, &request)
        .map_err(|err| TestkitError::new(format!("model snapshot install failed: {err}")))?;
    let mut model = ModelBranchStore::default();
    for (branch, rows) in [(first, first_rows), (second, second_rows)] {
        model.add_branch(branch);
        for row in rows {
            model.push_own(row)?;
        }
        let view = branch_state_by_id(&branches, branch)?
            .capture_read_view()
            .map_err(|err| TestkitError::new(format!("model snapshot view failed: {err}")))?
            .with_timestamp_coverage(BranchTimestampCoverage::complete());
        assert_model_store_read_surface(
            &view,
            &model,
            branch,
            &[
                b"model-snapshot-a".as_slice(),
                b"model-snapshot-b".as_slice(),
                b"model-snapshot-deleted".as_slice(),
                b"model-snapshot-expiring".as_slice(),
            ],
            b"model-snapshot-",
            Some((b"model-snapshot-a", b"model-snapshot-expiring")),
        )?;
    }
    Ok(())
}

fn check_model_backed_compaction_install(script: &[u8]) -> Result<(), TestkitError> {
    let branch = branch_id(0xd1);
    let rows = vec![
        storage_row_with(
            branch,
            b"model-compact-live".to_vec(),
            6,
            60,
            Timestamp::EPOCH,
            vec![script_byte(script, 64)],
        )?,
        storage_row_with(
            branch,
            b"model-compact-live".to_vec(),
            2,
            20,
            Timestamp::EPOCH,
            b"old".to_vec(),
        )?,
        tombstone_row(branch, b"model-compact-deleted".to_vec(), 7, 70)?,
        storage_row_with(
            branch,
            b"model-compact-scan".to_vec(),
            4,
            40,
            Timestamp::EPOCH,
            vec![script_byte(script, 65)],
        )?,
    ];
    let mut state = compaction_state_with_l0_tables(
        branch,
        "model-compact",
        vec![
            vec![rows[0].clone(), rows[2].clone()],
            vec![rows[1].clone(), rows[3].clone()],
        ],
    )?;
    let mut model = ModelBranchStore::default();
    model.add_branch(branch);
    for row in rows {
        model.push_own(row)?;
    }
    let before = state
        .capture_read_view()
        .map_err(|err| TestkitError::new(format!("model compaction before view failed: {err}")))?;
    assert_model_store_read_surface(
        &before,
        &model,
        branch,
        &[
            b"model-compact-live".as_slice(),
            b"model-compact-deleted".as_slice(),
            b"model-compact-scan".as_slice(),
        ],
        b"model-compact-",
        Some((b"model-compact-deleted", b"model-compact-scan")),
    )?;
    state
        .compact_branch_owned_tables(&branch_compaction_request(
            branch,
            BranchCompactionKind::CompactL0,
            "model-compact-output",
        )?)
        .map_err(|err| TestkitError::new(format!("model compaction install failed: {err}")))?;
    let after = state
        .capture_read_view()
        .map_err(|err| TestkitError::new(format!("model compaction after view failed: {err}")))?;
    assert_model_store_read_surface(
        &after,
        &model,
        branch,
        &[
            b"model-compact-live".as_slice(),
            b"model-compact-deleted".as_slice(),
            b"model-compact-scan".as_slice(),
        ],
        b"model-compact-",
        Some((b"model-compact-deleted", b"model-compact-scan")),
    )
}
