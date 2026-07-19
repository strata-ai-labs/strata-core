//! Clone transport fault injection (TCP3.13).
//!
//! `clone_flow.rs` covers the happy path, the compatibility gate, and one
//! mid-download failure (a missing object). Its `FakeHub` can only fail at
//! `get_object`, so the earlier steps of the §3.6 flow — `default_branch`,
//! `resolve_ref`, `get_manifest` — and the integrity boundary were never
//! fault-tested. This drives a configurable fault transport that fails at any
//! chosen step and counts every call, so each test asserts three things: the
//! right `CloneError` surfaces, the flow **short-circuits** (no later transport
//! call happens), and a failed clone leaves **no destination state**.

use std::cell::Cell;
use std::collections::HashMap;
use std::path::Path;

use strata_engine::{
    BranchName as EngineBranchName, Database, DurableLocalOpenOptions, KvKey, KvValue, ProductSpace,
};
use strata_hub::{
    clone_dataset, BundleImportError, CloneError, CloneRequest, EngineExportOptions, HubTransport,
    StrataCoreEngine,
};
use stratahub_protocol::{BranchName, DatasetName, Hash, Manifest};

/// The step at which the transport should inject a `Transport` failure.
#[derive(Clone, Copy, PartialEq, Eq)]
enum FailAt {
    DefaultBranch,
    ResolveRef,
    GetManifest,
    None,
}

/// Per-method call counters, so a test can prove the flow stopped before the
/// next wire call.
#[derive(Default)]
struct Calls {
    default_branch: Cell<u64>,
    resolve_ref: Cell<u64>,
    get_manifest: Cell<u64>,
    get_object: Cell<u64>,
}

/// A hub backed by a real exported bundle that can fail at a chosen step or
/// hand back corrupted object bytes.
struct FaultHub {
    dataset: DatasetName,
    branch: BranchName,
    manifest_hash: Hash,
    manifest: Manifest,
    objects: HashMap<Hash, Vec<u8>>,
    fail_at: FailAt,
    corrupt_objects: bool,
    calls: Calls,
}

impl FaultHub {
    fn from_database(path: &Path) -> Self {
        let mut engine = StrataCoreEngine::open(path).expect("open");
        let output = engine
            .export_bundle(&EngineExportOptions::default())
            .expect("export");
        let manifest_hash = stratahub_protocol::hash_bytes(&output.manifest_canonical_bytes);
        Self {
            dataset: DatasetName::parse("titanic").expect("dataset"),
            branch: BranchName::parse("default").expect("branch"),
            manifest_hash,
            manifest: output.manifest,
            objects: output
                .objects
                .into_iter()
                .map(|object| (object.hash, object.bytes))
                .collect(),
            fail_at: FailAt::None,
            corrupt_objects: false,
            calls: Calls::default(),
        }
    }

    fn fault(detail: &str) -> CloneError {
        CloneError::Transport {
            detail: detail.to_owned(),
        }
    }
}

impl HubTransport for FaultHub {
    fn hub_url(&self) -> String {
        "https://hub.example.com".to_owned()
    }

    fn default_branch(&self, dataset: &DatasetName) -> Result<BranchName, CloneError> {
        self.calls
            .default_branch
            .set(self.calls.default_branch.get() + 1);
        if self.fail_at == FailAt::DefaultBranch {
            return Err(Self::fault("default_branch unreachable"));
        }
        assert_eq!(dataset, &self.dataset);
        Ok(self.branch.clone())
    }

    fn resolve_ref(
        &self,
        _dataset: &DatasetName,
        _branch: &BranchName,
    ) -> Result<Hash, CloneError> {
        self.calls.resolve_ref.set(self.calls.resolve_ref.get() + 1);
        if self.fail_at == FailAt::ResolveRef {
            return Err(Self::fault("resolve_ref unreachable"));
        }
        Ok(self.manifest_hash.clone())
    }

    fn get_manifest(&self, hash: &Hash) -> Result<Manifest, CloneError> {
        self.calls
            .get_manifest
            .set(self.calls.get_manifest.get() + 1);
        if self.fail_at == FailAt::GetManifest {
            return Err(Self::fault("get_manifest unreachable"));
        }
        assert_eq!(hash, &self.manifest_hash);
        Ok(self.manifest.clone())
    }

    fn get_object(&self, hash: &Hash) -> Result<Vec<u8>, CloneError> {
        self.calls.get_object.set(self.calls.get_object.get() + 1);
        let bytes = self
            .objects
            .get(hash)
            .cloned()
            .ok_or_else(|| Self::fault(&format!("object {} not found", hash.as_str())))?;
        if self.corrupt_objects {
            // Deliver a valid-hash object with tampered bytes: the fetch loop
            // accepts it, but import's per-object hash check must reject it.
            let mut tampered = bytes;
            tampered.push(0xFF);
            return Ok(tampered);
        }
        Ok(bytes)
    }
}

