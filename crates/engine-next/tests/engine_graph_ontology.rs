//! Graph ontology conformance tests (GO1): draft/freeze lifecycle,
//! freeze-time validation, frozen immutability, temporal reads, and
//! space isolation with durable reopen.

mod common;

use strata_engine_next::{
    Database, GraphLinkTypeDef, GraphName, GraphObjectTypeDef, GraphOntologyStatus,
    GraphPropertyDef, GraphTypeName,
};

use common::{branch, open_cache_database, open_durable_database, space};

fn run_database_modes(exercise: fn(Database)) {
    exercise(open_cache_database().expect("cache open succeeds"));

    let tempdir = tempfile::tempdir().expect("tempdir");
    exercise(open_durable_database(tempdir.path()).expect("durable open succeeds"));
}

fn graph_service<'a>(
    database: &'a mut Database,
    branch_name: &str,
    space_name: &str,
) -> strata_engine_next::GraphService<'a> {
    database
        .graph(branch(branch_name), space(space_name))
        .expect("graph service opens")
}

fn graph_name(value: &str) -> GraphName {
    GraphName::new(value).expect("valid graph")
}

fn type_name(value: &str) -> GraphTypeName {
    GraphTypeName::new(value).expect("valid type name")
}

fn object_type(name: &str) -> GraphObjectTypeDef {
    GraphObjectTypeDef::new(
        type_name(name),
        [(
            "title".to_owned(),
            GraphPropertyDef::new(Some("string".to_owned()), true).expect("property"),
        )],
    )
    .expect("object type")
}

fn link_type(name: &str, source: &str, target: &str) -> GraphLinkTypeDef {
    GraphLinkTypeDef::new(
        type_name(name),
        type_name(source),
        type_name(target),
        Some("one-to-many".to_owned()),
        [],
    )
    .expect("link type")
}

#[test]
fn graph_ontology_lifecycle_runs_in_cache_and_durable_modes() {
    run_database_modes(exercise_ontology_lifecycle);
}

fn exercise_ontology_lifecycle(mut database: Database) {
    let mut graph = graph_service(&mut database, "default", "default");
    let name = graph_name("deps");
    graph.create_graph(name.clone()).expect("graph created");

    assert!(
        graph.ontology(&name).expect("read succeeds").is_none(),
        "no ontology before any definition"
    );

    // Definitions are upserts while Draft.
    let created = graph
        .define_object_type(&name, object_type("Document"))
        .expect("define succeeds");
    assert!(created.created(), "first definition is new");
    let redefined = graph
        .define_object_type(&name, object_type("Document"))
        .expect("redefine succeeds while draft");
    assert!(!redefined.created(), "redefinition is not new");
    graph
        .define_object_type(&name, object_type("Author"))
        .expect("second type");
    graph
        .define_link_type(&name, link_type("wrote", "Author", "Document"))
        .expect("link type");

    let ontology = graph
        .ontology(&name)
        .expect("read succeeds")
        .expect("ontology visible");
    assert_eq!(ontology.status(), GraphOntologyStatus::Draft);
    let object_names: Vec<&str> = ontology
        .object_types()
        .iter()
        .map(|def| def.name().as_str())
        .collect();
    assert_eq!(object_names, vec!["Author", "Document"], "sorted by name");
    assert_eq!(ontology.link_types().len(), 1);
    let link = &ontology.link_types()[0];
    assert_eq!(link.source().as_str(), "Author");
    assert_eq!(link.target().as_str(), "Document");
    assert_eq!(link.cardinality(), Some("one-to-many"));

    // Draft deletes work and report absence honestly.
    let deleted = graph
        .delete_object_type(&name, &type_name("Author"))
        .expect("delete succeeds");
    assert!(deleted.deleted());
    let missing = graph
        .delete_object_type(&name, &type_name("Author"))
        .expect("second delete succeeds");
    assert!(!missing.deleted(), "absent type reports deleted=false");

    // Restore and freeze.
    graph
        .define_object_type(&name, object_type("Author"))
        .expect("restore type");
    let frozen = graph.freeze_ontology(&name).expect("freeze succeeds");
    assert_eq!(frozen.object_types(), 2);
    assert_eq!(frozen.link_types(), 1);
    let ontology = graph
        .ontology(&name)
        .expect("read succeeds")
        .expect("ontology visible");
    assert_eq!(ontology.status(), GraphOntologyStatus::Frozen);

    // Frozen ontologies are immutable — every mutation refuses.
    let error = graph
        .define_object_type(&name, object_type("Reviewer"))
        .expect_err("define after freeze refuses");
    assert_eq!(
        error.code(),
        "failed_precondition.engine.graph_ontology_frozen"
    );
    let error = graph
        .delete_link_type(&name, &type_name("wrote"))
        .expect_err("delete after freeze refuses");
    assert_eq!(
        error.code(),
        "failed_precondition.engine.graph_ontology_frozen"
    );
    let error = graph
        .freeze_ontology(&name)
        .expect_err("second freeze refuses");
    assert_eq!(
        error.code(),
        "failed_precondition.engine.graph_ontology_frozen"
    );
}

