//! SpaceIndex: Space lifecycle management within branches
//!
//! SpaceIndex tracks which spaces exist within each branch. Spaces are
//! organizational namespaces that provide data isolation within a branch.
//!
//! ## Methods
//!
//! - `register(branch_id, space)` - Idempotent registration of a space
//! - `exists(branch_id, space)` - Check if a space exists ("default" always true)
//! - `list(branch_id)` - List all spaces (always includes "default")
//! - `delete(branch_id, space)` - Delete space metadata key only
//! - `is_empty(branch_id, space)` - Check if a space has any data
//!
//! ## Key Design
//!
//! - Space metadata is stored in the branch-level namespace (not space-scoped)
//!   to avoid circular dependency.
//! - Key format: `<branch_namespace>:<TypeTag::Space>:<space_name>`

use crate::database::Database;
use crate::{StrataError, StrataResult};
use std::sync::Arc;
use strata_core::BranchId;
use strata_core::EntityRef;
use strata_core::Value;
use strata_storage::{Key, Namespace, TypeTag};
use tracing::info;

/// Validate a user-visible space name.
///
/// This is the engine-owned validation boundary for product callers. The
/// underlying naming rules still mirror storage's physical namespace rules, but
/// callers above engine should receive a normal [`StrataError::InvalidInput`]
/// instead of a storage-local error/string.
pub fn validate_space_name(name: &str) -> StrataResult<()> {
    strata_storage::validate_space_name(name).map_err(StrataError::invalid_input)
}

/// Space lifecycle management primitive.
///
/// SpaceIndex is a stateless facade over the Database engine, holding only
/// an `Arc<Database>` reference. It tracks which spaces exist within each
/// branch and can check whether a space contains any data.
#[derive(Clone)]
pub struct SpaceIndex {
    db: Arc<Database>,
}

impl SpaceIndex {
    /// Create a new SpaceIndex instance.
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    /// Idempotent registration of a space.
    ///
    /// Writes a metadata key for the space if it doesn't already exist.
    /// Safe to call repeatedly — only the first call writes.
    pub fn register(&self, branch_id: BranchId, space: &str) -> StrataResult<()> {
        self.db.transaction(branch_id, |txn| {
            let key = Key::new_space(branch_id, space);
            if txn.get(&key)?.is_none() {
                txn.put(key, Value::String("{}".to_string()))?;
                info!(target: "strata::space", space, branch_id = %branch_id, "Space registered");
            }
            Ok(())
        })
    }

    /// Check if a space exists in the branch.
    ///
    /// Returns `true` for the implicit `default` and `_system_` spaces
    /// without hitting storage. Otherwise checks the metadata index first
    /// (cheap) and falls back to a data scan so that legacy or
    /// bypass-written spaces are still discoverable.
    pub fn exists(&self, branch_id: BranchId, space: &str) -> StrataResult<bool> {
        if space == "default" || space == crate::system_space::SYSTEM_SPACE {
            return Ok(true);
        }
        if self.exists_metadata(branch_id, space)? {
            return Ok(true);
        }
        self.has_any_data(branch_id, space)
    }

    /// List all spaces in a branch.
    ///
    /// Returns the union of metadata-registered spaces and spaces
    /// discovered by scanning the data layer. Always includes `default`.
    /// `_system_` is excluded.
    ///
    /// This is administrative — not a hot path. Cost is dominated by
    /// `discover_used_spaces`, which walks every user-data type tag once.
    pub fn list(&self, branch_id: BranchId) -> StrataResult<Vec<String>> {
        use std::collections::BTreeSet;

        let metadata_spaces: BTreeSet<String> =
            self.list_metadata(branch_id)?.into_iter().collect();
        let discovered_spaces: BTreeSet<String> =
            self.discover_used_spaces(branch_id)?.into_iter().collect();

        Ok(metadata_spaces.union(&discovered_spaces).cloned().collect())
    }

    /// Delete the space metadata key only.
    ///
    /// The caller is responsible for cleaning up data in the space before
    /// calling this method.
    pub fn delete(&self, branch_id: BranchId, space: &str) -> StrataResult<()> {
        self.db.transaction(branch_id, |txn| {
            let key = Key::new_space(branch_id, space);
            txn.delete(key)?;
            info!(target: "strata::space", space, branch_id = %branch_id, "Space deleted");
            Ok(())
        })
    }

