//! Graph core service.

use std::collections::BTreeMap;

use strata_core_next::{CommitVersion, Timestamp};

use crate::branch::catalog::BranchCatalogRecord;
use crate::branch::BranchName;
use crate::commit::CommitOutcome;
use crate::control::ControlPlane;
use crate::data::kv::ProductSpace;
use crate::diagnostics::{EngineError, EngineResult};
use crate::persistence::{
    decode_graph_binding_key, decode_graph_edge_key, decode_graph_metadata_key,
    decode_graph_node_key, decode_graph_reverse_edge_key, encode_graph_binding_key,
    encode_graph_binding_space_prefix, encode_graph_binding_target_prefix, encode_graph_edge_key,
    encode_graph_edge_prefix, encode_graph_incoming_edge_prefix, encode_graph_metadata_key,
    encode_graph_metadata_prefix, encode_graph_node_key, encode_graph_node_prefix,
    encode_graph_outgoing_edge_prefix, encode_graph_reverse_edge_key,
    encode_graph_reverse_edge_prefix, CommitPlan, PersistenceReadRow, ReadSelector, RowAddress,
    RowClass, RowMutation, StoragePersistence,
};

use super::{
    decode_graph_binding_record, decode_graph_edge_record, decode_graph_metadata_record,
    decode_graph_node_record, encode_graph_binding_record, encode_graph_edge_record,
    encode_graph_metadata_record, encode_graph_node_record, GraphBatchOpOutcome,
    GraphBatchOperation, GraphBatchWrite, GraphBatchWriteOutcome, GraphBinding, GraphBindingPage,
    GraphBindingRecord, GraphBindingTarget, GraphDeleteOutcome, GraphDirection, GraphEdge,
    GraphEdgeRecord, GraphEdgeType, GraphEdgeWriteOutcome, GraphInfo, GraphName, GraphNamePage,
    GraphNeighbor, GraphNeighborPage, GraphNode, GraphNodeId, GraphNodePage, GraphNodeRecord,
    GraphWriteOutcome,
};

type EdgeIdentity = (GraphNodeId, GraphEdgeType, GraphNodeId);
type MutationKey = (RowClass, Vec<u8>);

/// Service for graph core operations.
pub struct GraphService<'a> {
    persistence: &'a mut StoragePersistence,
    control: &'a mut ControlPlane,
    branch: BranchName,
    space: ProductSpace,
}

impl<'a> GraphService<'a> {
    pub(crate) const fn new(
        persistence: &'a mut StoragePersistence,
        control: &'a mut ControlPlane,
        branch: BranchName,
        space: ProductSpace,
    ) -> Self {
        Self {
            persistence,
            control,
            branch,
            space,
        }
    }

    /// Creates a graph.
    pub fn create_graph(&mut self, name: GraphName) -> EngineResult<GraphInfo> {
        let record = self.branch_record()?;
        let address = self.metadata_address(&record, &name);
        if self
            .persistence
            .read_row(&address, ReadSelector::Latest)?
            .is_some_and(|row| !row.is_tombstone())
        {
            return Err(EngineError::conflict(
                "already_exists.engine.graph",
                "graph already exists",
            ));
        }
        let metadata = super::GraphMetadataRecord::new(name.clone());
        let commit = self.commit_batch(
            &record,
            vec![RowMutation::put(
                address,
                encode_graph_metadata_record(&metadata)?,
            )],
        )?;
        Ok(GraphInfo::new(
            name,
            0,
            0,
            commit.version(),
            commit.timestamp(),
            commit.version(),
            commit.timestamp(),
        ))
    }

    /// Deletes a graph and all visible graph data rows.
    pub fn delete_graph(&mut self, name: &GraphName) -> EngineResult<GraphDeleteOutcome> {
        let record = self.branch_record()?;
        if self
            .graph_metadata_row(&record, name, ReadSelector::Latest)?
            .is_none()
        {
            return Ok(GraphDeleteOutcome::new(name.clone(), false, None));
        }

        let mut mutations = Vec::new();
        mutations.push(RowMutation::delete(self.metadata_address(&record, name)));
        for row in self.node_rows(&record, name, ReadSelector::Latest)? {
            if !row.is_tombstone() {
                mutations.push(RowMutation::delete(RowAddress::new(
                    record.storage_branch_id(),
                    RowClass::GraphNode,
                    row.key().to_vec(),
                )));
            }
        }
        for row in self.edge_rows(&record, name, ReadSelector::Latest)? {
            if !row.is_tombstone() {
                mutations.push(RowMutation::delete(RowAddress::new(
                    record.storage_branch_id(),
                    RowClass::GraphEdge,
                    row.key().to_vec(),
                )));
            }
        }
        for row in self.reverse_edge_rows(&record, name, ReadSelector::Latest)? {
            if !row.is_tombstone() {
                mutations.push(RowMutation::delete(RowAddress::new(
                    record.storage_branch_id(),
                    RowClass::GraphReverseEdge,
                    row.key().to_vec(),
                )));
            }
        }
        for row in self.binding_rows_for_space(&record, ReadSelector::Latest)? {
            if row.is_tombstone() {
                continue;
            }
            let (_, graph, _) = decode_graph_binding_key(&self.space, row.key())?;
            if &graph == name {
                mutations.push(RowMutation::delete(RowAddress::new(
                    record.storage_branch_id(),
                    RowClass::GraphBindingIndex,
                    row.key().to_vec(),
                )));
            }
        }
        let commit = self.commit_batch(&record, mutations)?;
        Ok(GraphDeleteOutcome::new(name.clone(), true, Some(commit)))
    }

    /// Lists visible graphs.
    pub fn list_graphs(
        &mut self,
        cursor: Option<&GraphName>,
        limit: usize,
    ) -> EngineResult<GraphNamePage> {
        self.list_graphs_with_selector(cursor, limit, ReadSelector::Latest)
    }

    /// Lists graphs visible at a commit version.
    pub fn list_graphs_at_version(
        &mut self,
        cursor: Option<&GraphName>,
        limit: usize,
        version: CommitVersion,
    ) -> EngineResult<GraphNamePage> {
        self.list_graphs_with_selector(cursor, limit, ReadSelector::AtVersion(version))
    }

    /// Lists graphs visible at a timestamp.
    pub fn list_graphs_at(
        &mut self,
        cursor: Option<&GraphName>,
        limit: usize,
        timestamp: Timestamp,
    ) -> EngineResult<GraphNamePage> {
        self.list_graphs_with_selector(cursor, limit, ReadSelector::AtTimestamp(timestamp))
    }

