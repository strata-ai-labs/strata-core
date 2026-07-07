//! Shared data-capability conformance suite.
//!
//! One parameterized suite drives the behavior that every data capability must
//! share — branch resolution, branch fork inheritance and isolation, space
//! isolation, missing-branch and closed-runtime rejection, and durable reopen —
//! against KV, JSON, event, vector, and graph through a single fixture trait.
//! Capability-specific behavior (temporal models, path semantics, hash chains,
//! index artifacts) stays in the dedicated per-capability suites; this suite
//! exists so the five capabilities cannot drift apart on the shared skeleton.

#![allow(clippy::result_large_err)]

mod common;

use common::{assert_status, branch, key, open_cache_database, space, value};
use serde_json::json;
use strata_engine_next::{
    Database, EngineErrorClass, EngineResult, EventPayload, EventType, GraphName, GraphNodeData,
    GraphNodeId, JsonDocumentId, JsonPath, JsonValue, VectorCollectionName, VectorConfig,
    VectorDistanceMetric, VectorEmbedding, VectorKey, VectorMetadata,
};

/// A capability the shared conformance suite can exercise generically.
///
/// Each capability identifies a stored value by a `seed` byte. Capabilities that
/// need a container (vector collections, graphs) create it in [`ensure`]; the
/// others make it a no-op.
trait CapabilityFixture {
    /// Label used in assertion messages.
    fn label() -> &'static str;

    /// Creates any container the capability needs before writes. Idempotent.
    fn ensure(db: &mut Database, branch_name: &str, space_name: &str) -> EngineResult<()>;

    /// Writes a value identified by `seed` into `(branch, space)`.
    fn write(db: &mut Database, branch_name: &str, space_name: &str, seed: u8) -> EngineResult<()>;

    /// Returns whether the value for `seed` is visible at latest.
    fn exists(
        db: &mut Database,
        branch_name: &str,
        space_name: &str,
        seed: u8,
    ) -> EngineResult<bool>;

    /// Performs the cheapest read so `?` surfaces branch and runtime errors.
    fn probe(db: &mut Database, branch_name: &str, space_name: &str) -> EngineResult<()>;
}

struct KvFixture;

impl CapabilityFixture for KvFixture {
    fn label() -> &'static str {
        "kv"
    }

    fn ensure(_db: &mut Database, _branch_name: &str, _space_name: &str) -> EngineResult<()> {
        Ok(())
    }

    fn write(db: &mut Database, branch_name: &str, space_name: &str, seed: u8) -> EngineResult<()> {
        db.kv(branch(branch_name), space(space_name))?
            .put(key(&[seed]), value(&[seed]))?;
        Ok(())
    }

    fn exists(
        db: &mut Database,
        branch_name: &str,
        space_name: &str,
        seed: u8,
    ) -> EngineResult<bool> {
        Ok(db
            .kv(branch(branch_name), space(space_name))?
            .get(&key(&[seed]))?
            .is_some())
    }

    fn probe(db: &mut Database, branch_name: &str, space_name: &str) -> EngineResult<()> {
        db.kv(branch(branch_name), space(space_name))?
            .get(&key(b"probe"))?;
        Ok(())
    }
}

struct JsonFixture;

impl CapabilityFixture for JsonFixture {
    fn label() -> &'static str {
        "json"
    }

    fn ensure(_db: &mut Database, _branch_name: &str, _space_name: &str) -> EngineResult<()> {
        Ok(())
    }

    fn write(db: &mut Database, branch_name: &str, space_name: &str, seed: u8) -> EngineResult<()> {
        let id = JsonDocumentId::new(format!("doc-{seed}"))?;
        let document = JsonValue::new(json!({ "seed": seed }))?;
        db.json(branch(branch_name), space(space_name))?
            .create(id, document)?;
        Ok(())
    }

    fn exists(
        db: &mut Database,
        branch_name: &str,
        space_name: &str,
        seed: u8,
    ) -> EngineResult<bool> {
        let id = JsonDocumentId::new(format!("doc-{seed}"))?;
        db.json(branch(branch_name), space(space_name))?.exists(&id)
    }

    fn probe(db: &mut Database, branch_name: &str, space_name: &str) -> EngineResult<()> {
        let id = JsonDocumentId::new("probe")?;
        db.json(branch(branch_name), space(space_name))?
            .get(&id, &JsonPath::root())?;
        Ok(())
    }
}

struct EventFixture;