    /// Delete a user-visible space and all primitive data owned by it.
    ///
    /// This is the product boundary for space deletion: callers above engine
    /// provide the branch name, space name, and force policy, while engine owns
    /// branch validation, storage row cleanup, primitive runtime cleanup, search
    /// index cleanup, and metadata removal.
    ///
    /// This method only runs engine-owned cleanup. Product layers with
    /// additional feature-local state, such as intelligence shadow embeddings,
    /// must call [`Self::delete_user_space_with_post_data_cleanup`] instead.
    pub fn delete_user_space(
        &self,
        branch_name: &str,
        space: &str,
        force: bool,
    ) -> StrataResult<()> {
        self.delete_user_space_with_post_data_cleanup(branch_name, space, force, |_, _| Ok(()))
    }

    /// Delete a user-visible space, invoking `post_data_cleanup` after user
    /// data/search/vector cleanup and before metadata deletion.
    ///
    /// This preserves the engine-owned storage boundary while letting product
    /// layers drain feature-local runtime state before the space metadata
    /// disappears. The hook is fallible so product-layer cleanup failures do
    /// not get silently hidden behind a successful metadata delete.
    pub fn delete_user_space_with_post_data_cleanup<F>(
        &self,
        branch_name: &str,
        space: &str,
        force: bool,
        post_data_cleanup: F,
    ) -> StrataResult<()>
    where
        F: FnOnce(BranchId, &str) -> StrataResult<()>,
    {
        let branch_id = BranchId::from_user_name(branch_name);

        if space == "default" || space == crate::system_space::SYSTEM_SPACE {
            return Err(protected_space_delete_constraint(branch_id, space));
        }

        validate_space_name(space)?;
        self.require_branch_exists_for_delete(branch_name)?;
        let branch_id = product_branch_id(branch_name)?;

        if !force && !self.is_empty(branch_id, space)? {
            return Err(non_empty_space_delete_constraint(branch_id, space));
        }

        self.delete_space_data_rows(branch_id, space)?;
        self.remove_search_documents_in_space(branch_id, space);
        self.purge_vector_collections_in_space(branch_id, space);
        post_data_cleanup(branch_id, space)?;
        self.delete(branch_id, space)
    }

    /// Check if a space has any data.
    ///
    /// **Note:** This is O(N) — scans all data TypeTags (KV, Event,
    /// Json, Vector, Graph) in the space's namespace. Not a cheap check.
    pub fn is_empty(&self, branch_id: BranchId, space: &str) -> StrataResult<bool> {
        self.db.transaction(branch_id, |txn| {
            let ns = Arc::new(Namespace::for_branch_space(branch_id, space));

            for type_tag in [
                TypeTag::KV,
                TypeTag::Event,
                TypeTag::Json,
                TypeTag::Vector,
                TypeTag::Graph,
            ] {
                let prefix = Key::new(ns.clone(), type_tag, vec![]);
                let entries = txn.scan_prefix(&prefix)?;
                if !entries.is_empty() {
                    return Ok(false);
                }
            }

            Ok(true)
        })
    }

    /// Discover spaces that contain real user data, regardless of whether
    /// they were ever registered through `register()`.
    ///
    /// Iterates the storage layer for each user-data type tag (KV, Event,
    /// Json, Vector, Graph) at the current MVCC version, collecting the
    /// distinct `space` field of every live (non-tombstone) entry. The
    /// `_system_` shadow space is excluded; `default` is always included.
    ///
    /// Cost: O(branch size) over five iterations. This is an administrative
    /// operation — it powers `SPACE LIST`, branch ops, and startup repair.
    /// No hot path uses it.
    pub fn discover_used_spaces(&self, branch_id: BranchId) -> StrataResult<Vec<String>> {
        use std::collections::BTreeSet;

        let mut spaces: BTreeSet<String> = BTreeSet::new();
        spaces.insert("default".to_string());

        let snapshot = strata_core::id::CommitVersion(self.db.storage().version());
        for type_tag in [
            TypeTag::KV,
            TypeTag::Event,
            TypeTag::Json,
            TypeTag::Vector,
            TypeTag::Graph,
        ] {
            for entry in self
                .db
                .storage()
                .list_by_type_at_version(&branch_id, type_tag, snapshot)
            {
                if entry.is_tombstone {
                    continue;
                }
                let space = entry.key.namespace.space.as_str();
                if space == crate::system_space::SYSTEM_SPACE {
                    continue;
                }
                spaces.insert(space.to_string());
            }
        }
        Ok(spaces.into_iter().collect())
    }

