fn assert_model_store_read_surface(
    view: &BranchReadView,
    model: &ModelBranchStore,
    branch: BranchId,
    user_keys: &[&[u8]],
    prefix: &[u8],
    range: Option<(&[u8], &[u8])>,
) -> Result<(), TestkitError> {
    for user_key in user_keys {
        let key = physical_key(branch, (*user_key).to_vec())?;
        for bound in [
            BranchReadBound::latest(),
            BranchReadBound::at_version(CommitVersion::new(4)),
            BranchReadBound::at_timestamp(Timestamp::from_micros(44)),
        ] {
            let actual = visible_row(view, &key, bound)?;
            let expected = model.visible(branch, &key, bound)?;
            if actual != expected {
                return Err(TestkitError::new(format!(
                    "model-backed point read mismatch for {:?}",
                    key.user_key()
                )));
            }
        }
        let actual_history = view
            .history(&key, BranchHistoryOptions::all().include_tombstones(true))
            .map_err(|err| TestkitError::new(format!("model-backed history failed: {err}")))?
            .into_iter()
            .map(|row| row.row().clone())
            .collect::<Vec<_>>();
        let expected_history = model.history(branch, &key)?;
        if actual_history != expected_history {
            return Err(TestkitError::new(format!(
                "model-backed history mismatch for {:?}",
                key.user_key()
            )));
        }
    }

    let prefix_key = physical_key(branch, prefix.to_vec())?;
    let actual_prefix = view
        .scan_prefix(
            &BranchScanBounds::prefix(&prefix_key),
            BranchReadBound::latest(),
        )
        .map_err(|err| TestkitError::new(format!("model-backed prefix failed: {err}")))?
        .into_iter()
        .map(|row| row.row().clone())
        .collect::<Vec<_>>();
    let expected_prefix = model.scan_prefix(branch, prefix, BranchReadBound::latest())?;
    if actual_prefix != expected_prefix {
        return Err(TestkitError::new("model-backed prefix scan mismatch"));
    }

    if let Some((lower, upper)) = range {
        let lower_key = physical_key(branch, lower.to_vec())?;
        let upper_key = physical_key(branch, upper.to_vec())?;
        let actual_range = view
            .scan_range(
                &BranchScanBounds::closed(&lower_key, &upper_key).map_err(|err| {
                    TestkitError::new(format!("model-backed range bounds failed: {err}"))
                })?,
                BranchReadBound::latest(),
            )
            .map_err(|err| TestkitError::new(format!("model-backed range failed: {err}")))?
            .into_iter()
            .map(|row| row.row().clone())
            .collect::<Vec<_>>();
        let expected_range = model.scan_range(branch, lower, upper, BranchReadBound::latest())?;
        if actual_range != expected_range {
            return Err(TestkitError::new("model-backed range scan mismatch"));
        }
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ModelBranch {
    branch: BranchId,
    rows: Vec<StorageRow>,
}

impl ModelBranch {
    fn new(branch: BranchId) -> Self {
        Self {
            branch,
            rows: Vec::new(),
        }
    }

    fn push(&mut self, row: StorageRow) -> Result<(), TestkitError> {
        if row.physical_key().branch_id() != self.branch {
            return Err(TestkitError::new("model row branch drifted"));
        }
        if self.rows.iter().any(|existing| {
            TableInternalKeyBytes::from_row(existing) == TableInternalKeyBytes::from_row(&row)
        }) {
            return Err(TestkitError::new("model generated duplicate internal key"));
        }
        self.rows.push(row);
        Ok(())
    }

    fn visible(&self, key: &PhysicalKey, bound: BranchReadBound) -> Option<StorageRow> {
        let read_timestamp = match bound {
            BranchReadBound::Latest | BranchReadBound::AtVersion(_) => None,
            BranchReadBound::AtTimestamp(timestamp) => Some(timestamp),
        };
        self.rows
            .iter()
            .filter(|row| row.physical_key() == key)
            .filter(|row| model_row_matches_bound(row, bound))
            .max_by(|left, right| {
                left.commit_version()
                    .as_u64()
                    .cmp(&right.commit_version().as_u64())
            })
            .and_then(|row| {
                if row.is_tombstone() || model_row_is_expired_at(row, read_timestamp) {
                    None
                } else {
                    Some(row.clone())
                }
            })
    }

    fn history(&self, key: &PhysicalKey) -> Vec<StorageRow> {
        let mut rows = self
            .rows
            .iter()
            .filter(|row| row.physical_key() == key)
            .cloned()
            .collect::<Vec<_>>();
        rows.sort_by(|left, right| {
            right
                .commit_version()
                .as_u64()
                .cmp(&left.commit_version().as_u64())
        });
        rows
    }

    fn scan_visible(
        &self,
        branch: BranchId,
        bound: BranchReadBound,
    ) -> Result<Vec<StorageRow>, TestkitError> {
        let mut rows = Vec::new();
        for index in 0..MODEL_KEY_COUNT {
            let key = model_physical_key(branch, index)?;
            if let Some(row) = self.visible(&key, bound) {
                rows.push(row);
            }
        }
        rows.sort_by(|left, right| {
            left.physical_key()
                .user_key()
                .cmp(right.physical_key().user_key())
        });
        Ok(rows)
    }
}

const MODEL_KEY_COUNT: u8 = 6;

fn append_model_row(
    state: &mut BranchLocalState,
    model: &mut ModelBranch,
    row: StorageRow,
) -> Result<(), TestkitError> {
    state
        .append_committed_row(row.clone())
        .map_err(|err| TestkitError::new(format!("model append failed: {err}")))?;
    model.push(row)
}

fn install_model_l0_rows(
    state: &mut BranchLocalState,
    model: &mut ModelBranch,
    identity: &str,
    rows: Vec<StorageRow>,
) -> Result<(), TestkitError> {
    state
        .install_l0_table(branch_owned_table(
            model.branch,
            BranchLevel::ZERO,
            identity,
            rows.clone(),
        )?)
        .map_err(|err| TestkitError::new(format!("model L0 install failed: {err}")))?;
    for row in rows {
        model.push(row)?;
    }
    Ok(())
}

fn assert_model_matches_state(
    script: &[u8],
    step: usize,
    model: &ModelBranch,
    state: &BranchLocalState,
) -> Result<(), TestkitError> {
    let view = state
        .capture_read_view()
        .map_err(|err| TestkitError::new(format!("model view capture failed: {err}")))?
        .with_timestamp_coverage(BranchTimestampCoverage::complete());
    let version_bound = BranchReadBound::at_version(CommitVersion::new(1 + step as u64));
    let timestamp_bound = BranchReadBound::at_timestamp(Timestamp::from_micros(
        1 + u64::from(script_byte(script, 240 + (step % 16))) % 96,
    ));

    for key_index in 0..MODEL_KEY_COUNT {
        let key = model_physical_key(model.branch, key_index)?;
        assert_model_point(&view, model, &key, BranchReadBound::latest(), "latest")?;
        assert_model_point(&view, model, &key, version_bound, "version")?;
        assert_model_point(&view, model, &key, timestamp_bound, "timestamp")?;

        let actual_history = view
            .history(&key, BranchHistoryOptions::all())
            .map_err(|err| TestkitError::new(format!("model history failed: {err}")))?
            .into_iter()
            .map(|row| row.row().clone())
            .collect::<Vec<_>>();
        let expected_history = model.history(&key);
        if actual_history != expected_history {
            return Err(TestkitError::new("model history mismatch"));
        }
    }

    assert_model_scan(&view, model, BranchReadBound::latest(), "latest scan")?;
    assert_model_scan(&view, model, timestamp_bound, "timestamp scan")?;
    Ok(())
}

fn assert_model_point(
    view: &BranchReadView,
    model: &ModelBranch,
    key: &PhysicalKey,
    bound: BranchReadBound,
    label: &'static str,
) -> Result<(), TestkitError> {
    let actual = visible_row(view, key, bound)?;
    let expected = model.visible(key, bound);
    if actual != expected {
        return Err(TestkitError::new(format!(
            "model {label} mismatch for key {:?}",
            key.user_key()
        )));
    }
    Ok(())
}

fn assert_model_scan(
    view: &BranchReadView,
    model: &ModelBranch,
    bound: BranchReadBound,
    label: &'static str,
) -> Result<(), TestkitError> {
    let prefix = BranchScanBounds::prefix(&physical_key(model.branch, b"model-key-".to_vec())?);
    let actual = view
        .scan_prefix(&prefix, bound)
        .map_err(|err| TestkitError::new(format!("model scan failed: {err}")))?
        .into_iter()
        .map(|row| row.row().clone())
        .collect::<Vec<_>>();
    let expected = model.scan_visible(model.branch, bound)?;
    if actual != expected {
        return Err(TestkitError::new(format!("model {label} mismatch")));
    }
    Ok(())
}

fn model_row_matches_bound(row: &StorageRow, bound: BranchReadBound) -> bool {
    match bound {
        BranchReadBound::Latest => true,
        BranchReadBound::AtVersion(version) => row.commit_version().as_u64() <= version.as_u64(),
        BranchReadBound::AtTimestamp(timestamp) => {
            row.commit_timestamp().as_micros() <= timestamp.as_micros()
        }
    }
}

fn model_row_is_expired_at(row: &StorageRow, read_timestamp: Option<Timestamp>) -> bool {
    read_timestamp.is_some_and(|timestamp| {
        !row.is_tombstone() && row.expires_at() != Timestamp::EPOCH && row.expires_at() <= timestamp
    })
}

fn model_put_row(
    branch: BranchId,
    opcode: u8,
    version: u64,
    step: usize,
) -> Result<StorageRow, TestkitError> {
    storage_row_with(
        branch,
        model_user_key(opcode),
        version,
        model_timestamp(opcode, step),
        Timestamp::EPOCH,
        vec![opcode, u8::try_from(step % 251).expect("step byte")],
    )
}

fn model_expiring_row(
    branch: BranchId,
    opcode: u8,
    version: u64,
    step: usize,
) -> Result<StorageRow, TestkitError> {
    let timestamp = model_timestamp(opcode, step);
    storage_row_with(
        branch,
        model_user_key(opcode),
        version,
        timestamp,
        Timestamp::from_micros(timestamp.saturating_add(3)),
        vec![opcode, 0xee],
    )
}

fn model_tombstone_row(
    branch: BranchId,
    opcode: u8,
    version: u64,
    step: usize,
) -> Result<StorageRow, TestkitError> {
    tombstone_row(
        branch,
        model_user_key(opcode),
        version,
        model_timestamp(opcode, step),
    )
}

fn model_user_key(opcode: u8) -> Vec<u8> {
    format!("model-key-{}", opcode % MODEL_KEY_COUNT).into_bytes()
}

fn model_physical_key(branch: BranchId, key_index: u8) -> Result<PhysicalKey, TestkitError> {
    physical_key(branch, format!("model-key-{key_index}").into_bytes())
}

fn model_timestamp(opcode: u8, step: usize) -> u64 {
    1 + u64::from(opcode % 89) + u64::try_from(step % 7).expect("step fits in u64")
}
