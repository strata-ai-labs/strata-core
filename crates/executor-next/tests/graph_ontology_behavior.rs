//! Executor graph ontology command behavior tests (GO4): the 8 ontology
//! commands end-to-end, engine error codes surfacing through the executor,
//! and node object types round-tripping the wire DTOs.

#![allow(clippy::result_large_err, clippy::too_many_lines)]

use serde_json::json;
use strata_executor_next::{Executor, GraphObjectTypeDefData, GraphPropertyDef, Output};
use tempfile::TempDir;

fn run_modes(mut exercise: impl FnMut(&mut Executor)) {
    let mut cache = Executor::open_cache().expect("cache executor opens");
    exercise(&mut cache);

    let temp = TempDir::new().expect("temp dir");
    let path = temp.path().join("db");
    let mut durable = Executor::open_durable_local(&path).expect("durable executor opens");
    exercise(&mut durable);
}

fn title_property() -> std::collections::BTreeMap<String, GraphPropertyDef> {
    [(
        "title".to_owned(),
        GraphPropertyDef::new(Some("string".to_owned()), true),
    )]
    .into_iter()
    .collect()
}

#[test]
fn ontology_command_lifecycle_runs_in_cache_and_durable_modes() {
    run_modes(exercise_ontology_lifecycle);
}

fn exercise_ontology_lifecycle(executor: &mut Executor) {
    executor.graph_create("deps").expect("graph created");

    // No ontology before any definition.
    let Output::GraphOntologyResult(None) =
        executor.graph_get_ontology("deps").expect("read succeeds")
    else {
        panic!("expected empty ontology result");
    };

    // Define types; first definition is created, redefinition is not.
    let Output::GraphOntologyWriteResult { created, kind, .. } = executor
        .graph_define_object_type("deps", "Document", title_property())
        .expect("define succeeds")
    else {
        panic!("expected ontology write result");
    };
    assert!(created);
    assert_eq!(kind, "object");
    let Output::GraphOntologyWriteResult { created, .. } = executor
        .graph_define_object_type("deps", "Document", title_property())
        .expect("redefine succeeds")
    else {
        panic!("expected ontology write result");
    };
    assert!(!created, "draft redefinition is not new");

    executor
        .graph_define_object_type("deps", "Author", title_property())
        .expect("author type");
    executor
        .graph_define_link_type(
            "deps",
            "wrote",
            "Author",
            "Document",
            Some("one-to-many".to_owned()),
        )
        .expect("link type");

    // The canonical read returns status plus every definition.
    let Output::GraphOntologyResult(Some(ontology)) =
        executor.graph_get_ontology("deps").expect("read succeeds")
    else {
        panic!("expected ontology");
    };
    assert_eq!(ontology.status(), "draft");
    let names: Vec<&str> = ontology
        .object_types()
        .iter()
        .map(GraphObjectTypeDefData::name)
        .collect();
    assert_eq!(names, vec!["Author", "Document"]);
    assert_eq!(ontology.link_types().len(), 1);
    assert_eq!(ontology.link_types()[0].cardinality(), Some("one-to-many"));

    // Draft deletes report honestly; freeze validates and locks.
    let Output::GraphOntologyDeleteResult { deleted, .. } = executor
        .graph_delete_object_type("deps", "Author")
        .expect("delete succeeds")
    else {
        panic!("expected ontology delete result");
    };
    assert!(deleted);
    let freeze_error = executor
        .graph_freeze_ontology("deps")
        .expect_err("dangling link endpoint cannot freeze");
    assert_eq!(
        freeze_error.code(),
        "failed_precondition.engine.graph_ontology_freeze"
    );
    executor
        .graph_define_object_type("deps", "Author", title_property())
        .expect("restore type");
    let Output::GraphOntologyFreezeResult {
        object_types,
        link_types,
        ..
    } = executor
        .graph_freeze_ontology("deps")
        .expect("freeze succeeds")
    else {
        panic!("expected freeze result");
    };
    assert_eq!(object_types, 2);
    assert_eq!(link_types, 1);

    let frozen_error = executor
        .graph_define_object_type("deps", "Reviewer", title_property())
        .expect_err("frozen ontology refuses definitions");
    assert_eq!(
        frozen_error.code(),
        "failed_precondition.engine.graph_ontology_frozen"
    );
}

#[test]
fn typed_nodes_enforce_and_query_through_commands() {
    run_modes(exercise_typed_nodes);
}

fn exercise_typed_nodes(executor: &mut Executor) {
    executor.graph_create("deps").expect("graph created");
    executor
        .graph_define_object_type("deps", "Document", title_property())
        .expect("document type");
    executor
        .graph_define_object_type("deps", "Author", title_property())
        .expect("author type");
    executor
        .graph_define_link_type("deps", "wrote", "Author", "Document", None)
        .expect("link type");
    executor.graph_freeze_ontology("deps").expect("freeze");

    // Typed writes validate through the command surface.
    executor
        .graph_add_typed_node("deps", "d1", "Document", Some(json!({"title": "Spec"})))
        .expect("typed node");
    executor
        .graph_add_typed_node("deps", "d2", "Document", Some(json!({"title": "Plan"})))
        .expect("typed node");
    let error = executor
        .graph_add_typed_node("deps", "x1", "Reviewer", Some(json!({"title": "T"})))
        .expect_err("undeclared type refuses");
    assert_eq!(
        error.code(),
        "failed_precondition.engine.graph_ontology_node_type"
    );
    let error = executor
        .graph_add_typed_node("deps", "d3", "Document", None)
        .expect_err("missing required property refuses");
    assert_eq!(
        error.code(),
        "failed_precondition.engine.graph_ontology_required_property"
    );
    executor
        .graph_add_node("deps", "free", None, None)
        .expect("untyped node still passes");

    // The object type round-trips node reads.
    let Output::GraphNodeResult(Some(node)) = executor
        .graph_get_node("deps", "d1")
        .expect("read succeeds")
    else {
        panic!("expected node");
    };
    assert_eq!(node.object_type(), Some("Document"));

    // nodes-by-type pages through the index.
    let Output::GraphNodePage { items, page } = executor
        .graph_nodes_by_type("deps", "Document", None, Some(1))
        .expect("first page")
    else {
        panic!("expected node page");
    };
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].node_id(), "d1");
    assert!(page.has_more());
    let Output::GraphNodePage { items, page } = executor
        .graph_nodes_by_type("deps", "Document", page.cursor().cloned(), Some(5))
        .expect("second page")
    else {
        panic!("expected node page");
    };
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].node_id(), "d2");
    assert!(!page.has_more());

    // The summary reports exact usage counts.
    let Output::GraphOntologySummaryResult(Some(summary)) = executor
        .graph_ontology_summary("deps")
        .expect("summary succeeds")
    else {
        panic!("expected summary");
    };
    assert_eq!(summary.status(), "frozen");
    let counts: Vec<(&str, u64)> = summary
        .object_types()
        .iter()
        .map(|entry| (entry.def().name(), entry.node_count()))
        .collect();
    assert_eq!(counts, vec![("Author", 0), ("Document", 2)]);
    assert_eq!(summary.link_types()[0].edge_count(), 0);
}
