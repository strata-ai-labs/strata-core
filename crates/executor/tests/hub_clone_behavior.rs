//! `HubClone` behavior: the executor verb end-to-end against a live
//! in-process hub, plus its typed error paths.

#![cfg(feature = "hub")]

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use strata_engine::{
    BranchName as EngineBranchName, Database, DurableLocalOpenOptions, KvKey, KvValue, ProductSpace,
};
use strata_executor::{Command, Executor, Output};
use strata_hub::stratahub_protocol::Hash;
use strata_hub::{EngineExportOptions, StrataCoreEngine};

fn build_source(path: &Path) {
    let mut db = Database::open_local(path, DurableLocalOpenOptions::new())
        .expect("source opens")
        .into_database();
    let mut kv = db
        .kv(
            EngineBranchName::new("default").expect("branch"),
            ProductSpace::new("default").expect("space"),
        )
        .expect("kv");
    kv.put(
        KvKey::new("user:ada").expect("key"),
        KvValue::new(b"engineer".to_vec()),
    )
    .expect("put");
}

fn serve_bundle(
    manifest_bytes: Vec<u8>,
    manifest_hash: &Hash,
    objects: HashMap<Hash, Vec<u8>>,
) -> String {
    let server = tiny_http::Server::http("127.0.0.1:0").expect("server binds");
    let port = server.server_addr().to_ip().expect("ip addr").port();
    let base = format!("http://127.0.0.1:{port}");
    let reference = serde_json::json!({
        "dataset": "titanic",
        "branch": "default",
        "manifest_hash": manifest_hash.as_str(),
        "last_updated": "2026-07-10T00:00:00Z",
    })
    .to_string();

    std::thread::spawn(move || {
        let objects = Arc::new(objects);
        for _ in 0..64 {
            let Ok(Some(request)) = server.recv_timeout(std::time::Duration::from_secs(5)) else {
                return;
            };
            let url = request.url().to_owned();
            let body = if url == "/v1/refs/titanic/default" {
                Some((reference.clone().into_bytes(), "application/json"))
            } else if url.starts_with("/v1/manifests/") {
                Some((manifest_bytes.clone(), "application/json"))
            } else if let Some(hash) = url.strip_prefix("/v1/objects/") {
                Hash::parse(hash)
                    .ok()
                    .and_then(|hash| objects.get(&hash).cloned())
                    .map(|body| (body, "application/octet-stream"))
            } else {
                None
            };
            match body {
                Some((bytes, content_type)) => {
                    let header = tiny_http::Header::from_bytes(
                        &b"Content-Type"[..],
                        content_type.as_bytes(),
                    )
                    .expect("header");
                    let _ =
                        request.respond(tiny_http::Response::from_data(bytes).with_header(header));
                }
                None => {
                    let _ = request.respond(tiny_http::Response::empty(404));
                }
            }
        }
    });
    base
}

#[test]
fn hub_clone_reconstitutes_a_queryable_database_with_origin() {
    let source = tempfile::tempdir().expect("tempdir");
    build_source(source.path());
    let mut engine = StrataCoreEngine::open(source.path()).expect("open");
    let output = engine
        .export_bundle(&EngineExportOptions::default())
        .expect("export");
    let manifest_hash =
        strata_hub::stratahub_protocol::hash_bytes(&output.manifest_canonical_bytes);
    let objects: HashMap<Hash, Vec<u8>> = output
        .objects
        .into_iter()
        .map(|object| (object.hash, object.bytes))
        .collect();
    let base = serve_bundle(output.manifest_canonical_bytes, &manifest_hash, objects);

    let workdir = tempfile::tempdir().expect("workdir");
    let dest = workdir.path().join("titanic.strata");

    let mut executor = Executor::open_cache().expect("cache executor");
    let outcome = executor
        .execute(Command::HubClone {
            dataset: "titanic".to_owned(),
            branch: Some("default".to_owned()),
            dest: dest.display().to_string(),
            hub_url: Some(base.clone()),
        })
        .expect("clone succeeds");
    let Output::HubCloneResult {
        dataset,
        branch,
        manifest_hash: reported_hash,
        ..
    } = outcome
    else {
        panic!("unexpected output: {outcome:?}");
    };
    assert_eq!(dataset, "titanic");
    assert_eq!(branch, "default");
    assert_eq!(reported_hash, manifest_hash.as_str());

    // The clone answers reads and `remote_get` reports its origin.
    let mut clone_executor = Executor::open_durable_local(&dest).expect("clone opens");
    let origin = clone_executor
        .execute(Command::RemoteGet)
        .expect("remote get");
    let Output::RemoteOriginResult { origin } = origin else {
        panic!("unexpected output");
    };
    let origin = origin.expect("origin recorded");
    assert_eq!(origin.hub_url_or_dataset(), ("titanic", base.as_str()));
}