    /// Reconcile metadata with real data: register every space that has
    /// data but no metadata entry. Returns the count of spaces newly
    /// registered. Idempotent — a second call after the first returns 0.
    ///
    /// Called during database startup so that downstream recovery
    /// participants and administrative consumers see a complete set of
    /// spaces, even on legacy databases or after bypass writes.
    pub fn repair_space_metadata(&self, branch_id: BranchId) -> StrataResult<usize> {
        let discovered = self.discover_used_spaces(branch_id)?;
        let mut repaired = 0usize;
        for space in &discovered {
            if space == "default" || space == crate::system_space::SYSTEM_SPACE {
                continue;
            }
            if !self.exists_metadata(branch_id, space)? {
                self.register(branch_id, space)?;
                repaired += 1;
            }
        }
        Ok(repaired)
    }

    /// Internal: metadata-only existence check.
    ///
    /// Used by `exists()` as a fast path before falling back to a data scan,
    /// and by `repair_space_metadata()` to avoid double-writes.
    fn exists_metadata(&self, branch_id: BranchId, space: &str) -> StrataResult<bool> {
        self.db.transaction(branch_id, |txn| {
            let key = Key::new_space(branch_id, space);
            Ok(txn.get(&key)?.is_some())
        })
    }

    /// Internal: metadata-only list of registered spaces (always includes
    /// "default"). Used by `list()` as one half of the union.
    fn list_metadata(&self, branch_id: BranchId) -> StrataResult<Vec<String>> {
        self.db.transaction(branch_id, |txn| {
            let prefix = Key::new_space_prefix(branch_id);
            let results = txn.scan_prefix(&prefix)?;

            let mut spaces: Vec<String> = results
                .into_iter()
                .filter_map(|(k, _)| String::from_utf8(k.user_key.to_vec()).ok())
                .filter(|s| s != crate::system_space::SYSTEM_SPACE)
                .collect();

            if !spaces.contains(&"default".to_string()) {
                spaces.insert(0, "default".to_string());
            }

            Ok(spaces)
        })
    }

    /// Internal: returns true if any user-data entry exists for this
    /// `(branch, space)`. Used by `exists()` as the fallback when metadata
    /// doesn't have the space registered.
    ///
    /// Equivalent to `!is_empty(branch_id, space)?`, but expressed
    /// directly so the call site is self-documenting.
    fn has_any_data(&self, branch_id: BranchId, space: &str) -> StrataResult<bool> {
        Ok(!self.is_empty(branch_id, space)?)
    }

    fn require_branch_exists_for_delete(&self, branch_name: &str) -> StrataResult<()> {
        let default_branch = self
            .db
            .default_branch_name()
            .unwrap_or_else(|| "default".to_string());
        if default_branch == branch_name {
            return Ok(());
        }

        if self.db.branches().exists(branch_name)? {
            Ok(())
        } else {
            Err(StrataError::branch_not_found_by_name(branch_name))
        }
    }

    fn delete_space_data_rows(&self, branch_id: BranchId, space: &str) -> StrataResult<()> {
        let vector = crate::vector::VectorStore::new(self.db.clone());
        let namespace = Arc::new(Namespace::for_branch_space(branch_id, space));

        self.db.transaction(branch_id, |txn| {
            vector
                .delete_space_data_in_transaction(txn, branch_id, space)
                .map_err(|error| error.into_strata_error(branch_id))?;

            for type_tag in [TypeTag::KV, TypeTag::Event, TypeTag::Json, TypeTag::Graph] {
                let prefix = Key::new(namespace.clone(), type_tag, Vec::new());
                let entries = txn.scan_prefix(&prefix)?;
                for (key, _) in entries {
                    txn.delete(key)?;
                }
            }
            Ok(())
        })
    }