    fn list_graphs_with_selector(
        &mut self,
        cursor: Option<&GraphName>,
        limit: usize,
        selector: ReadSelector,
    ) -> EngineResult<GraphNamePage> {
        let record = self.branch_record()?;
        if limit == 0 {
            return Ok(GraphNamePage::new(Vec::new(), false, None));
        }
        let mut graphs = self
            .persistence
            .scan_prefix(
                record.storage_branch_id(),
                RowClass::GraphMetadata,
                encode_graph_metadata_prefix(&self.space),
                selector,
                None,
            )?
            .into_iter()
            .filter(|row| !row.is_tombstone())
            .map(|row| decode_graph_metadata_key(&self.space, row.key()))
            .collect::<EngineResult<Vec<_>>>()?;
        graphs.sort();
        if let Some(cursor) = cursor {
            graphs.retain(|graph| graph > cursor);
        }
        let has_more = graphs.len() > limit;
        if has_more {
            graphs.truncate(limit);
        }
        let cursor = has_more.then(|| graphs.last().expect("non-empty page").clone());
        Ok(GraphNamePage::new(graphs, has_more, cursor))
    }

    /// Returns graph metadata when the graph exists.
    pub fn graph_info(&mut self, name: &GraphName) -> EngineResult<Option<GraphInfo>> {
        self.graph_info_with_selector(name, ReadSelector::Latest)
    }

    /// Returns graph metadata visible at a commit version.
    pub fn graph_info_at_version(
        &mut self,
        name: &GraphName,
        version: CommitVersion,
    ) -> EngineResult<Option<GraphInfo>> {
        self.graph_info_with_selector(name, ReadSelector::AtVersion(version))
    }

    /// Returns graph metadata visible at a timestamp.
    pub fn graph_info_at(
        &mut self,
        name: &GraphName,
        timestamp: Timestamp,
    ) -> EngineResult<Option<GraphInfo>> {
        self.graph_info_with_selector(name, ReadSelector::AtTimestamp(timestamp))
    }

    fn graph_info_with_selector(
        &mut self,
        name: &GraphName,
        selector: ReadSelector,
    ) -> EngineResult<Option<GraphInfo>> {
        let record = self.branch_record()?;
        self.graph_metadata_row(&record, name, selector)?
            .map(|row| self.graph_info_from_row(&record, &row, selector))
            .transpose()
    }

    /// Upserts one graph node.
    pub fn upsert_node(
        &mut self,
        graph: &GraphName,
        node_id: GraphNodeId,
        data: super::GraphNodeData,
    ) -> EngineResult<GraphWriteOutcome> {
        let record = self.branch_record()?;
        self.require_graph(&record, graph)?;
        let current = self.node_record(&record, graph, &node_id)?;
        let created = current.is_none();
        let new_record = GraphNodeRecord::new(graph.clone(), node_id.clone(), data);
        let mut mutations = Vec::new();
        if let Some(old) = current.as_ref().and_then(|record| record.data().binding()) {
            if Some(old) != new_record.data().binding() {
                mutations.push(RowMutation::delete(self.binding_address(
                    &record,
                    old.target(),
                    graph,
                    &node_id,
                )));
            }
        }
        mutations.push(RowMutation::put(
            self.node_address(&record, graph, &node_id),
            encode_graph_node_record(&new_record)?,
        ));
        if let Some(binding) = new_record.data().binding() {
            let binding_record =
                GraphBindingRecord::new(graph.clone(), node_id.clone(), binding.clone());
            mutations.push(RowMutation::put(
                self.binding_address(&record, binding.target(), graph, &node_id),
                encode_graph_binding_record(&binding_record)?,
            ));
        }
        let commit = self.commit_batch(&record, mutations)?;
        Ok(GraphWriteOutcome::new(
            graph.clone(),
            node_id,
            created,
            commit,
        ))
    }

    /// Reads one visible graph node.
    pub fn get_node(
        &mut self,
        graph: &GraphName,
        node_id: &GraphNodeId,
    ) -> EngineResult<Option<GraphNode>> {
        self.get_node_with_selector(graph, node_id, ReadSelector::Latest)
    }

    /// Reads one graph node visible at a commit version.
    pub fn get_node_at_version(
        &mut self,
        graph: &GraphName,
        node_id: &GraphNodeId,
        version: CommitVersion,
    ) -> EngineResult<Option<GraphNode>> {
        self.get_node_with_selector(graph, node_id, ReadSelector::AtVersion(version))
    }

    /// Reads one graph node visible at a timestamp.
    pub fn get_node_at(
        &mut self,
        graph: &GraphName,
        node_id: &GraphNodeId,
        timestamp: Timestamp,
    ) -> EngineResult<Option<GraphNode>> {
        self.get_node_with_selector(graph, node_id, ReadSelector::AtTimestamp(timestamp))
    }

    fn get_node_with_selector(
        &mut self,
        graph: &GraphName,
        node_id: &GraphNodeId,
        selector: ReadSelector,
    ) -> EngineResult<Option<GraphNode>> {
        let record = self.branch_record()?;
        self.require_graph_with_selector(&record, graph, selector)?;
        self.node_row_with_selector(&record, graph, node_id, selector)?
            .map(|row| self.node_from_row(&row))
            .transpose()
    }

    /// Deletes one graph node and its incident edges.
    pub fn delete_node(
        &mut self,
        graph: &GraphName,
        node_id: &GraphNodeId,
    ) -> EngineResult<GraphDeleteOutcome> {
        let record = self.branch_record()?;
        self.require_graph(&record, graph)?;
        let Some(current) = self.node_record(&record, graph, node_id)? else {
            return Ok(GraphDeleteOutcome::new(graph.clone(), false, None));
        };
        let mut mutations = MutationMap::default();
        mutations.delete(self.node_address(&record, graph, node_id));
        if let Some(binding) = current.data().binding() {
            mutations.delete(self.binding_address(&record, binding.target(), graph, node_id));
        }
        for edge in self
            .edge_record_map(&record, graph, ReadSelector::Latest)?
            .into_values()
        {
            if edge.src() == node_id || edge.dst() == node_id {
                self.delete_edge_mutations(&record, &mut mutations, &edge);
            }
        }
        let commit = self.commit_batch(&record, mutations.into_mutations())?;
        Ok(GraphDeleteOutcome::new(graph.clone(), true, Some(commit)))
    }

    /// Lists visible graph nodes.
    pub fn list_nodes(
        &mut self,
        graph: &GraphName,
        prefix: Option<&GraphNodeId>,
        cursor: Option<&GraphNodeId>,
        limit: usize,
    ) -> EngineResult<GraphNodePage> {
        self.list_nodes_with_selector(graph, prefix, cursor, limit, ReadSelector::Latest)
    }