/// Small accessor shim so the assertion reads clearly.
trait OriginFacts {
    fn hub_url_or_dataset(&self) -> (&str, &str);
}
impl OriginFacts for strata_executor::RemoteOriginInfo {
    fn hub_url_or_dataset(&self) -> (&str, &str) {
        (self.dataset.as_str(), self.remote_url.as_str())
    }
}

#[test]
fn hub_clone_without_configuration_reports_the_hub_url_code() {
    // Explicitly break resolution for this invocation: an invalid flag
    // value fails at the flag layer, env-independent.
    let mut executor = Executor::open_cache().expect("cache executor");
    let error = executor
        .execute(Command::HubClone {
            dataset: "titanic".to_owned(),
            branch: None,
            dest: "unused".to_owned(),
            hub_url: Some("not a url".to_owned()),
        })
        .expect_err("malformed hub URL refuses");
    assert_eq!(error.status().code(), "invalid_argument.executor.hub_url");
}

#[test]
fn hub_clone_transport_failures_report_the_transport_code() {
    let workdir = tempfile::tempdir().expect("workdir");
    let mut executor = Executor::open_cache().expect("cache executor");
    // A live listener that 404s everything: resolve_ref fails on the wire.
    let server = tiny_http::Server::http("127.0.0.1:0").expect("binds");
    let port = server.server_addr().to_ip().expect("addr").port();
    std::thread::spawn(move || {
        while let Ok(Some(request)) = server.recv_timeout(std::time::Duration::from_secs(3)) {
            let _ = request.respond(tiny_http::Response::empty(404));
        }
    });
    let error = executor
        .execute(Command::HubClone {
            dataset: "titanic".to_owned(),
            branch: Some("default".to_owned()),
            dest: workdir.path().join("x").display().to_string(),
            hub_url: Some(format!("http://127.0.0.1:{port}")),
        })
        .expect_err("transport failure refuses");
    assert_eq!(error.status().code(), "unavailable.executor.hub_transport");
}

#[test]
fn hub_clone_of_an_incompatible_bundle_reports_the_precondition_code() {
    // A well-formed bundle whose manifest demands an engine this build cannot
    // satisfy. The pre-download compatibility gate refuses it, and the failure
    // surfaces through the executor as the non-retryable clone envelope
    // (TCP3.13). No objects are served: the gate fires before any download.
    let source = tempfile::tempdir().expect("tempdir");
    build_source(source.path());
    let mut engine = StrataCoreEngine::open(source.path()).expect("open");
    let output = engine
        .export_bundle(&EngineExportOptions::default())
        .expect("export");
    let mut manifest = output.manifest;
    manifest.engine_compatibility.required_engine_version = ">=99.0.0".to_owned();
    let manifest_bytes = manifest.canonical_bytes().expect("canonicalize");
    let manifest_hash = strata_hub::stratahub_protocol::hash_bytes(&manifest_bytes);
    let base = serve_bundle(manifest_bytes, &manifest_hash, HashMap::new());

    let workdir = tempfile::tempdir().expect("workdir");
    let dest = workdir.path().join("titanic.strata");
    let mut executor = Executor::open_cache().expect("cache executor");
    let error = executor
        .execute(Command::HubClone {
            dataset: "titanic".to_owned(),
            branch: Some("default".to_owned()),
            dest: dest.display().to_string(),
            hub_url: Some(base),
        })
        .expect_err("incompatible bundle refuses");
    assert_eq!(
        error.status().code(),
        "failed_precondition.executor.hub_clone"
    );
    assert!(
        !dest.exists(),
        "no destination state on an incompatible clone"
    );
}