    fn remove_search_documents_in_space(&self, branch_id: BranchId, space: &str) {
        if let Ok(index) = self.db.extension::<crate::search::InvertedIndex>() {
            let removed = index.remove_documents_in_space(branch_id, space);
            if removed > 0 {
                tracing::info!(
                    target: "strata::space",
                    branch_id = %branch_id,
                    space = space,
                    removed,
                    "Removed search documents during space delete"
                );
            }
        }
    }

    fn purge_vector_collections_in_space(&self, branch_id: BranchId, space: &str) {
        let vector = crate::vector::VectorStore::new(self.db.clone());
        if let Err(error) = vector.purge_collections_in_space(branch_id, space) {
            tracing::warn!(
                target: "strata::space",
                branch_id = %branch_id,
                space = space,
                error = %error,
                "Failed to purge vector collections during space delete"
            );
        }
    }
}

fn product_branch_id(branch_name: &str) -> StrataResult<BranchId> {
    if crate::branch_domain::aliases_default_branch_sentinel(branch_name) {
        return Err(StrataError::invalid_input(
            "branch name aliases reserved default-branch sentinel",
        ));
    }

    Ok(BranchId::from_user_name(branch_name))
}

fn space_delete_constraint(branch_id: BranchId, reason: String) -> StrataError {
    StrataError::invalid_operation(EntityRef::Branch { branch_id }, reason)
}

fn protected_space_delete_reason(space: &str) -> String {
    format!("Cannot delete the '{space}' space")
}

fn non_empty_space_delete_reason(space: &str) -> String {
    format!("Space '{space}' is not empty. Use force=true to delete anyway.")
}

fn protected_space_delete_constraint(branch_id: BranchId, space: &str) -> StrataError {
    space_delete_constraint(branch_id, protected_space_delete_reason(space))
}

fn non_empty_space_delete_constraint(branch_id: BranchId, space: &str) -> StrataError {
    space_delete_constraint(branch_id, non_empty_space_delete_reason(space))
}

/// Return true when an engine error should surface as a product-level space
/// delete constraint violation above engine.
///
/// This keeps executor and future product facades from depending directly on
/// engine's user-facing error strings. `StrataError` does not currently carry
/// typed invalid-operation reasons, so the string classification remains
/// intentionally local to the primitive that produces these errors.
pub fn is_user_space_delete_constraint(error: &StrataError) -> bool {
    let StrataError::InvalidOperation { reason, .. } = error else {
        return false;
    };

    (reason.starts_with("Cannot delete the '") && reason.ends_with("' space"))
        || (reason.starts_with("Space '")
            && reason.ends_with("' is not empty. Use force=true to delete anyway."))
}

/// Atomically ensure a space's metadata key exists within an open
/// transaction.
///
/// Idempotent: writes the metadata key only if it is missing. Skips the
/// implicit `default` and `_system_` spaces. Callers invoke this from
/// inside the same `db.transaction(...)` that performs the data write so
/// that registration is atomic with the user write — no partial state.
///
/// This is the canonical helper used by `EventLog::append`,
/// `KVStore::put`, `JsonStore::create/set`,
/// `VectorStore::create_collection/insert_inner`, and graph mutate paths.
/// Centralising it here ensures every primitive uses the same skip rules
/// and the same key encoding.
pub fn ensure_space_registered_in_txn(
    txn: &mut strata_storage::TransactionContext,
    branch_id: &BranchId,
    space: &str,
) -> StrataResult<()> {
    if space == "default" || space == crate::system_space::SYSTEM_SPACE {
        return Ok(());
    }
    let key = Key::new_space(*branch_id, space);
    if txn.get(&key)?.is_none() {
        txn.put(key, Value::String("{}".to_string()))?;
    }
    Ok(())
}