    /// Lists graph nodes visible at a commit version.
    pub fn list_nodes_at_version(
        &mut self,
        graph: &GraphName,
        prefix: Option<&GraphNodeId>,
        cursor: Option<&GraphNodeId>,
        limit: usize,
        version: CommitVersion,
    ) -> EngineResult<GraphNodePage> {
        self.list_nodes_with_selector(
            graph,
            prefix,
            cursor,
            limit,
            ReadSelector::AtVersion(version),
        )
    }

    /// Lists graph nodes visible at a timestamp.
    pub fn list_nodes_at(
        &mut self,
        graph: &GraphName,
        prefix: Option<&GraphNodeId>,
        cursor: Option<&GraphNodeId>,
        limit: usize,
        timestamp: Timestamp,
    ) -> EngineResult<GraphNodePage> {
        self.list_nodes_with_selector(
            graph,
            prefix,
            cursor,
            limit,
            ReadSelector::AtTimestamp(timestamp),
        )
    }

    fn list_nodes_with_selector(
        &mut self,
        graph: &GraphName,
        prefix: Option<&GraphNodeId>,
        cursor: Option<&GraphNodeId>,
        limit: usize,
        selector: ReadSelector,
    ) -> EngineResult<GraphNodePage> {
        let record = self.branch_record()?;
        self.require_graph_with_selector(&record, graph, selector)?;
        if limit == 0 {
            return Ok(GraphNodePage::new(Vec::new(), false, None));
        }
        let mut nodes = self
            .node_rows(&record, graph, selector)?
            .into_iter()
            .filter(|row| !row.is_tombstone())
            .map(|row| self.node_from_row(&row))
            .collect::<EngineResult<Vec<_>>>()?;
        nodes.sort_by(|left, right| left.node_id().cmp(right.node_id()));
        if let Some(prefix) = prefix {
            nodes.retain(|node| node.node_id().as_str().starts_with(prefix.as_str()));
        }
        if let Some(cursor) = cursor {
            nodes.retain(|node| node.node_id() > cursor);
        }
        let has_more = nodes.len() > limit;
        if has_more {
            nodes.truncate(limit);
        }
        let cursor = has_more.then(|| nodes.last().expect("non-empty page").node_id().clone());
        Ok(GraphNodePage::new(nodes, has_more, cursor))
    }

    /// Upserts one graph edge.
    pub fn upsert_edge(
        &mut self,
        graph: &GraphName,
        src: GraphNodeId,
        edge_type: GraphEdgeType,
        dst: GraphNodeId,
        data: super::GraphEdgeData,
    ) -> EngineResult<GraphEdgeWriteOutcome> {
        let record = self.branch_record()?;
        self.require_graph(&record, graph)?;
        self.require_node(&record, graph, &src)?;
        self.require_node(&record, graph, &dst)?;
        let created = self
            .edge_record(&record, graph, &src, &edge_type, &dst)?
            .is_none();
        let edge = GraphEdgeRecord::new(
            graph.clone(),
            src.clone(),
            edge_type.clone(),
            dst.clone(),
            data,
        );
        let commit = self.commit_batch(
            &record,
            vec![
                RowMutation::put(
                    self.edge_address(&record, graph, &src, &edge_type, &dst),
                    encode_graph_edge_record(&edge)?,
                ),
                RowMutation::put(
                    self.reverse_edge_address(&record, graph, &dst, &edge_type, &src),
                    encode_graph_edge_record(&edge)?,
                ),
            ],
        )?;
        Ok(GraphEdgeWriteOutcome::new(
            graph.clone(),
            src,
            edge_type,
            dst,
            created,
            commit,
        ))
    }

    /// Reads one graph edge.
    pub fn get_edge(
        &mut self,
        graph: &GraphName,
        src: &GraphNodeId,
        edge_type: &GraphEdgeType,
        dst: &GraphNodeId,
    ) -> EngineResult<Option<GraphEdge>> {
        self.get_edge_with_selector(graph, src, edge_type, dst, ReadSelector::Latest)
    }

    /// Reads one graph edge visible at a commit version.
    pub fn get_edge_at_version(
        &mut self,
        graph: &GraphName,
        src: &GraphNodeId,
        edge_type: &GraphEdgeType,
        dst: &GraphNodeId,
        version: CommitVersion,
    ) -> EngineResult<Option<GraphEdge>> {
        self.get_edge_with_selector(graph, src, edge_type, dst, ReadSelector::AtVersion(version))
    }

    /// Reads one graph edge visible at a timestamp.
    pub fn get_edge_at(
        &mut self,
        graph: &GraphName,
        src: &GraphNodeId,
        edge_type: &GraphEdgeType,
        dst: &GraphNodeId,
        timestamp: Timestamp,
    ) -> EngineResult<Option<GraphEdge>> {
        self.get_edge_with_selector(
            graph,
            src,
            edge_type,
            dst,
            ReadSelector::AtTimestamp(timestamp),
        )
    }

    fn get_edge_with_selector(
        &mut self,
        graph: &GraphName,
        src: &GraphNodeId,
        edge_type: &GraphEdgeType,
        dst: &GraphNodeId,
        selector: ReadSelector,
    ) -> EngineResult<Option<GraphEdge>> {
        let record = self.branch_record()?;
        self.require_graph_with_selector(&record, graph, selector)?;
        self.edge_row_with_selector(&record, graph, src, edge_type, dst, selector)?
            .map(|row| self.edge_from_forward_row(&row))
            .transpose()
    }

    /// Deletes one graph edge.
    pub fn delete_edge(
        &mut self,
        graph: &GraphName,
        src: &GraphNodeId,
        edge_type: &GraphEdgeType,
        dst: &GraphNodeId,
    ) -> EngineResult<GraphDeleteOutcome> {
        let record = self.branch_record()?;
        self.require_graph(&record, graph)?;
        let Some(edge) = self.edge_record(&record, graph, src, edge_type, dst)? else {
            return Ok(GraphDeleteOutcome::new(graph.clone(), false, None));
        };
        let mut mutations = MutationMap::default();
        self.delete_edge_mutations(&record, &mut mutations, &edge);
        let commit = self.commit_batch(&record, mutations.into_mutations())?;
        Ok(GraphDeleteOutcome::new(graph.clone(), true, Some(commit)))
    }

    /// Looks up neighboring nodes.
    pub fn neighbors(
        &mut self,
        graph: &GraphName,
        node_id: &GraphNodeId,
        direction: GraphDirection,
        edge_type: Option<&GraphEdgeType>,
        cursor: Option<&str>,
        limit: usize,
    ) -> EngineResult<GraphNeighborPage> {
        self.neighbors_with_selector(
            graph,
            node_id,
            direction,
            edge_type,
            cursor,
            limit,
            ReadSelector::Latest,
        )
    }

