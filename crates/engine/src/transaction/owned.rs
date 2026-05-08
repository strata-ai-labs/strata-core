//! Owned transaction handle for manual transaction lifecycle.

#[cfg(test)]
use std::ops::{Deref, DerefMut};
use std::sync::Arc;

use serde_json::Value as JsonValue;

use super::context::Transaction as ScopedTransaction;
use super::json_state::JsonTxnState;
use crate::graph::types::{EdgeData, GraphMeta, Neighbor, NodeData};
use crate::graph::{GraphStore, GraphStoreExt};
use crate::vector::{VectorRecord, VectorStore};
use crate::{StrataError, StrataResult};
use strata_core::{BranchId, Version};
use strata_storage::{Namespace, TransactionContext};

use crate::Database;

/// Owned manual transaction handle.
///
/// The handle owns the pooled `TransactionContext` and returns it to the
/// engine automatically on commit, abort, or drop.
pub struct Transaction {
    db: Arc<Database>,
    ctx: Option<TransactionContext>,
    json_state: JsonTxnState,
}

impl Transaction {
    /// Construct an owned transaction handle from an active context.
    pub(crate) fn new(db: Arc<Database>, ctx: TransactionContext) -> Self {
        Self {
            db,
            ctx: Some(ctx),
            json_state: JsonTxnState::new(),
        }
    }

    /// Commit the transaction and return its commit version.
    pub fn commit(&mut self) -> StrataResult<u64> {
        let mut ctx = self
            .ctx
            .take()
            .ok_or_else(|| StrataError::transaction_not_active("finished"))?;
        if let Err(error) = self
            .db
            .prepare_json_transaction_context(&mut ctx, &mut self.json_state)
        {
            self.db
                .abort_transaction_in_place(&mut ctx, format!("Commit failed: {}", error));
            self.db.end_transaction_context(ctx);
            return Err(error);
        }
        let result = self.db.commit_transaction_context(&mut ctx);
        self.db.end_transaction_context(ctx);
        result
    }

    /// Abort the transaction and release its pooled context.
    pub fn abort(&mut self) {
        if let Some(mut ctx) = self.ctx.take() {
            self.db
                .abort_transaction_in_place(&mut ctx, "Transaction aborted");
            self.db.end_transaction_context(ctx);
        }
    }

    /// Get the transaction's branch ID.
    pub fn branch_id(&self) -> BranchId {
        self.context()
            .expect("branch_id() requires an active transaction")
            .branch_id
    }

    /// Return the transaction ID as an engine-owned display string.
    pub fn id_string(&self) -> String {
        self.context()
            .expect("id_string() requires an active transaction")
            .txn_id
            .to_string()
    }

    fn active_context_mut(&mut self) -> StrataResult<&mut TransactionContext> {
        self.ctx
            .as_mut()
            .ok_or_else(|| StrataError::transaction_not_active("finished"))
    }

    #[cfg(test)]
    fn context_mut(&mut self) -> Option<&mut TransactionContext> {
        self.ctx.as_mut()
    }