// ========== Tests ==========

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::types::NodeData;
    use crate::graph::GraphStore;
    use crate::primitives::event::EventLog;
    use crate::primitives::json::JsonStore;
    use crate::primitives::KVStore;
    use crate::semantics::json::{JsonPath, JsonValue};
    use crate::semantics::vector::{DistanceMetric, VectorConfig};
    use crate::vector::VectorStore;
    use std::collections::HashMap;
    use tempfile::TempDir;

    fn setup() -> (TempDir, Arc<Database>, SpaceIndex) {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path()).unwrap();
        let si = SpaceIndex::new(db.clone());
        (temp_dir, db, si)
    }

    fn default_branch() -> BranchId {
        BranchId::from_bytes([0u8; 16])
    }

    #[test]
    fn validate_space_name_accepts_product_space_names() {
        validate_space_name("default").unwrap();
        validate_space_name("tenant_a-1").unwrap();
    }

    #[test]
    fn validate_space_name_returns_engine_invalid_input() {
        let err = validate_space_name("not allowed").unwrap_err();

        assert!(matches!(
            err,
            StrataError::InvalidInput { ref message }
                if message == "Space name can only contain lowercase letters, digits, hyphens, and underscores"
        ));
    }

    #[test]
    fn test_register_and_exists() {
        let (_temp, _db, si) = setup();
        let bid = default_branch();

        assert!(!si.exists(bid, "alpha").unwrap());
        si.register(bid, "alpha").unwrap();
        assert!(si.exists(bid, "alpha").unwrap());
    }

    #[test]
    fn test_list_includes_default() {
        let (_temp, _db, si) = setup();
        let bid = default_branch();

        let spaces = si.list(bid).unwrap();
        assert!(spaces.contains(&"default".to_string()));
    }

    #[test]
    fn test_register_idempotent() {
        let (_temp, _db, si) = setup();
        let bid = default_branch();

        si.register(bid, "alpha").unwrap();
        si.register(bid, "alpha").unwrap();

        let spaces = si.list(bid).unwrap();
        let alpha_count = spaces.iter().filter(|s| *s == "alpha").count();
        assert_eq!(alpha_count, 1);
    }

    #[test]
    fn test_is_empty_true_for_new_space() {
        let (_temp, _db, si) = setup();
        let bid = default_branch();

        si.register(bid, "alpha").unwrap();
        assert!(si.is_empty(bid, "alpha").unwrap());
    }

    #[test]
    fn test_is_empty_false_after_write() {
        let (_temp, db, si) = setup();
        let bid = default_branch();

        si.register(bid, "alpha").unwrap();

        // Write KV data into the "alpha" space
        let kv = KVStore::new(db.clone());
        kv.put(&bid, "alpha", "test-key", Value::Int(42)).unwrap();

        assert!(!si.is_empty(bid, "alpha").unwrap());
    }

    #[test]
    fn test_delete_metadata() {
        let (_temp, _db, si) = setup();
        let bid = default_branch();

        si.register(bid, "alpha").unwrap();
        assert!(si.exists(bid, "alpha").unwrap());

        si.delete(bid, "alpha").unwrap();
        assert!(!si.exists(bid, "alpha").unwrap());
    }

    #[test]
    fn delete_user_space_rejects_protected_and_non_empty_spaces() {
        let (_temp, db, si) = setup();
        let bid = default_branch();
        let kv = KVStore::new(db);

        let protected = si
            .delete_user_space("default", "default", true)
            .unwrap_err();
        assert!(matches!(
            protected,
            StrataError::InvalidOperation { ref reason, .. }
                if reason == "Cannot delete the 'default' space"
        ));

        kv.put(&bid, "alpha", "key", Value::Int(1)).unwrap();

        let non_empty = si.delete_user_space("default", "alpha", false).unwrap_err();
        assert!(matches!(
            non_empty,
            StrataError::InvalidOperation { ref reason, .. }
                if reason == "Space 'alpha' is not empty. Use force=true to delete anyway."
        ));
    }

    #[test]
    fn delete_user_space_force_cleans_primitive_rows_and_runtime_side_effects() {
        let (_temp, db, si) = setup();
        let bid = default_branch();
        let kv = KVStore::new(db.clone());
        let json = JsonStore::new(db.clone());
        let events = EventLog::new(db.clone());
        let graph = GraphStore::new(db.clone());
        let vector = VectorStore::new(db.clone());
        let index = db.extension::<crate::search::InvertedIndex>().unwrap();

        kv.put(
            &bid,
            "alpha",
            "key",
            Value::String("searchable text".into()),
        )
        .unwrap();
        kv.put(&bid, "beta", "key", Value::String("sibling text".into()))
            .unwrap();
        json.create(&bid, "alpha", "doc", JsonValue::from("json alpha"))
            .unwrap();
        json.create(&bid, "beta", "doc", JsonValue::from("json beta"))
            .unwrap();
        events
            .append(
                &bid,
                "alpha",
                "space.delete.test",
                Value::object(HashMap::new()),
            )
            .unwrap();
        events
            .append(
                &bid,
                "beta",
                "space.delete.test",
                Value::object(HashMap::new()),
            )
            .unwrap();
        graph.create_graph(bid, "alpha", "links", None).unwrap();
        graph
            .add_node(bid, "alpha", "links", "n1", NodeData::default())
            .unwrap();
        graph.create_graph(bid, "beta", "links", None).unwrap();
        graph
            .add_node(bid, "beta", "links", "n1", NodeData::default())
            .unwrap();

        let doc = EntityRef::kv(bid, "alpha", "key");
        index.index_document(&doc, "searchable text", None);
        assert!(index.has_document(&doc));

        let config = VectorConfig::new(4, DistanceMetric::Cosine).unwrap();
        vector
            .create_collection(bid, "alpha", "embeddings", config.clone())
            .unwrap();
        vector
            .insert(
                bid,
                "alpha",
                "embeddings",
                "v1",
                &[1.0, 0.0, 0.0, 0.0],
                None,
            )
            .unwrap();
        vector
            .create_collection(bid, "beta", "embeddings", config)
            .unwrap();
        vector
            .insert(bid, "beta", "embeddings", "v1", &[0.0, 1.0, 0.0, 0.0], None)
            .unwrap();

        let hook_observed_metadata = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let hook_observed_metadata_clone = hook_observed_metadata.clone();
        let db_for_hook = db.clone();
        si.delete_user_space_with_post_data_cleanup(
            "default",
            "alpha",
            true,
            move |branch_id, space| {
                assert_eq!(space, "alpha");

                let space_index = SpaceIndex::new(db_for_hook.clone());
                hook_observed_metadata_clone.store(
                    space_index.exists(branch_id, space).unwrap(),
                    std::sync::atomic::Ordering::SeqCst,
                );

                Ok(())
            },
        )
        .unwrap();
        assert!(hook_observed_metadata.load(std::sync::atomic::Ordering::SeqCst));

        assert_eq!(kv.get(&bid, "alpha", "key").unwrap(), None);
        assert_eq!(
            kv.get(&bid, "beta", "key").unwrap(),
            Some(Value::String("sibling text".into()))
        );
        assert_eq!(
            json.get(&bid, "alpha", "doc", &JsonPath::root()).unwrap(),
            None
        );
        assert_eq!(
            json.get(&bid, "beta", "doc", &JsonPath::root()).unwrap(),
            Some(JsonValue::from("json beta"))
        );
        assert_eq!(events.len(&bid, "alpha").unwrap(), 0);
        assert_eq!(events.len(&bid, "beta").unwrap(), 1);
        assert!(graph.list_graphs(bid, "alpha").unwrap().is_empty());
        assert_eq!(graph.list_graphs(bid, "beta").unwrap(), vec!["links"]);
        assert!(!si.exists(bid, "alpha").unwrap());
        assert!(!index.has_document(&doc));
        assert!(vector.list_collections(bid, "alpha").unwrap().is_empty());
        let beta_collections: Vec<_> = vector
            .list_collections(bid, "beta")
            .unwrap()
            .into_iter()
            .map(|collection| collection.name)
            .collect();
        assert_eq!(beta_collections, vec!["embeddings"]);
    }

    #[test]
    fn delete_user_space_force_cleans_only_requested_branch() {
        let (_temp, db, si) = setup();
        let default_bid = default_branch();
        db.branches().create("feature").unwrap();
        let feature_bid = BranchId::from_user_name("feature");
        let kv = KVStore::new(db);

        kv.put(
            &default_bid,
            "alpha",
            "key",
            Value::String("default".into()),
        )
        .unwrap();
        kv.put(
            &feature_bid,
            "alpha",
            "key",
            Value::String("feature".into()),
        )
        .unwrap();

        si.delete_user_space("feature", "alpha", true).unwrap();

        assert_eq!(
            kv.get(&default_bid, "alpha", "key").unwrap(),
            Some(Value::String("default".into()))
        );
        assert_eq!(kv.get(&feature_bid, "alpha", "key").unwrap(), None);
        assert!(si.exists(default_bid, "alpha").unwrap());
        assert!(!si.exists(feature_bid, "alpha").unwrap());
    }

    #[test]
    fn delete_user_space_propagates_post_data_cleanup_failure() {
        let (_temp, db, si) = setup();
        let bid = default_branch();
        let kv = KVStore::new(db);

        kv.put(&bid, "alpha", "key", Value::Int(1)).unwrap();

        let error = si
            .delete_user_space_with_post_data_cleanup("default", "alpha", true, |_, _| {
                Err(StrataError::internal("post-data cleanup failed"))
            })
            .unwrap_err();

        assert!(matches!(
            error,
            StrataError::Internal { ref message } if message == "post-data cleanup failed"
        ));
        assert_eq!(kv.get(&bid, "alpha", "key").unwrap(), None);
        assert!(si.exists(bid, "alpha").unwrap());
    }

    #[test]
    fn user_space_delete_constraint_classifier_is_engine_owned() {
        let bid = default_branch();
        let protected = protected_space_delete_constraint(bid, "default");
        let non_empty = non_empty_space_delete_constraint(bid, "alpha");
        let unrelated = StrataError::invalid_operation(
            EntityRef::Branch { branch_id: bid },
            "some other invalid operation",
        );

        assert!(is_user_space_delete_constraint(&protected));
        assert!(is_user_space_delete_constraint(&non_empty));
        assert!(!is_user_space_delete_constraint(&unrelated));
        assert!(!is_user_space_delete_constraint(
            &StrataError::invalid_input(
                "Space 'alpha' is not empty. Use force=true to delete anyway."
            )
        ));
    }

    #[test]
    fn test_default_always_exists() {
        let (_temp, _db, si) = setup();
        let bid = default_branch();

        // "default" should always return true without explicit registration
        assert!(si.exists(bid, "default").unwrap());
    }

    #[test]
    fn test_list_with_multiple_spaces() {
        let (_temp, _db, si) = setup();
        let bid = default_branch();

        si.register(bid, "alpha").unwrap();
        si.register(bid, "beta").unwrap();
        si.register(bid, "gamma").unwrap();

        let spaces = si.list(bid).unwrap();
        assert!(spaces.contains(&"default".to_string()));
        assert!(spaces.contains(&"alpha".to_string()));
        assert!(spaces.contains(&"beta".to_string()));
        assert!(spaces.contains(&"gamma".to_string()));
    }

    #[test]
    fn test_system_space_exists() {
        let (_temp, _db, si) = setup();
        let bid = default_branch();

        // _system_ should always exist without explicit registration
        assert!(si.exists(bid, crate::system_space::SYSTEM_SPACE).unwrap());
    }

    #[test]
    fn test_system_space_hidden_from_list() {
        let (_temp, _db, si) = setup();
        let bid = default_branch();

        let spaces = si.list(bid).unwrap();
        assert!(!spaces.contains(&crate::system_space::SYSTEM_SPACE.to_string()));
    }

    #[test]
    fn test_internal_write_to_system_space() {
        let (_temp, db, _si) = setup();
        let bid = default_branch();

        // Internal code can write to _system_ via system_kv_key
        let key = crate::system_space::system_kv_key(bid, "recipe:default");
        db.transaction(bid, |txn| {
            txn.put(key.clone(), Value::String("{\"version\":1}".into()))?;
            Ok(())
        })
        .unwrap();

        // Read it back and verify content
        let val = db
            .transaction(bid, |txn| Ok(txn.get(&key)?))
            .unwrap()
            .expect("system space value should be readable");
        assert_eq!(val.as_str().unwrap(), "{\"version\":1}");
    }
}