    /// Looks up neighboring nodes visible at a commit version.
    pub fn neighbors_at_version(
        &mut self,
        graph: &GraphName,
        node_id: &GraphNodeId,
        direction: GraphDirection,
        edge_type: Option<&GraphEdgeType>,
        cursor: Option<&str>,
        limit: usize,
        version: CommitVersion,
    ) -> EngineResult<GraphNeighborPage> {
        self.neighbors_with_selector(
            graph,
            node_id,
            direction,
            edge_type,
            cursor,
            limit,
            ReadSelector::AtVersion(version),
        )
    }

    /// Looks up neighboring nodes visible at a timestamp.
    pub fn neighbors_at(
        &mut self,
        graph: &GraphName,
        node_id: &GraphNodeId,
        direction: GraphDirection,
        edge_type: Option<&GraphEdgeType>,
        cursor: Option<&str>,
        limit: usize,
        timestamp: Timestamp,
    ) -> EngineResult<GraphNeighborPage> {
        self.neighbors_with_selector(
            graph,
            node_id,
            direction,
            edge_type,
            cursor,
            limit,
            ReadSelector::AtTimestamp(timestamp),
        )
    }

    fn neighbors_with_selector(
        &mut self,
        graph: &GraphName,
        node_id: &GraphNodeId,
        direction: GraphDirection,
        edge_type: Option<&GraphEdgeType>,
        cursor: Option<&str>,
        limit: usize,
        selector: ReadSelector,
    ) -> EngineResult<GraphNeighborPage> {
        let record = self.branch_record()?;
        self.require_graph_with_selector(&record, graph, selector)?;
        if limit == 0
            || self
                .node_record_with_selector(&record, graph, node_id, selector)?
                .is_none()
        {
            return Ok(GraphNeighborPage::new(Vec::new(), false, None));
        }
        let mut hits = Vec::new();
        if matches!(direction, GraphDirection::Outgoing | GraphDirection::Both) {
            hits.extend(self.outgoing_neighbors(&record, graph, node_id, edge_type, selector)?);
        }
        if matches!(direction, GraphDirection::Incoming | GraphDirection::Both) {
            hits.extend(self.incoming_neighbors(&record, graph, node_id, edge_type, selector)?);
        }
        hits.sort_by_key(neighbor_cursor);
        if let Some(cursor) = cursor {
            hits.retain(|hit| neighbor_cursor(hit).as_str() > cursor);
        }
        let has_more = hits.len() > limit;
        if has_more {
            hits.truncate(limit);
        }
        let cursor = has_more.then(|| neighbor_cursor(hits.last().expect("non-empty page")));
        Ok(GraphNeighborPage::new(hits, has_more, cursor))
    }

    /// Looks up graph nodes bound to an entity target.
    pub fn bindings_for_entity(
        &mut self,
        target: &GraphBindingTarget,
        cursor: Option<&str>,
        limit: usize,
    ) -> EngineResult<GraphBindingPage> {
        self.bindings_for_entity_with_selector(target, cursor, limit, ReadSelector::Latest)
    }

    /// Looks up graph nodes bound to an entity target at a commit version.
    pub fn bindings_for_entity_at_version(
        &mut self,
        target: &GraphBindingTarget,
        cursor: Option<&str>,
        limit: usize,
        version: CommitVersion,
    ) -> EngineResult<GraphBindingPage> {
        self.bindings_for_entity_with_selector(
            target,
            cursor,
            limit,
            ReadSelector::AtVersion(version),
        )
    }

    /// Looks up graph nodes bound to an entity target at a timestamp.
    pub fn bindings_for_entity_at(
        &mut self,
        target: &GraphBindingTarget,
        cursor: Option<&str>,
        limit: usize,
        timestamp: Timestamp,
    ) -> EngineResult<GraphBindingPage> {
        self.bindings_for_entity_with_selector(
            target,
            cursor,
            limit,
            ReadSelector::AtTimestamp(timestamp),
        )
    }

    fn bindings_for_entity_with_selector(
        &mut self,
        target: &GraphBindingTarget,
        cursor: Option<&str>,
        limit: usize,
        selector: ReadSelector,
    ) -> EngineResult<GraphBindingPage> {
        let record = self.branch_record()?;
        if limit == 0 {
            return Ok(GraphBindingPage::new(Vec::new(), false, None));
        }
        let mut bindings = self
            .persistence
            .scan_prefix(
                record.storage_branch_id(),
                RowClass::GraphBindingIndex,
                encode_graph_binding_target_prefix(&self.space, target),
                selector,
                None,
            )?
            .into_iter()
            .filter(|row| !row.is_tombstone())
            .map(|row| self.binding_from_row(&row))
            .collect::<EngineResult<Vec<_>>>()?;
        bindings.sort_by_key(binding_cursor);
        if let Some(cursor) = cursor {
            bindings.retain(|binding| binding_cursor(binding).as_str() > cursor);
        }
        let has_more = bindings.len() > limit;
        if has_more {
            bindings.truncate(limit);
        }
        let cursor = has_more.then(|| binding_cursor(bindings.last().expect("non-empty page")));
        Ok(GraphBindingPage::new(bindings, has_more, cursor))
    }