#[test]
fn graph_ontology_freeze_validation_and_refusals_run_in_cache_and_durable_modes() {
    run_database_modes(exercise_freeze_validation_and_refusals);
}

fn exercise_freeze_validation_and_refusals(mut database: Database) {
    let mut graph = graph_service(&mut database, "default", "default");
    let name = graph_name("deps");

    // Every ontology call requires the graph to exist.
    let error = graph.ontology(&name).expect_err("missing graph refuses");
    assert_eq!(error.code(), "not_found.engine.graph");
    let error = graph
        .define_object_type(&name, object_type("Document"))
        .expect_err("missing graph refuses");
    assert_eq!(error.code(), "not_found.engine.graph");

    graph.create_graph(name.clone()).expect("graph created");

    // Freezing nothing refuses.
    let error = graph
        .freeze_ontology(&name)
        .expect_err("empty ontology cannot freeze");
    assert_eq!(
        error.code(),
        "failed_precondition.engine.graph_ontology_freeze"
    );

    // A link endpoint must reference a declared object type.
    graph
        .define_link_type(&name, link_type("wrote", "Author", "Document"))
        .expect("link type in draft");
    let error = graph
        .freeze_ontology(&name)
        .expect_err("dangling endpoint cannot freeze");
    assert_eq!(
        error.code(),
        "failed_precondition.engine.graph_ontology_freeze"
    );

    // Declaring the endpoints repairs the freeze.
    graph
        .define_object_type(&name, object_type("Author"))
        .expect("author type");
    graph
        .define_object_type(&name, object_type("Document"))
        .expect("document type");
    assert!(graph.freeze_ontology(&name).is_ok(), "freeze repaired");
}

#[test]
fn graph_ontology_temporal_reads_track_the_freeze_in_cache_and_durable_modes() {
    run_database_modes(exercise_temporal_reads);
}

fn exercise_temporal_reads(mut database: Database) {
    let mut graph = graph_service(&mut database, "default", "default");
    let name = graph_name("deps");
    graph.create_graph(name.clone()).expect("graph created");

    let defined = graph
        .define_object_type(&name, object_type("Document"))
        .expect("define succeeds");
    let draft_version = defined.commit().version();
    let frozen = graph.freeze_ontology(&name).expect("freeze succeeds");
    let frozen_version = frozen.commit().version();

    let at_draft = graph
        .ontology_at_version(&name, draft_version)
        .expect("read succeeds")
        .expect("ontology visible at draft version");
    assert_eq!(at_draft.status(), GraphOntologyStatus::Draft);
    assert_eq!(at_draft.version(), draft_version);

    let at_freeze = graph
        .ontology_at_version(&name, frozen_version)
        .expect("read succeeds")
        .expect("ontology visible at freeze version");
    assert_eq!(at_freeze.status(), GraphOntologyStatus::Frozen);

    let latest = graph
        .ontology(&name)
        .expect("read succeeds")
        .expect("ontology visible");
    assert_eq!(latest.status(), GraphOntologyStatus::Frozen);
    assert_eq!(latest.version(), frozen_version);
}

#[test]
fn graph_ontology_space_isolation_reopen_and_delete_cleanup_on_durable() {
    let tempdir = tempfile::tempdir().expect("tempdir");
    let name = graph_name("deps");
    {
        let mut database = open_durable_database(tempdir.path()).expect("durable open succeeds");
        let mut graph = graph_service(&mut database, "default", "docs");
        graph.create_graph(name.clone()).expect("graph created");
        graph
            .define_object_type(&name, object_type("Document"))
            .expect("define succeeds");
        graph.freeze_ontology(&name).expect("freeze succeeds");

        // The same graph name in another space has no ontology.
        let mut other = graph_service(&mut database, "default", "notes");
        other.create_graph(name.clone()).expect("graph created");
        assert!(
            other.ontology(&name).expect("read succeeds").is_none(),
            "ontology is space-isolated"
        );
        database.close().expect("close succeeds");
    }

    // Reopen: the frozen ontology survives durably.
    let mut database = open_durable_database(tempdir.path()).expect("durable reopen succeeds");
    let mut graph = graph_service(&mut database, "default", "docs");
    let ontology = graph
        .ontology(&name)
        .expect("read succeeds")
        .expect("ontology survives reopen");
    assert_eq!(ontology.status(), GraphOntologyStatus::Frozen);
    assert_eq!(ontology.object_types().len(), 1);

    // Deleting the graph removes its ontology: a recreated graph starts
    // with no ontology and a mutable draft.
    graph.delete_graph(&name).expect("delete succeeds");
    graph.create_graph(name.clone()).expect("recreate succeeds");
    assert!(
        graph.ontology(&name).expect("read succeeds").is_none(),
        "recreated graph has no stale ontology"
    );
    graph
        .define_object_type(&name, object_type("Fresh"))
        .expect("recreated graph accepts definitions");
}