    /// Create a scoped primitive wrapper that shares this manual transaction's
    /// engine-owned JSON state.
    pub(crate) fn scoped(&mut self, namespace: Arc<Namespace>) -> ScopedTransaction<'_> {
        let ctx = self
            .ctx
            .as_mut()
            .expect("transaction context not available after commit/abort");
        ScopedTransaction::with_json_state(
            ctx,
            namespace,
            &mut self.json_state,
            self.db.storage().clone(),
        )
    }

    /// Create a scoped primitive wrapper for a product space without exposing
    /// storage namespaces above engine.
    pub fn scoped_space(&mut self, space: &str) -> ScopedTransaction<'_> {
        let namespace = Arc::new(Namespace::for_branch_space(self.branch_id(), space));
        self.scoped(namespace)
    }

    /// Create graph metadata inside this transaction.
    pub fn graph_create_in_space(
        &mut self,
        space: &str,
        graph: &str,
        meta: GraphMeta,
    ) -> StrataResult<()> {
        let branch_id = self.branch_id();
        self.active_context_mut()?
            .graph_create(branch_id, space, graph, meta)
    }

    /// Add or update a graph node inside this transaction.
    pub fn graph_add_node_in_space(
        &mut self,
        space: &str,
        graph: &str,
        node_id: &str,
        data: &NodeData,
    ) -> StrataResult<bool> {
        let branch_id = self.branch_id();
        let backend_state = GraphStore::new(self.db.clone()).state()?;
        self.active_context_mut()?.graph_add_node(
            branch_id,
            space,
            graph,
            node_id,
            data,
            &backend_state,
        )
    }

    /// Get graph node data inside this transaction.
    pub fn graph_get_node_in_space(
        &mut self,
        space: &str,
        graph: &str,
        node_id: &str,
    ) -> StrataResult<Option<NodeData>> {
        let branch_id = self.branch_id();
        self.active_context_mut()?
            .graph_get_node(branch_id, space, graph, node_id)
    }

    /// Remove a graph node inside this transaction.
    pub fn graph_remove_node_in_space(
        &mut self,
        space: &str,
        graph: &str,
        node_id: &str,
    ) -> StrataResult<()> {
        let branch_id = self.branch_id();
        let backend_state = GraphStore::new(self.db.clone()).state()?;
        self.active_context_mut()?.graph_remove_node(
            branch_id,
            space,
            graph,
            node_id,
            &backend_state,
        )
    }

    /// List graph nodes inside this transaction.
    pub fn graph_list_nodes_in_space(
        &mut self,
        space: &str,
        graph: &str,
    ) -> StrataResult<Vec<String>> {
        let branch_id = self.branch_id();
        self.active_context_mut()?
            .graph_list_nodes(branch_id, space, graph)
    }

    /// Get graph metadata inside this transaction.
    pub fn graph_get_meta_in_space(
        &mut self,
        space: &str,
        graph: &str,
    ) -> StrataResult<Option<GraphMeta>> {
        let branch_id = self.branch_id();
        self.active_context_mut()?
            .graph_get_meta(branch_id, space, graph)
    }

    /// List graph names inside this transaction.
    pub fn graph_list_in_space(&mut self, space: &str) -> StrataResult<Vec<String>> {
        let branch_id = self.branch_id();
        self.active_context_mut()?.graph_list(branch_id, space)
    }

    /// Add or update a graph edge inside this transaction.
    #[allow(clippy::too_many_arguments)]
    pub fn graph_add_edge_in_space(
        &mut self,
        space: &str,
        graph: &str,
        src: &str,
        dst: &str,
        edge_type: &str,
        data: &EdgeData,
    ) -> StrataResult<bool> {
        let branch_id = self.branch_id();
        self.active_context_mut()?
            .graph_add_edge(branch_id, space, graph, src, dst, edge_type, data)
    }

    /// Remove a graph edge inside this transaction.
    #[allow(clippy::too_many_arguments)]
    pub fn graph_remove_edge_in_space(
        &mut self,
        space: &str,
        graph: &str,
        src: &str,
        dst: &str,
        edge_type: &str,
    ) -> StrataResult<()> {
        let branch_id = self.branch_id();
        self.active_context_mut()?
            .graph_remove_edge(branch_id, space, graph, src, dst, edge_type)
    }

    /// Get outgoing graph neighbors inside this transaction.
    pub fn graph_outgoing_neighbors_in_space(
        &mut self,
        space: &str,
        graph: &str,
        node_id: &str,
        edge_type_filter: Option<&str>,
    ) -> StrataResult<Vec<Neighbor>> {
        let branch_id = self.branch_id();
        self.active_context_mut()?.graph_outgoing_neighbors(
            branch_id,
            space,
            graph,
            node_id,
            edge_type_filter,
        )
    }

    /// Get incoming graph neighbors inside this transaction.
    pub fn graph_incoming_neighbors_in_space(
        &mut self,
        space: &str,
        graph: &str,
        node_id: &str,
        edge_type_filter: Option<&str>,
    ) -> StrataResult<Vec<Neighbor>> {
        let branch_id = self.branch_id();
        self.active_context_mut()?.graph_incoming_neighbors(
            branch_id,
            space,
            graph,
            node_id,
            edge_type_filter,
        )
    }

    /// Insert a vector inside this transaction.
    #[allow(clippy::too_many_arguments)]
    pub fn vector_insert_in_space(
        &mut self,
        store: &VectorStore,
        space: &str,
        collection: &str,
        key: &str,
        embedding: &[f32],
        metadata: Option<JsonValue>,
    ) -> StrataResult<Version> {
        let branch_id = self.branch_id();
        let ctx = self.active_context_mut()?;
        store
            .insert_in_transaction(ctx, branch_id, space, collection, key, embedding, metadata)
            .map_err(|error| error.into_strata_error(branch_id))
    }

    /// Get a vector record inside this transaction.
    pub fn vector_get_in_space(
        &mut self,
        store: &VectorStore,
        space: &str,
        collection: &str,
        key: &str,
    ) -> StrataResult<Option<VectorRecord>> {
        let branch_id = self.branch_id();
        let ctx = self.active_context_mut()?;
        store
            .get_in_transaction(ctx, branch_id, space, collection, key)
            .map_err(|error| error.into_strata_error(branch_id))
    }

    /// Delete a vector inside this transaction.
    pub fn vector_delete_in_space(
        &mut self,
        store: &VectorStore,
        space: &str,
        collection: &str,
        key: &str,
    ) -> StrataResult<bool> {
        let branch_id = self.branch_id();
        let ctx = self.active_context_mut()?;
        store
            .delete_in_transaction(ctx, branch_id, space, collection, key)
            .map_err(|error| error.into_strata_error(branch_id))
    }

    fn context(&self) -> Option<&TransactionContext> {
        self.ctx.as_ref()
    }
}

#[cfg(test)]
impl Deref for Transaction {
    type Target = TransactionContext;

    fn deref(&self) -> &Self::Target {
        self.context()
            .expect("transaction context not available after commit/abort")
    }
}

#[cfg(test)]
impl DerefMut for Transaction {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.context_mut()
            .expect("transaction context not available after commit/abort")
    }
}

impl Drop for Transaction {
    fn drop(&mut self) {
        self.abort();
    }
}