    /// Applies an all-or-nothing graph batch.
    #[allow(clippy::too_many_lines)]
    pub fn batch_write(
        &mut self,
        graph: &GraphName,
        batch: &GraphBatchWrite,
    ) -> EngineResult<GraphBatchWriteOutcome> {
        let record = self.branch_record()?;
        self.require_graph(&record, graph)?;
        if batch.is_empty() {
            return Ok(GraphBatchWriteOutcome::new(graph.clone(), Vec::new(), None));
        }

        let mut nodes = self.node_record_map(&record, graph, ReadSelector::Latest)?;
        let mut edges = self.edge_record_map(&record, graph, ReadSelector::Latest)?;
        let mut mutations = MutationMap::default();
        let mut outcomes = Vec::with_capacity(batch.operations().len());

        for (index, operation) in batch.operations().iter().enumerate() {
            match operation {
                GraphBatchOperation::UpsertNode { node_id, data } => {
                    let created = !nodes.contains_key(node_id);
                    if let Some(old) = nodes
                        .get(node_id)
                        .and_then(|record| record.data().binding())
                    {
                        if data.binding() != Some(old) {
                            mutations.delete(self.binding_address(
                                &record,
                                old.target(),
                                graph,
                                node_id,
                            ));
                        }
                    }
                    let node = GraphNodeRecord::new(graph.clone(), node_id.clone(), data.clone());
                    mutations.put(
                        self.node_address(&record, graph, node_id),
                        encode_graph_node_record(&node)?,
                    );
                    if let Some(binding) = node.data().binding() {
                        let binding_record = GraphBindingRecord::new(
                            graph.clone(),
                            node_id.clone(),
                            binding.clone(),
                        );
                        mutations.put(
                            self.binding_address(&record, binding.target(), graph, node_id),
                            encode_graph_binding_record(&binding_record)?,
                        );
                    }
                    nodes.insert(node_id.clone(), node);
                    outcomes.push(GraphBatchOpOutcome::created(index, created));
                }
                GraphBatchOperation::DeleteNode { node_id } => {
                    let removed = nodes.remove(node_id);
                    let deleted = removed.is_some();
                    if let Some(removed) = removed {
                        mutations.delete(self.node_address(&record, graph, node_id));
                        if let Some(binding) = removed.data().binding() {
                            mutations.delete(self.binding_address(
                                &record,
                                binding.target(),
                                graph,
                                node_id,
                            ));
                        }
                        let incident = edges
                            .values()
                            .filter(|edge| edge.src() == node_id || edge.dst() == node_id)
                            .cloned()
                            .collect::<Vec<_>>();
                        for edge in incident {
                            edges.remove(&edge_identity(&edge));
                            self.delete_edge_mutations(&record, &mut mutations, &edge);
                        }
                    }
                    outcomes.push(GraphBatchOpOutcome::deleted(index, deleted));
                }
                GraphBatchOperation::UpsertEdge {
                    src,
                    edge_type,
                    dst,
                    data,
                } => {
                    if !nodes.contains_key(src) || !nodes.contains_key(dst) {
                        return Err(EngineError::invalid_input(
                            "invalid_argument.engine.graph_edge_endpoint",
                            "graph edge endpoints must exist before an edge can be written",
                        ));
                    }
                    let identity = (src.clone(), edge_type.clone(), dst.clone());
                    let created = !edges.contains_key(&identity);
                    let edge = GraphEdgeRecord::new(
                        graph.clone(),
                        src.clone(),
                        edge_type.clone(),
                        dst.clone(),
                        data.clone(),
                    );
                    self.put_edge_mutations(&record, &mut mutations, &edge)?;
                    edges.insert(identity, edge);
                    outcomes.push(GraphBatchOpOutcome::created(index, created));
                }
                GraphBatchOperation::DeleteEdge {
                    src,
                    edge_type,
                    dst,
                } => {
                    let identity = (src.clone(), edge_type.clone(), dst.clone());
                    let deleted = edges.remove(&identity).is_some();
                    if deleted {
                        let edge = GraphEdgeRecord::new(
                            graph.clone(),
                            src.clone(),
                            edge_type.clone(),
                            dst.clone(),
                            super::GraphEdgeData::default(),
                        );
                        self.delete_edge_mutations(&record, &mut mutations, &edge);
                    }
                    outcomes.push(GraphBatchOpOutcome::deleted(index, deleted));
                }
            }
        }

        let mutations = mutations.into_mutations();
        if mutations.is_empty() {
            return Ok(GraphBatchWriteOutcome::new(graph.clone(), outcomes, None));
        }
        let commit = self.commit_batch(&record, mutations)?;
        Ok(GraphBatchWriteOutcome::new(
            graph.clone(),
            outcomes,
            Some(commit),
        ))
    }

    fn branch_record(&self) -> EngineResult<BranchCatalogRecord> {
        self.control.require_healthy()?;
        self.control
            .lookup_branch(&self.branch)
            .cloned()
            .ok_or_else(|| {
                EngineError::not_found(
                    "not_found.engine.branch",
                    format!("branch `{}` does not exist", self.branch),
                )
            })
    }

    fn metadata_address(&self, record: &BranchCatalogRecord, graph: &GraphName) -> RowAddress {
        RowAddress::new(
            record.storage_branch_id(),
            RowClass::GraphMetadata,
            encode_graph_metadata_key(&self.space, graph),
        )
    }

    fn node_address(
        &self,
        record: &BranchCatalogRecord,
        graph: &GraphName,
        node_id: &GraphNodeId,
    ) -> RowAddress {
        RowAddress::new(
            record.storage_branch_id(),
            RowClass::GraphNode,
            encode_graph_node_key(&self.space, graph, node_id),
        )
    }

    fn edge_address(
        &self,
        record: &BranchCatalogRecord,
        graph: &GraphName,
        src: &GraphNodeId,
        edge_type: &GraphEdgeType,
        dst: &GraphNodeId,
    ) -> RowAddress {
        RowAddress::new(
            record.storage_branch_id(),
            RowClass::GraphEdge,
            encode_graph_edge_key(&self.space, graph, src, edge_type, dst),
        )
    }

    fn reverse_edge_address(
        &self,
        record: &BranchCatalogRecord,
        graph: &GraphName,
        dst: &GraphNodeId,
        edge_type: &GraphEdgeType,
        src: &GraphNodeId,
    ) -> RowAddress {
        RowAddress::new(
            record.storage_branch_id(),
            RowClass::GraphReverseEdge,
            encode_graph_reverse_edge_key(&self.space, graph, dst, edge_type, src),
        )
    }

    fn binding_address(
        &self,
        record: &BranchCatalogRecord,
        target: &GraphBindingTarget,
        graph: &GraphName,
        node_id: &GraphNodeId,
    ) -> RowAddress {
        RowAddress::new(
            record.storage_branch_id(),
            RowClass::GraphBindingIndex,
            encode_graph_binding_key(&self.space, target, graph, node_id),
        )
    }

    fn graph_metadata_row(
        &mut self,
        record: &BranchCatalogRecord,
        graph: &GraphName,
        selector: ReadSelector,
    ) -> EngineResult<Option<PersistenceReadRow>> {
        let address = self.metadata_address(record, graph);
        Ok(self
            .persistence
            .read_row(&address, selector)?
            .filter(|row| !row.is_tombstone()))
    }

    fn require_graph(
        &mut self,
        record: &BranchCatalogRecord,
        graph: &GraphName,
    ) -> EngineResult<()> {
        self.require_graph_with_selector(record, graph, ReadSelector::Latest)
    }

    fn require_graph_with_selector(
        &mut self,
        record: &BranchCatalogRecord,
        graph: &GraphName,
        selector: ReadSelector,
    ) -> EngineResult<()> {
        let Some(row) = self.graph_metadata_row(record, graph, selector)? else {
            return Err(EngineError::not_found(
                "not_found.engine.graph",
                "graph does not exist",
            ));
        };
        let value = row.value().ok_or_else(|| {
            EngineError::corruption(
                "data_loss.engine.graph_metadata",
                "stored graph metadata row is missing a value",
            )
        })?;
        let _ = decode_graph_metadata_record(graph, value)?;
        Ok(())
    }