impl CapabilityFixture for EventFixture {
    fn label() -> &'static str {
        "event"
    }

    fn ensure(_db: &mut Database, _branch_name: &str, _space_name: &str) -> EngineResult<()> {
        Ok(())
    }

    fn write(db: &mut Database, branch_name: &str, space_name: &str, seed: u8) -> EngineResult<()> {
        // Events have no key, so encode the seed in the type (for server-side
        // filtering) and the payload (for unambiguous recovery in `exists`).
        let event_type = EventType::new(format!("conformance.seed.{seed}"))?;
        let payload = EventPayload::new(json!({ "seed": seed }))?;
        db.event(branch(branch_name), space(space_name))?
            .append(event_type, payload)?;
        Ok(())
    }

    fn exists(
        db: &mut Database,
        branch_name: &str,
        space_name: &str,
        seed: u8,
    ) -> EngineResult<bool> {
        let event_type = EventType::new(format!("conformance.seed.{seed}"))?;
        let records = db
            .event(branch(branch_name), space(space_name))?
            .get_by_type(&event_type, None, None)?;
        Ok(records.iter().any(|record| {
            record
                .payload()
                .as_inner()
                .get("seed")
                .and_then(serde_json::Value::as_u64)
                == Some(u64::from(seed))
        }))
    }

    fn probe(db: &mut Database, branch_name: &str, space_name: &str) -> EngineResult<()> {
        db.event(branch(branch_name), space(space_name))?.len()?;
        Ok(())
    }
}

const VECTOR_DIMENSION: usize = 2;

struct VectorFixture;

impl CapabilityFixture for VectorFixture {
    fn label() -> &'static str {
        "vector"
    }

    fn ensure(db: &mut Database, branch_name: &str, space_name: &str) -> EngineResult<()> {
        let name = VectorCollectionName::new("conformance")?;
        let config = VectorConfig::new(VECTOR_DIMENSION, VectorDistanceMetric::Cosine)?;
        match db
            .vector(branch(branch_name), space(space_name))?
            .create_collection(name, config)
        {
            Ok(_) => Ok(()),
            Err(error) if error.code() == "already_exists.engine.vector_collection" => Ok(()),
            Err(error) => Err(error),
        }
    }

    fn write(db: &mut Database, branch_name: &str, space_name: &str, seed: u8) -> EngineResult<()> {
        let name = VectorCollectionName::new("conformance")?;
        let vector_key = VectorKey::new(format!("k-{seed}"))?;
        let scalar = f32::from(seed) / 255.0;
        let embedding = VectorEmbedding::new(vec![scalar, 1.0 - scalar])?;
        let metadata = VectorMetadata::new(json!({ "seed": seed }))?;
        db.vector(branch(branch_name), space(space_name))?.upsert(
            name,
            vector_key,
            embedding,
            Some(metadata),
        )?;
        Ok(())
    }

    fn exists(
        db: &mut Database,
        branch_name: &str,
        space_name: &str,
        seed: u8,
    ) -> EngineResult<bool> {
        let name = VectorCollectionName::new("conformance")?;
        let vector_key = VectorKey::new(format!("k-{seed}"))?;
        db.vector(branch(branch_name), space(space_name))?
            .exists(&name, &vector_key)
    }

    fn probe(db: &mut Database, branch_name: &str, space_name: &str) -> EngineResult<()> {
        let name = VectorCollectionName::new("conformance")?;
        let vector_key = VectorKey::new("probe")?;
        db.vector(branch(branch_name), space(space_name))?
            .exists(&name, &vector_key)?;
        Ok(())
    }
}

struct GraphFixture;

impl CapabilityFixture for GraphFixture {
    fn label() -> &'static str {
        "graph"
    }

    fn ensure(db: &mut Database, branch_name: &str, space_name: &str) -> EngineResult<()> {
        let name = GraphName::new("conformance")?;
        match db
            .graph(branch(branch_name), space(space_name))?
            .create_graph(name)
        {
            Ok(_) => Ok(()),
            Err(error) if error.code() == "already_exists.engine.graph" => Ok(()),
            Err(error) => Err(error),
        }
    }

    fn write(db: &mut Database, branch_name: &str, space_name: &str, seed: u8) -> EngineResult<()> {
        let name = GraphName::new("conformance")?;
        let node_id = GraphNodeId::new(format!("n-{seed}"))?;
        db.graph(branch(branch_name), space(space_name))?
            .upsert_node(&name, node_id, GraphNodeData::new(None, None))?;
        Ok(())
    }

    fn exists(
        db: &mut Database,
        branch_name: &str,
        space_name: &str,
        seed: u8,
    ) -> EngineResult<bool> {
        let name = GraphName::new("conformance")?;
        let node_id = GraphNodeId::new(format!("n-{seed}"))?;
        Ok(db
            .graph(branch(branch_name), space(space_name))?
            .get_node(&name, &node_id)?
            .is_some())
    }

    fn probe(db: &mut Database, branch_name: &str, space_name: &str) -> EngineResult<()> {
        db.graph(branch(branch_name), space(space_name))?
            .list_graphs(None, 1)?;
        Ok(())
    }
}