fn build_source() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut db = Database::open_local(dir.path(), DurableLocalOpenOptions::new())
        .expect("opens")
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
    dir
}

/// Runs a clone against `hub` with `branch`, expecting failure; returns the error.
fn clone_expecting_failure(hub: &FaultHub, branch: Option<BranchName>, dest: &Path) -> CloneError {
    let result = clone_dataset(
        hub,
        &CloneRequest {
            dataset: hub.dataset.clone(),
            branch,
            dest: dest.to_owned(),
        },
        &mut |_| {},
    );
    match result {
        Ok(_) => panic!("clone must fail"),
        Err(error) => error,
    }
}

#[test]
fn default_branch_fault_short_circuits_before_resolve() {
    let source = build_source();
    let mut hub = FaultHub::from_database(source.path());
    hub.fail_at = FailAt::DefaultBranch;
    let workdir = tempfile::tempdir().expect("workdir");
    let dest = workdir.path().join("titanic.strata");

    // branch: None forces default-branch discovery, which fails first.
    let error = clone_expecting_failure(&hub, None, &dest);
    assert!(matches!(error, CloneError::Transport { .. }));
    assert_eq!(hub.calls.default_branch.get(), 1);
    assert_eq!(hub.calls.resolve_ref.get(), 0, "resolve must not run");
    assert_eq!(hub.calls.get_manifest.get(), 0);
    assert_eq!(hub.calls.get_object.get(), 0);
    assert!(!dest.exists(), "no destination state");
}

#[test]
fn resolve_ref_fault_short_circuits_before_manifest() {
    let source = build_source();
    let mut hub = FaultHub::from_database(source.path());
    hub.fail_at = FailAt::ResolveRef;
    let workdir = tempfile::tempdir().expect("workdir");
    let dest = workdir.path().join("titanic.strata");

    let error = clone_expecting_failure(&hub, Some(hub.branch.clone()), &dest);
    assert!(matches!(error, CloneError::Transport { .. }));
    // Explicit branch is given, so default_branch is skipped entirely.
    assert_eq!(
        hub.calls.default_branch.get(),
        0,
        "explicit branch skips discovery"
    );
    assert_eq!(hub.calls.resolve_ref.get(), 1);
    assert_eq!(hub.calls.get_manifest.get(), 0, "manifest must not run");
    assert_eq!(hub.calls.get_object.get(), 0);
    assert!(!dest.exists(), "no destination state");
}

#[test]
fn get_manifest_fault_short_circuits_before_download() {
    let source = build_source();
    let mut hub = FaultHub::from_database(source.path());
    hub.fail_at = FailAt::GetManifest;
    let workdir = tempfile::tempdir().expect("workdir");
    let dest = workdir.path().join("titanic.strata");

    let error = clone_expecting_failure(&hub, Some(hub.branch.clone()), &dest);
    assert!(matches!(error, CloneError::Transport { .. }));
    assert_eq!(hub.calls.get_manifest.get(), 1);
    assert_eq!(
        hub.calls.get_object.get(),
        0,
        "no object download after a manifest fault"
    );
    assert!(!dest.exists(), "no destination state");
}

#[test]
fn corrupted_object_bytes_are_rejected_by_import_integrity() {
    let source = build_source();
    let mut hub = FaultHub::from_database(source.path());
    hub.corrupt_objects = true;
    let workdir = tempfile::tempdir().expect("workdir");
    let dest = workdir.path().join("titanic.strata");

    // The transport delivers bytes that pass the fetch loop but do not hash to
    // their declared object hash; import's integrity check must catch it.
    let error = clone_expecting_failure(&hub, Some(hub.branch.clone()), &dest);
    assert!(
        matches!(
            error,
            CloneError::Import(BundleImportError::ObjectHashMismatch { .. })
        ),
        "expected an object-hash-mismatch, got {error:?}"
    );
    assert!(
        hub.calls.get_object.get() >= 1,
        "at least one object was fetched"
    );
    assert!(
        !dest.exists(),
        "no destination state after a corrupt bundle"
    );
}

#[test]
fn malformed_engine_requirement_refuses_before_download() {
    let source = build_source();
    let mut hub = FaultHub::from_database(source.path());
    // A manifest whose engine requirement is not valid semver: the pre-download
    // compatibility gate parses it and must refuse without fetching objects.
    hub.manifest.engine_compatibility.required_engine_version = "not-a-semver-range".to_owned();
    let workdir = tempfile::tempdir().expect("workdir");
    let dest = workdir.path().join("titanic.strata");

    let error = clone_expecting_failure(&hub, Some(hub.branch.clone()), &dest);
    assert!(
        matches!(error, CloneError::Transport { .. }),
        "a malformed engine requirement surfaces as Transport, got {error:?}"
    );
    assert_eq!(
        hub.calls.get_object.get(),
        0,
        "no bandwidth on a malformed manifest"
    );
    assert!(!dest.exists(), "no destination state");
}
