use super::command_cases::{all_commands, command_round_trip_cases};
use super::output_cases::all_outputs;
use super::support::{Command, Output};

#[test]
fn every_command_round_trips_through_json() {
    for command in command_round_trip_cases() {
        let encoded = serde_json::to_string(&command).expect("command serializes");
        let decoded: Command = serde_json::from_str(&encoded).expect("command deserializes");
        assert_eq!(decoded, command);
    }
}

#[test]
fn every_output_round_trips_through_json() {
    for output in all_outputs() {
        let encoded = serde_json::to_string(&output).expect("output serializes");
        let decoded: Output = serde_json::from_str(&encoded).expect("output deserializes");
        assert_eq!(decoded, output);
    }
}

#[test]
#[allow(clippy::too_many_lines)]
fn command_names_cover_every_variant() {
    let names = all_commands()
        .into_iter()
        .map(|command| command.name())
        .collect::<Vec<_>>();

    assert_eq!(
        names,
        vec![
            "ping",
            "info",
            "health",
            "metrics",
            "describe",
            "config_get",
            "configure_get_key",
            "space_list",
            "space_create",
            "space_exists",
            "space_delete",
            "hub_info",
            "hub_list_datasets",
            "hub_get_dataset",
            "hub_list_refs",
            "hub_list_yanked",
            "branch_list",
            "branch_get",
            "branch_create",
            "branch_fork_current",
            "branch_fork_at_version",
            "branch_fork_at_timestamp",
            "branch_delete",
            "kv_put",
            "kv_get",
            "kv_delete",
            "kv_list",
            "kv_scan",
            "kv_batch_put",
            "kv_batch_get",
            "kv_batch_delete",
            "kv_batch_exists",
            "kv_exists",
            "kv_history",
            "kv_count",
            "kv_sample",
            "json_set",
            "json_get",
            "json_delete",
            "json_history",
            "json_exists",
            "json_batch_exists",
            "json_batch_set",
            "json_batch_get",
            "json_batch_delete",
            "json_list",
            "json_scan",
            "json_count",
            "json_sample",
            "json_create_index",
            "json_drop_index",
            "json_list_indexes",
            "vector_create_collection",
            "vector_delete_collection",
            "vector_list_collections",
            "vector_collection_stats",
            "vector_count",
            "vector_upsert",
            "vector_get",
            "vector_history",
            "vector_exists",
            "vector_batch_exists",
            "vector_list_keys",
            "vector_scan",
            "vector_update_metadata",
            "vector_delete",
            "vector_delete_by_filter",
            "vector_delete_all",
            "vector_query",
            "vector_index_query",
            "vector_batch_upsert",
            "vector_batch_get",
            "vector_batch_delete",
            "event_batch_append",
            "event_append",
            "event_get",
            "event_exists",
            "event_count",
            "event_range",
            "event_range_by_time",
            "event_list_types",
            "event_list",
            "event_verify_chain",
            "graph_create",
            "graph_delete",
            "graph_list",
            "graph_get_meta",
            "graph_add_node",
            "graph_get_node",
            "graph_remove_node",
            "graph_list_nodes",
            "graph_add_edge",
            "graph_get_edge",
            "graph_remove_edge",
            "graph_neighbors",
            "graph_bindings_for_entity",
            "graph_batch_write",
            "graph_define_object_type",
            "graph_define_link_type",
            "graph_delete_object_type",
            "graph_delete_link_type",
            "graph_freeze_ontology",
            "graph_get_ontology",
            "graph_ontology_summary",
            "graph_nodes_by_type",
            "graph_wcc",
            "graph_lcc",
            "graph_sssp",
            "graph_pagerank",
            "graph_cdlp",
            "graph_bfs",
            "graph_apply_delete_policy",
            "graph_bulk_insert",
            "arrow_import",
            "arrow_export",
        ]
    );
}