    fn require_node(
        &mut self,
        record: &BranchCatalogRecord,
        graph: &GraphName,
        node_id: &GraphNodeId,
    ) -> EngineResult<()> {
        if self.node_record(record, graph, node_id)?.is_some() {
            return Ok(());
        }
        Err(EngineError::invalid_input(
            "invalid_argument.engine.graph_edge_endpoint",
            "graph edge endpoints must exist before an edge can be written",
        ))
    }

    fn graph_info_from_row(
        &mut self,
        record: &BranchCatalogRecord,
        row: &PersistenceReadRow,
        selector: ReadSelector,
    ) -> EngineResult<GraphInfo> {
        let graph = decode_graph_metadata_key(&self.space, row.key())?;
        let value = row.value().ok_or_else(|| {
            EngineError::corruption(
                "data_loss.engine.graph_metadata",
                "stored graph metadata row is missing a value",
            )
        })?;
        let _ = decode_graph_metadata_record(&graph, value)?;
        let node_rows = self.node_rows(record, &graph, selector)?;
        let edge_rows = self.edge_rows(record, &graph, selector)?;
        let mut node_count = 0_u64;
        for row in node_rows.iter().filter(|row| !row.is_tombstone()) {
            let _ = self.node_record_from_row(row)?;
            node_count = node_count.saturating_add(1);
        }
        let mut edge_count = 0_u64;
        for row in edge_rows.iter().filter(|row| !row.is_tombstone()) {
            let _ = self.edge_record_from_forward_row(row)?;
            edge_count = edge_count.saturating_add(1);
        }
        let mut updated_version = row.commit_version();
        let mut updated_timestamp = row.commit_timestamp();
        for candidate in node_rows.iter().chain(edge_rows.iter()) {
            if candidate.commit_version() > updated_version {
                updated_version = candidate.commit_version();
                updated_timestamp = candidate.commit_timestamp();
            }
        }
        Ok(GraphInfo::new(
            graph,
            node_count,
            edge_count,
            row.commit_version(),
            row.commit_timestamp(),
            updated_version,
            updated_timestamp,
        ))
    }

    fn node_row_with_selector(
        &mut self,
        record: &BranchCatalogRecord,
        graph: &GraphName,
        node_id: &GraphNodeId,
        selector: ReadSelector,
    ) -> EngineResult<Option<PersistenceReadRow>> {
        let address = self.node_address(record, graph, node_id);
        Ok(self
            .persistence
            .read_row(&address, selector)?
            .filter(|row| !row.is_tombstone()))
    }

    fn node_record(
        &mut self,
        record: &BranchCatalogRecord,
        graph: &GraphName,
        node_id: &GraphNodeId,
    ) -> EngineResult<Option<GraphNodeRecord>> {
        self.node_record_with_selector(record, graph, node_id, ReadSelector::Latest)
    }

    fn node_record_with_selector(
        &mut self,
        record: &BranchCatalogRecord,
        graph: &GraphName,
        node_id: &GraphNodeId,
        selector: ReadSelector,
    ) -> EngineResult<Option<GraphNodeRecord>> {
        self.node_row_with_selector(record, graph, node_id, selector)?
            .map(|row| self.node_record_from_row(&row))
            .transpose()
    }

    fn edge_row_with_selector(
        &mut self,
        record: &BranchCatalogRecord,
        graph: &GraphName,
        src: &GraphNodeId,
        edge_type: &GraphEdgeType,
        dst: &GraphNodeId,
        selector: ReadSelector,
    ) -> EngineResult<Option<PersistenceReadRow>> {
        let address = self.edge_address(record, graph, src, edge_type, dst);
        Ok(self
            .persistence
            .read_row(&address, selector)?
            .filter(|row| !row.is_tombstone()))
    }

    fn edge_record(
        &mut self,
        record: &BranchCatalogRecord,
        graph: &GraphName,
        src: &GraphNodeId,
        edge_type: &GraphEdgeType,
        dst: &GraphNodeId,
    ) -> EngineResult<Option<GraphEdgeRecord>> {
        self.edge_row_with_selector(record, graph, src, edge_type, dst, ReadSelector::Latest)?
            .map(|row| self.edge_record_from_forward_row(&row))
            .transpose()
    }

    fn node_rows(
        &mut self,
        record: &BranchCatalogRecord,
        graph: &GraphName,
        selector: ReadSelector,
    ) -> EngineResult<Vec<PersistenceReadRow>> {
        self.persistence.scan_prefix(
            record.storage_branch_id(),
            RowClass::GraphNode,
            encode_graph_node_prefix(&self.space, graph),
            selector,
            None,
        )
    }

    fn edge_rows(
        &mut self,
        record: &BranchCatalogRecord,
        graph: &GraphName,
        selector: ReadSelector,
    ) -> EngineResult<Vec<PersistenceReadRow>> {
        self.persistence.scan_prefix(
            record.storage_branch_id(),
            RowClass::GraphEdge,
            encode_graph_edge_prefix(&self.space, graph),
            selector,
            None,
        )
    }

    fn reverse_edge_rows(
        &mut self,
        record: &BranchCatalogRecord,
        graph: &GraphName,
        selector: ReadSelector,
    ) -> EngineResult<Vec<PersistenceReadRow>> {
        self.persistence.scan_prefix(
            record.storage_branch_id(),
            RowClass::GraphReverseEdge,
            encode_graph_reverse_edge_prefix(&self.space, graph),
            selector,
            None,
        )
    }

    fn binding_rows_for_space(
        &mut self,
        record: &BranchCatalogRecord,
        selector: ReadSelector,
    ) -> EngineResult<Vec<PersistenceReadRow>> {
        self.persistence.scan_prefix(
            record.storage_branch_id(),
            RowClass::GraphBindingIndex,
            encode_graph_binding_space_prefix(&self.space),
            selector,
            None,
        )
    }

    fn node_record_map(
        &mut self,
        record: &BranchCatalogRecord,
        graph: &GraphName,
        selector: ReadSelector,
    ) -> EngineResult<BTreeMap<GraphNodeId, GraphNodeRecord>> {
        self.node_rows(record, graph, selector)?
            .into_iter()
            .filter(|row| !row.is_tombstone())
            .map(|row| {
                let record = self.node_record_from_row(&row)?;
                Ok((record.node_id().clone(), record))
            })
            .collect()
    }

    fn edge_record_map(
        &mut self,
        record: &BranchCatalogRecord,
        graph: &GraphName,
        selector: ReadSelector,
    ) -> EngineResult<BTreeMap<EdgeIdentity, GraphEdgeRecord>> {
        self.edge_rows(record, graph, selector)?
            .into_iter()
            .filter(|row| !row.is_tombstone())
            .map(|row| {
                let record = self.edge_record_from_forward_row(&row)?;
                Ok((edge_identity(&record), record))
            })
            .collect()
    }