fn write_then_read_visible<C: CapabilityFixture>() {
    let mut db = open_cache_database().expect("cache database opens");
    C::ensure(&mut db, "default", "default").expect("ensure container");
    C::write(&mut db, "default", "default", 1).expect("write");
    assert!(
        C::exists(&mut db, "default", "default", 1).expect("exists read"),
        "{}: a written value must be visible at latest",
        C::label()
    );
}

fn fork_inherits_and_isolates<C: CapabilityFixture>() {
    let mut db = open_cache_database().expect("cache database opens");
    C::ensure(&mut db, "default", "default").expect("ensure source container");
    C::write(&mut db, "default", "default", 1).expect("seed source");
    db.branches()
        .expect("branch service")
        .create_from_head(&branch("default"), branch("feature"))
        .expect("fork from head");
    assert!(
        C::exists(&mut db, "feature", "default", 1).expect("child read"),
        "{}: a fork must inherit the source value",
        C::label()
    );
    C::ensure(&mut db, "feature", "default").expect("ensure child container");
    C::write(&mut db, "feature", "default", 2).expect("write child");
    assert!(
        C::exists(&mut db, "feature", "default", 2).expect("child read own"),
        "{}: a child must see its own write",
        C::label()
    );
    assert!(
        !C::exists(&mut db, "default", "default", 2).expect("source read"),
        "{}: the source must not see the child write",
        C::label()
    );
}

fn space_isolation<C: CapabilityFixture>() {
    let mut db = open_cache_database().expect("cache database opens");
    C::ensure(&mut db, "default", "default").expect("ensure first space");
    C::ensure(&mut db, "default", "alt").expect("ensure second space");
    C::write(&mut db, "default", "default", 1).expect("write first space");
    assert!(
        C::exists(&mut db, "default", "default", 1).expect("read first space"),
        "{}: a value must be visible in its own space",
        C::label()
    );
    assert!(
        !C::exists(&mut db, "default", "alt", 1).expect("read second space"),
        "{}: spaces must isolate values",
        C::label()
    );
}

fn missing_branch_is_not_found<C: CapabilityFixture>() {
    let mut db = open_cache_database().expect("cache database opens");
    let error = C::probe(&mut db, "absent", "default").expect_err("missing-branch read fails");
    assert_status(
        &error,
        EngineErrorClass::NotFound,
        "not_found.engine.branch",
        false,
    );
}

fn closed_runtime_rejected<C: CapabilityFixture>() {
    let mut db = open_cache_database().expect("cache database opens");
    db.close().expect("close succeeds");
    let error = C::probe(&mut db, "default", "default").expect_err("closed handle rejects reads");
    assert_status(
        &error,
        EngineErrorClass::ClosedRuntime,
        "failed_precondition.engine.runtime_closed",
        false,
    );
}

#[cfg(feature = "localfs")]
fn durable_reopen_preserves<C: CapabilityFixture>() {
    let dir = tempfile::tempdir().expect("temp dir");
    {
        let mut db = common::open_durable_database(dir.path()).expect("durable open");
        C::ensure(&mut db, "default", "default").expect("ensure container");
        C::write(&mut db, "default", "default", 1).expect("write");
        db.close().expect("close");
    }
    let mut db = common::open_durable_database(dir.path()).expect("durable reopen");
    assert!(
        C::exists(&mut db, "default", "default", 1).expect("read after reopen"),
        "{}: durable reopen must preserve the value",
        C::label()
    );
}

macro_rules! conformance_suite {
    ($module:ident, $fixture:ty) => {
        mod $module {
            use super::*;

            #[test]
            fn write_then_read_visible() {
                super::write_then_read_visible::<$fixture>();
            }

            #[test]
            fn fork_inherits_and_isolates() {
                super::fork_inherits_and_isolates::<$fixture>();
            }

            #[test]
            fn space_isolation() {
                super::space_isolation::<$fixture>();
            }

            #[test]
            fn missing_branch_is_not_found() {
                super::missing_branch_is_not_found::<$fixture>();
            }

            #[test]
            fn closed_runtime_rejected() {
                super::closed_runtime_rejected::<$fixture>();
            }

            #[cfg(feature = "localfs")]
            #[test]
            fn durable_reopen_preserves() {
                super::durable_reopen_preserves::<$fixture>();
            }
        }
    };
}

conformance_suite!(kv, KvFixture);
conformance_suite!(json, JsonFixture);
conformance_suite!(event, EventFixture);
conformance_suite!(vector, VectorFixture);
conformance_suite!(graph, GraphFixture);
