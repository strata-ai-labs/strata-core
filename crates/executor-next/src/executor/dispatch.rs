use super::{Command, Executor, ExecutorError, ExecutorResult, Output, PageInfo};

impl Executor {
    /// Executes one serialized command.
    pub fn execute(&mut self, command: Command) -> ExecutorResult<Output> {
        match command {
            Command::Ping => self.execute_ping(),
            Command::Info { branch } => self.execute_info(branch.as_deref()),
            Command::Health { branch } => self.execute_health(branch.as_deref()),
            Command::Metrics { branch } => self.execute_metrics(branch.as_deref()),
            Command::Describe { branch } => self.execute_describe(branch.as_deref()),
            Command::ConfigGet => self.execute_config_get(),
            Command::ConfigureGetKey { key } => self.execute_configure_get_key(&key),
            Command::SpaceList { branch } => self.execute_space_list(branch.as_deref()),
            Command::SpaceCreate { branch, space } => {
                self.execute_space_create(branch.as_deref(), &space)
            }
            Command::SpaceExists { branch, space } => {
                self.execute_space_exists(branch.as_deref(), &space)
            }
            Command::SpaceDelete {
                branch,
                space,
                force,
            } => self.execute_space_delete(branch.as_deref(), &space, force),
            Command::BranchList => self.execute_branch_list(),
            Command::BranchGet { branch } => self.execute_branch_get(&branch),
            Command::BranchCreate { branch } => self.execute_branch_create(&branch),
            Command::BranchForkCurrent { source, branch } => {
                self.execute_branch_fork_current(&source, &branch)
            }
            Command::BranchForkAtVersion {
                source,
                branch,
                version,
            } => self.execute_branch_fork_at_version(&source, &branch, version),
            Command::BranchForkAtTimestamp {
                source,
                branch,
                timestamp,
            } => self.execute_branch_fork_at_timestamp(&source, &branch, timestamp),
            Command::BranchDelete { branch } => self.execute_branch_delete(&branch),
            Command::KvPut {
                branch,
                space,
                key,
                value,
            } => self.execute_kv_put(branch.as_deref(), space.as_deref(), key, value),
            Command::KvGet {
                branch,
                space,
                key,
                as_of,
            } => self.execute_kv_get(branch.as_deref(), space.as_deref(), key, as_of),
            Command::KvDelete { branch, space, key } => {
                self.execute_kv_delete(branch.as_deref(), space.as_deref(), key)
            }
            Command::KvList {
                branch,
                space,
                prefix,
                cursor,
                limit,
                as_of,
            } => self.execute_kv_list(
                branch.as_deref(),
                space.as_deref(),
                prefix,
                cursor,
                limit,
                as_of,
            ),
            Command::KvScan {
                branch,
                space,
                start,
                limit,
            } => self.execute_kv_scan(branch.as_deref(), space.as_deref(), start, limit),
            Command::KvBatchPut {
                branch,
                space,
                entries,
            } => self.execute_kv_batch_put(branch.as_deref(), space.as_deref(), entries),
            Command::KvBatchGet {
                branch,
                space,
                keys,
            } => self.execute_kv_batch_get(branch.as_deref(), space.as_deref(), keys),
            Command::KvBatchDelete {
                branch,
                space,
                keys,
            } => self.execute_kv_batch_delete(branch.as_deref(), space.as_deref(), keys),
            Command::KvBatchExists {
                branch,
                space,
                keys,
            } => self.execute_kv_batch_exists(branch.as_deref(), space.as_deref(), keys),
            Command::KvExists { branch, space, key } => {
                self.execute_kv_exists(branch.as_deref(), space.as_deref(), key)
            }
            Command::KvGetv { branch, space, key } => {
                self.execute_kv_getv(branch.as_deref(), space.as_deref(), key)
            }
            Command::KvCount {
                branch,
                space,
                prefix,
            } => self.execute_kv_count(branch.as_deref(), space.as_deref(), prefix),
            Command::KvSample {
                branch,
                space,
                prefix,
                count,
            } => self.execute_kv_sample(branch.as_deref(), space.as_deref(), prefix, count),
            Command::JsonSet {
                branch,
                space,
                key,
                path,
                value,
            } => self.execute_json_set(branch.as_deref(), space.as_deref(), &key, &path, value),
            Command::JsonGet {
                branch,
                space,
                key,
                path,
                as_of,
            } => self.execute_json_get(branch.as_deref(), space.as_deref(), &key, &path, as_of),
            Command::JsonDelete {
                branch,
                space,
                key,
                path,
            } => self.execute_json_delete(branch.as_deref(), space.as_deref(), &key, &path),
            Command::JsonGetv { branch, space, key } => {
                self.execute_json_getv(branch.as_deref(), space.as_deref(), key)
            }
            Command::JsonExists { branch, space, key } => {
                self.execute_json_exists(branch.as_deref(), space.as_deref(), key)
            }
            Command::JsonBatchSet {
                branch,
                space,
                entries,
            } => self.execute_json_batch_set(branch.as_deref(), space.as_deref(), entries),
            Command::JsonBatchGet {
                branch,
                space,
                entries,
            } => self.execute_json_batch_get(branch.as_deref(), space.as_deref(), entries),
            Command::JsonBatchDelete {
                branch,
                space,
                entries,
            } => self.execute_json_batch_delete(branch.as_deref(), space.as_deref(), entries),
            Command::JsonList {
                branch,
                space,
                prefix,
                cursor,
                limit,
                as_of,
            } => self.execute_json_list(
                branch.as_deref(),
                space.as_deref(),
                prefix,
                cursor,
                limit,
                as_of,
            ),
            Command::JsonCount {
                branch,
                space,
                prefix,
            } => self.execute_json_count(branch.as_deref(), space.as_deref(), prefix),
            Command::JsonSample {
                branch,
                space,
                prefix,
                count,
            } => self.execute_json_sample(branch.as_deref(), space.as_deref(), prefix, count),
            Command::JsonCreateIndex {
                branch,
                space,
                name,
                field_path,
                index_type,
            } => self.execute_json_create_index(
                branch.as_deref(),
                space.as_deref(),
                name,
                &field_path,
                index_type,
            ),
            Command::JsonDropIndex {
                branch,
                space,
                name,
            } => self.execute_json_drop_index(branch.as_deref(), space.as_deref(), name),
            Command::JsonListIndexes { branch, space } => {
                self.execute_json_list_indexes(branch.as_deref(), space.as_deref())
            }
            Command::VectorCreateCollection {
                branch,
                space,
                collection,
                dimension,
                metric,
            } => self.execute_vector_create_collection(
                branch.as_deref(),
                space.as_deref(),
                collection,
                dimension,
                metric,
            ),
            Command::VectorDeleteCollection {
                branch,
                space,
                collection,
            } => self.execute_vector_delete_collection(
                branch.as_deref(),
                space.as_deref(),
                collection,
            ),
            Command::VectorListCollections { branch, space } => {
                self.execute_vector_list_collections(branch.as_deref(), space.as_deref())
            }
            Command::VectorCollectionStats {
                branch,
                space,
                collection,
            } => self.execute_vector_collection_stats(
                branch.as_deref(),
                space.as_deref(),
                collection,
            ),
            Command::VectorCount {
                branch,
                space,
                collection,
            } => self.execute_vector_count(branch.as_deref(), space.as_deref(), collection),
            Command::VectorUpsert {
                branch,
                space,
                collection,
                key,
                vector,
                metadata,
            } => self.execute_vector_upsert(
                branch.as_deref(),
                space.as_deref(),
                collection,
                key,
                vector,
                metadata,
            ),
            Command::VectorGet {
                branch,
                space,
                collection,
                key,
                as_of,
            } => {
                self.execute_vector_get(branch.as_deref(), space.as_deref(), collection, key, as_of)
            }
            Command::VectorGetv {
                branch,
                space,
                collection,
                key,
            } => self.execute_vector_getv(branch.as_deref(), space.as_deref(), collection, key),
            Command::VectorExists {
                branch,
                space,
                collection,
                key,
            } => self.execute_vector_exists(branch.as_deref(), space.as_deref(), collection, key),
            Command::VectorListKeys {
                branch,
                space,
                collection,
                prefix,
                cursor,
                limit,
            } => self.execute_vector_list_keys(
                branch.as_deref(),
                space.as_deref(),
                collection,
                prefix,
                cursor,
                limit,
            ),
            Command::VectorUpdateMetadata {
                branch,
                space,
                collection,
                key,
                patch,
            } => self.execute_vector_update_metadata(
                branch.as_deref(),
                space.as_deref(),
                collection,
                key,
                patch,
            ),
            Command::VectorDelete {
                branch,
                space,
                collection,
                key,
            } => self.execute_vector_delete(branch.as_deref(), space.as_deref(), collection, key),
            Command::VectorDeleteByFilter {
                branch,
                space,
                collection,
                filter,
            } => self.execute_vector_delete_by_filter(
                branch.as_deref(),
                space.as_deref(),
                collection,
                filter,
            ),
            Command::VectorDeleteAll {
                branch,
                space,
                collection,
            } => self.execute_vector_delete_all(branch.as_deref(), space.as_deref(), collection),
            Command::VectorQuery {
                branch,
                space,
                collection,
                query,
                k,
                filter,
                as_of,
            } => self.execute_vector_query(
                branch.as_deref(),
                space.as_deref(),
                collection,
                query,
                k,
                filter,
                as_of,
            ),
            Command::VectorIndexQuery {
                branch,
                space,
                collection,
                query,
                k,
                filter,
                as_of,
            } => self.execute_vector_index_query(
                branch.as_deref(),
                space.as_deref(),
                collection,
                query,
                k,
                filter,
                as_of,
            ),
            Command::VectorBatchUpsert {
                branch,
                space,
                collection,
                entries,
            } => self.execute_vector_batch_upsert(
                branch.as_deref(),
                space.as_deref(),
                collection,
                entries,
            ),
            Command::VectorBatchGet {
                branch,
                space,
                collection,
                keys,
            } => {
                self.execute_vector_batch_get(branch.as_deref(), space.as_deref(), collection, keys)
            }
            Command::VectorBatchDelete {
                branch,
                space,
                collection,
                keys,
            } => self.execute_vector_batch_delete(
                branch.as_deref(),
                space.as_deref(),
                collection,
                keys,
            ),
            Command::EventBatchAppend {
                branch,
                space,
                entries,
            } => self.execute_event_batch_append(branch.as_deref(), space.as_deref(), entries),
            Command::EventAppend {
                branch,
                space,
                event_type,
                payload,
            } => {
                self.execute_event_append(branch.as_deref(), space.as_deref(), event_type, payload)
            }
            Command::EventGet {
                branch,
                space,
                sequence,
                as_of,
            } => self.execute_event_get(branch.as_deref(), space.as_deref(), sequence, as_of),
            Command::EventExists {
                branch,
                space,
                sequence,
            } => self.execute_event_exists(branch.as_deref(), space.as_deref(), sequence),
            Command::EventGetByType {
                branch,
                space,
                event_type,
                limit,
                after_sequence,
                as_of,
            } => self.execute_event_get_by_type(
                branch.as_deref(),
                space.as_deref(),
                event_type,
                limit,
                after_sequence,
                as_of,
            ),
            Command::EventLen {
                branch,
                space,
                as_of,
            } => self.execute_event_len(branch.as_deref(), space.as_deref(), as_of),
            Command::EventRange {
                branch,
                space,
                start_seq,
                end_seq,
                limit,
                direction,
                event_type,
            } => self.execute_event_range(
                branch.as_deref(),
                space.as_deref(),
                start_seq,
                end_seq,
                limit,
                direction,
                event_type,
            ),
            Command::EventRangeByTime {
                branch,
                space,
                start_ts,
                end_ts,
                limit,
                direction,
                event_type,
            } => self.execute_event_range_by_time(
                branch.as_deref(),
                space.as_deref(),
                start_ts,
                end_ts,
                limit,
                direction,
                event_type,
            ),
            Command::EventListTypes {
                branch,
                space,
                as_of,
            } => self.execute_event_list_types(branch.as_deref(), space.as_deref(), as_of),
            Command::EventList {
                branch,
                space,
                event_type,
                limit,
                as_of,
            } => self.execute_event_list(
                branch.as_deref(),
                space.as_deref(),
                event_type,
                limit,
                as_of,
            ),
            Command::EventVerifyChain { branch, space } => {
                self.execute_event_verify_chain(branch.as_deref(), space.as_deref())
            }
            Command::GraphCreate {
                branch,
                space,
                graph,
            } => self.execute_graph_create(branch.as_deref(), space.as_deref(), graph),
            Command::GraphDelete {
                branch,
                space,
                graph,
            } => self.execute_graph_delete(branch.as_deref(), space.as_deref(), graph),
            Command::GraphList {
                branch,
                space,
                cursor,
                limit,
            } => self.execute_graph_list(branch.as_deref(), space.as_deref(), cursor, limit),
            Command::GraphGetMeta {
                branch,
                space,
                graph,
            } => self.execute_graph_get_meta(branch.as_deref(), space.as_deref(), graph),
            Command::GraphAddNode {
                branch,
                space,
                graph,
                node_id,
                properties,
                binding,
            } => self.execute_graph_add_node(
                branch.as_deref(),
                space.as_deref(),
                graph,
                node_id,
                properties,
                binding,
            ),
            Command::GraphGetNode {
                branch,
                space,
                graph,
                node_id,
            } => self.execute_graph_get_node(branch.as_deref(), space.as_deref(), graph, node_id),
            Command::GraphRemoveNode {
                branch,
                space,
                graph,
                node_id,
            } => {
                self.execute_graph_remove_node(branch.as_deref(), space.as_deref(), graph, node_id)
            }
            Command::GraphListNodes {
                branch,
                space,
                graph,
                prefix,
                cursor,
                limit,
            } => self.execute_graph_list_nodes(
                branch.as_deref(),
                space.as_deref(),
                graph,
                prefix,
                cursor,
                limit,
            ),
            Command::GraphAddEdge {
                branch,
                space,
                graph,
                src,
                edge_type,
                dst,
                weight,
                properties,
            } => self.execute_graph_add_edge(
                branch.as_deref(),
                space.as_deref(),
                graph,
                src,
                edge_type,
                dst,
                weight,
                properties,
            ),
            Command::GraphGetEdge {
                branch,
                space,
                graph,
                src,
                edge_type,
                dst,
            } => self.execute_graph_get_edge(
                branch.as_deref(),
                space.as_deref(),
                graph,
                src,
                edge_type,
                dst,
            ),
            Command::GraphRemoveEdge {
                branch,
                space,
                graph,
                src,
                edge_type,
                dst,
            } => self.execute_graph_remove_edge(
                branch.as_deref(),
                space.as_deref(),
                graph,
                src,
                edge_type,
                dst,
            ),
            Command::GraphNeighbors {
                branch,
                space,
                graph,
                node_id,
                direction,
                edge_type,
                cursor,
                limit,
            } => self.execute_graph_neighbors(
                branch.as_deref(),
                space.as_deref(),
                graph,
                node_id,
                direction,
                edge_type,
                cursor.as_deref(),
                limit,
            ),
            Command::GraphBindingsForEntity {
                branch,
                space,
                target,
                cursor,
                limit,
            } => self.execute_graph_bindings_for_entity(
                branch.as_deref(),
                space.as_deref(),
                target,
                cursor.as_deref(),
                limit,
            ),
            Command::GraphBatchWrite {
                branch,
                space,
                graph,
                operations,
            } => self.execute_graph_batch_write(
                branch.as_deref(),
                space.as_deref(),
                graph,
                operations,
            ),
            Command::ArrowImport {
                branch,
                space,
                file_path,
                format,
                target,
                key_column,
                value_column,
                collection,
            } => self.execute_arrow_import(
                branch.as_deref(),
                space.as_deref(),
                file_path,
                format,
                target,
                key_column.as_deref(),
                value_column.as_deref(),
                collection.as_deref(),
            ),
            Command::ArrowExport {
                branch,
                space,
                primitive,
                format,
                path,
                prefix,
                limit,
                collection,
                graph,
                event_type,
            } => self.execute_arrow_export(
                branch.as_deref(),
                space.as_deref(),
                primitive,
                format,
                path,
                prefix.as_deref(),
                limit,
                collection,
                graph,
                event_type,
            ),
            #[cfg(feature = "inference")]
            Command::InferenceModelsList => Ok(Output::InferenceModels {
                items: self.inference.list_models(),
                page: PageInfo::terminal(),
            }),
            #[cfg(feature = "inference")]
            Command::InferenceModelsLocal => Ok(Output::InferenceModels {
                items: self.inference.list_local_models(),
                page: PageInfo::terminal(),
            }),
            #[cfg(feature = "inference")]
            Command::InferenceModelsPull { model } => self
                .inference
                .pull_model(&model)
                .map(Output::InferenceModelPulled)
                .map_err(ExecutorError::from),
            #[cfg(feature = "inference")]
            Command::InferenceModelCapability { model } => self
                .inference
                .capability(&model)
                .map(Output::InferenceCapability)
                .map_err(ExecutorError::from),
            #[cfg(feature = "inference")]
            Command::InferenceGenerate { model, request } => self
                .inference
                .generate(&model, &request)
                .map(Output::InferenceGeneration)
                .map_err(ExecutorError::from),
            #[cfg(feature = "inference")]
            Command::InferenceTokenize {
                model,
                text,
                add_special,
            } => self
                .inference
                .tokenize(&model, &text, add_special)
                .map(Output::InferenceTokenIds)
                .map_err(ExecutorError::from),
            #[cfg(feature = "inference")]
            Command::InferenceDetokenize { model, ids } => self
                .inference
                .detokenize(&model, &ids)
                .map(Output::InferenceText)
                .map_err(ExecutorError::from),
            #[cfg(feature = "inference")]
            Command::InferenceEmbed { model, request } => self
                .inference
                .embed(&model, &request)
                .map(Output::InferenceEmbedding)
                .map_err(ExecutorError::from),
            #[cfg(feature = "inference")]
            Command::InferenceEmbedBatch { model, texts } => self
                .inference
                .embed_batch(&model, &texts)
                .map(Output::InferenceEmbeddings)
                .map_err(ExecutorError::from),
            #[cfg(feature = "inference")]
            Command::InferenceRank { model, request } => self
                .inference
                .rank(&model, &request)
                .map(Output::InferenceRanking)
                .map_err(ExecutorError::from),
            #[cfg(feature = "inference")]
            Command::InferenceUnload { model } => self
                .inference
                .unload(model.as_deref())
                .map(|unloaded| Output::InferenceUnloadResult { unloaded })
                .map_err(ExecutorError::from),
            #[cfg(feature = "inference")]
            Command::InferenceCacheStatus => self
                .inference
                .cache_status()
                .map(Output::InferenceCacheStatus)
                .map_err(ExecutorError::from),
        }
    }
}