    fn outgoing_neighbors(
        &mut self,
        record: &BranchCatalogRecord,
        graph: &GraphName,
        node_id: &GraphNodeId,
        edge_type: Option<&GraphEdgeType>,
        selector: ReadSelector,
    ) -> EngineResult<Vec<GraphNeighbor>> {
        self.persistence
            .scan_prefix(
                record.storage_branch_id(),
                RowClass::GraphEdge,
                encode_graph_outgoing_edge_prefix(&self.space, graph, node_id),
                selector,
                None,
            )?
            .into_iter()
            .filter(|row| !row.is_tombstone())
            .map(|row| {
                let edge = self.edge_from_forward_row(&row)?;
                if edge_type.is_some_and(|expected| edge.edge_type() != expected) {
                    return Ok(None);
                }
                let node = self.visible_node_or_corruption(record, graph, edge.dst(), selector)?;
                Ok(Some(GraphNeighbor::new(
                    node,
                    edge,
                    GraphDirection::Outgoing,
                )))
            })
            .filter_map(Result::transpose)
            .collect()
    }

    fn incoming_neighbors(
        &mut self,
        record: &BranchCatalogRecord,
        graph: &GraphName,
        node_id: &GraphNodeId,
        edge_type: Option<&GraphEdgeType>,
        selector: ReadSelector,
    ) -> EngineResult<Vec<GraphNeighbor>> {
        self.persistence
            .scan_prefix(
                record.storage_branch_id(),
                RowClass::GraphReverseEdge,
                encode_graph_incoming_edge_prefix(&self.space, graph, node_id),
                selector,
                None,
            )?
            .into_iter()
            .filter(|row| !row.is_tombstone())
            .map(|row| {
                let edge = self.edge_from_reverse_row(&row)?;
                if edge_type.is_some_and(|expected| edge.edge_type() != expected) {
                    return Ok(None);
                }
                let node = self.visible_node_or_corruption(record, graph, edge.src(), selector)?;
                Ok(Some(GraphNeighbor::new(
                    node,
                    edge,
                    GraphDirection::Incoming,
                )))
            })
            .filter_map(Result::transpose)
            .collect()
    }

    fn visible_node_or_corruption(
        &mut self,
        record: &BranchCatalogRecord,
        graph: &GraphName,
        node_id: &GraphNodeId,
        selector: ReadSelector,
    ) -> EngineResult<GraphNode> {
        self.get_node_with_record(record, graph, node_id, selector)?
            .ok_or_else(|| {
                EngineError::corruption(
                    "data_loss.engine.graph_index",
                    "stored graph edge index points at a missing node",
                )
            })
    }

    fn get_node_with_record(
        &mut self,
        record: &BranchCatalogRecord,
        graph: &GraphName,
        node_id: &GraphNodeId,
        selector: ReadSelector,
    ) -> EngineResult<Option<GraphNode>> {
        self.node_row_with_selector(record, graph, node_id, selector)?
            .map(|row| self.node_from_row(&row))
            .transpose()
    }

    fn node_from_row(&self, row: &PersistenceReadRow) -> EngineResult<GraphNode> {
        let record = self.node_record_from_row(row)?;
        Ok(GraphNode::new(
            record.graph().clone(),
            record.node_id().clone(),
            record.data().clone(),
            row.commit_version(),
            row.commit_timestamp(),
        ))
    }

    fn node_record_from_row(&self, row: &PersistenceReadRow) -> EngineResult<GraphNodeRecord> {
        let (graph, node_id) = decode_graph_node_key(&self.space, row.key())?;
        let value = row.value().ok_or_else(|| {
            EngineError::corruption(
                "data_loss.engine.graph_node_record",
                "stored graph node row is missing a value",
            )
        })?;
        decode_graph_node_record(&graph, &node_id, value)
    }

    fn edge_from_forward_row(&self, row: &PersistenceReadRow) -> EngineResult<GraphEdge> {
        let record = self.edge_record_from_forward_row(row)?;
        Ok(Self::edge_from_record(&record, row))
    }

    fn edge_from_reverse_row(&self, row: &PersistenceReadRow) -> EngineResult<GraphEdge> {
        let record = self.edge_record_from_reverse_row(row)?;
        Ok(Self::edge_from_record(&record, row))
    }

    fn edge_from_record(record: &GraphEdgeRecord, row: &PersistenceReadRow) -> GraphEdge {
        GraphEdge::new(
            record.graph().clone(),
            record.src().clone(),
            record.edge_type().clone(),
            record.dst().clone(),
            record.data().clone(),
            row.commit_version(),
            row.commit_timestamp(),
        )
    }

    fn edge_record_from_forward_row(
        &self,
        row: &PersistenceReadRow,
    ) -> EngineResult<GraphEdgeRecord> {
        let (graph, src, edge_type, dst) = decode_graph_edge_key(&self.space, row.key())?;
        let value = row.value().ok_or_else(|| {
            EngineError::corruption(
                "data_loss.engine.graph_edge_record",
                "stored graph edge row is missing a value",
            )
        })?;
        decode_graph_edge_record(&graph, &src, &edge_type, &dst, value)
    }

    fn edge_record_from_reverse_row(
        &self,
        row: &PersistenceReadRow,
    ) -> EngineResult<GraphEdgeRecord> {
        let (graph, dst, edge_type, src) = decode_graph_reverse_edge_key(&self.space, row.key())?;
        let value = row.value().ok_or_else(|| {
            EngineError::corruption(
                "data_loss.engine.graph_edge_record",
                "stored graph reverse edge row is missing a value",
            )
        })?;
        decode_graph_edge_record(&graph, &src, &edge_type, &dst, value)
    }

    fn binding_from_row(&self, row: &PersistenceReadRow) -> EngineResult<GraphBinding> {
        binding_from_index_row(&self.space, row)
    }

    fn put_edge_mutations(
        &self,
        record: &BranchCatalogRecord,
        mutations: &mut MutationMap,
        edge: &GraphEdgeRecord,
    ) -> EngineResult<()> {
        let encoded = encode_graph_edge_record(edge)?;
        mutations.put(
            self.edge_address(
                record,
                edge.graph(),
                edge.src(),
                edge.edge_type(),
                edge.dst(),
            ),
            encoded.clone(),
        );
        mutations.put(
            self.reverse_edge_address(
                record,
                edge.graph(),
                edge.dst(),
                edge.edge_type(),
                edge.src(),
            ),
            encoded,
        );
        Ok(())
    }

    fn delete_edge_mutations(
        &self,
        record: &BranchCatalogRecord,
        mutations: &mut MutationMap,
        edge: &GraphEdgeRecord,
    ) {
        mutations.delete(self.edge_address(
            record,
            edge.graph(),
            edge.src(),
            edge.edge_type(),
            edge.dst(),
        ));
        mutations.delete(self.reverse_edge_address(
            record,
            edge.graph(),
            edge.dst(),
            edge.edge_type(),
            edge.src(),
        ));
    }

    fn commit_batch(
        &mut self,
        record: &BranchCatalogRecord,
        mutations: Vec<RowMutation>,
    ) -> EngineResult<CommitOutcome> {
        let mut mutations = mutations;
        if mutations.is_empty() {
            return Err(EngineError::invalid_input(
                "invalid_argument.engine.graph_batch",
                "graph batch must contain at least one mutation",
            ));
        }
        let user_put_count = mutations
            .iter()
            .filter(|mutation| mutation.is_put())
            .count();
        let user_delete_count = mutations
            .iter()
            .filter(|mutation| mutation.is_delete())
            .count();
        let mut space_mutations =
            self.control
                .space_registration_mutations(self.persistence, record, &self.space)?;
        if !space_mutations.is_empty() {
            space_mutations.extend(mutations);
            mutations = space_mutations;
        }
        let plan = CommitPlan::new(
            record.storage_branch_id(),
            mutations,
            Some(record.generation()),
        );
        Ok(self
            .persistence
            .commit(&plan)?
            .with_counts(user_put_count, user_delete_count))
    }
}

#[derive(Default)]
struct MutationMap {
    mutations: BTreeMap<MutationKey, RowMutation>,
}

impl MutationMap {
    fn put(&mut self, address: RowAddress, value: Vec<u8>) {
        self.mutations
            .insert(mutation_key(&address), RowMutation::put(address, value));
    }

    fn delete(&mut self, address: RowAddress) {
        self.mutations
            .insert(mutation_key(&address), RowMutation::delete(address));
    }

    fn into_mutations(self) -> Vec<RowMutation> {
        self.mutations.into_values().collect()
    }
}

fn mutation_key(address: &RowAddress) -> MutationKey {
    (address.row_class(), address.key().to_vec())
}

fn edge_identity(edge: &GraphEdgeRecord) -> EdgeIdentity {
    (
        edge.src().clone(),
        edge.edge_type().clone(),
        edge.dst().clone(),
    )
}

fn neighbor_cursor(hit: &GraphNeighbor) -> String {
    let direction = match hit.direction() {
        GraphDirection::Outgoing => "o",
        GraphDirection::Incoming => "i",
        GraphDirection::Both => "b",
    };
    format!(
        "{}\u{1f}{}\u{1f}{}\u{1f}{}",
        direction,
        hit.edge().edge_type().as_str(),
        hit.node().node_id().as_str(),
        hit.edge().dst().as_str()
    )
}

fn binding_cursor(binding: &GraphBinding) -> String {
    format!(
        "{}\u{1f}{}",
        binding.graph().as_str(),
        binding.node_id().as_str()
    )
}

fn binding_from_index_row(
    space: &ProductSpace,
    row: &PersistenceReadRow,
) -> EngineResult<GraphBinding> {
    let (target, graph, node_id) = decode_graph_binding_key(space, row.key())?;
    let value = row.value().ok_or_else(|| {
        EngineError::corruption(
            "data_loss.engine.graph_binding_record",
            "stored graph binding row is missing a value",
        )
    })?;
    let record = decode_graph_binding_record(&graph, &node_id, value)?;
    if &target != record.binding().target() {
        return Err(EngineError::corruption(
            "data_loss.engine.graph_binding_record",
            "stored graph binding target does not match its row key",
        ));
    }
    Ok(GraphBinding::new(
        graph,
        node_id,
        record.binding().clone(),
        row.commit_version(),
        row.commit_timestamp(),
    ))
}

#[cfg(test)]
mod tests {
    use super::{binding_from_index_row, neighbor_cursor};
    use crate::data::graph::{
        encode_graph_binding_record, GraphBindingPrimitive, GraphBindingRecord, GraphBindingTarget,
        GraphDirection, GraphEdge, GraphEdgeData, GraphEdgeType, GraphEntityBinding, GraphName,
        GraphNeighbor, GraphNode, GraphNodeData, GraphNodeId,
    };
    use crate::data::kv::ProductSpace;
    use crate::diagnostics::EngineErrorClass;
    use crate::persistence::{encode_graph_binding_key, PersistenceReadRow};
    use strata_core_next::{CommitVersion, Timestamp};

    #[test]
    fn neighbor_cursor_orders_direction_and_identity() {
        let graph = GraphName::new("deps").expect("graph");
        let edge_type = GraphEdgeType::new("links").expect("edge type");
        let node_a = GraphNodeId::new("a").expect("node");
        let node_b = GraphNodeId::new("b").expect("node");
        let node = GraphNode::new(
            graph.clone(),
            node_b.clone(),
            GraphNodeData::default(),
            CommitVersion::new(1),
            Timestamp::from_micros(1),
        );
        let edge = GraphEdge::new(
            graph,
            node_a,
            edge_type,
            node_b,
            GraphEdgeData::default(),
            CommitVersion::new(1),
            Timestamp::from_micros(1),
        );
        let hit = GraphNeighbor::new(node, edge, GraphDirection::Outgoing);
        assert!(neighbor_cursor(&hit).starts_with("o\u{1f}links"));
    }

    #[test]
    fn binding_index_row_rejects_target_mismatch() {
        let space = ProductSpace::new("default").expect("space");
        let graph = GraphName::new("deps").expect("graph");
        let node_id = GraphNodeId::new("doc").expect("node");
        let key_target = GraphBindingTarget::new(
            GraphBindingPrimitive::Json,
            None,
            ProductSpace::new("docs").expect("space"),
            "doc-a",
        )
        .expect("key target");
        let stored_target = GraphBindingTarget::new(
            GraphBindingPrimitive::Json,
            None,
            ProductSpace::new("docs").expect("space"),
            "doc-b",
        )
        .expect("stored target");
        let binding = GraphEntityBinding::new(stored_target);
        let record = GraphBindingRecord::new(graph.clone(), node_id.clone(), binding);
        let row = PersistenceReadRow::for_test(
            encode_graph_binding_key(&space, &key_target, &graph, &node_id),
            Some(encode_graph_binding_record(&record).expect("record encodes")),
            false,
        );

        let error = binding_from_index_row(&space, &row).expect_err("target mismatch rejected");
        assert_eq!(error.class(), EngineErrorClass::Corruption);
        assert_eq!(error.code(), "data_loss.engine.graph_binding_record");
    }
}
