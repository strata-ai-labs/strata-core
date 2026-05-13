//! Boundary tests for the feature-gated storage testkit.

#![deny(unsafe_code)]

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

struct ProbeCase<'a> {
    name: &'a str,
    default_features: bool,
    features: &'a [&'a str],
    source: &'a str,
    expected: ProbeExpectation<'a>,
}

enum ProbeExpectation<'a> {
    Success,
    FailureContaining(&'a [&'a str]),
}

#[test]
fn testkit_visibility_matches_feature_selection() {
    let target_dir = tempfile::tempdir().expect("probe target dir");

    for case in probe_cases() {
        let output = run_probe(&case, target_dir.path());
        match case.expected {
            ProbeExpectation::Success => {
                assert!(
                    output.status.success(),
                    "probe {} should compile\nstdout:\n{}\nstderr:\n{}",
                    case.name,
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr),
                );
            }
            ProbeExpectation::FailureContaining(expected) => {
                assert!(
                    !output.status.success(),
                    "probe {} should fail to compile",
                    case.name
                );
                assert_failure_contains(case.name, &output, expected);
            }
        }
    }
}

#[test]
fn testkit_source_boundary_stays_feature_gated_and_hidden() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let lib = fs::read_to_string(root.join("src/lib.rs")).expect("read lib.rs");
    let testkit = fs::read_to_string(root.join("src/testkit/mod.rs")).expect("read testkit");

    assert!(
        lib.contains("#[cfg(any(test, feature = \"testkit\"))]\n#[doc(hidden)]\npub mod testkit;"),
        "crate root should gate and hide the testkit module"
    );
    assert!(
        testkit.contains("#![doc(hidden)]"),
        "testkit module should hide its feature-gated public surface from normal docs"
    );
    assert!(
        testkit.contains("#[cfg(any(test, feature = \"fault-injection\"))]\npub use fault::"),
        "fault-injection exports should remain behind the fault-injection feature"
    );
}

#[test]
fn localfs_feature_is_rejected_for_wasm_builds() {
    let target_dir = tempfile::tempdir().expect("probe target dir");
    let output = run_target_probe(
        true,
        &[],
        r"
            fn main() {}
        ",
        "wasm32-unknown-unknown",
        target_dir.path(),
    );

    assert!(
        !output.status.success(),
        "default-feature wasm probe should fail to compile"
    );
    assert_failure_contains(
        "default-feature-wasm",
        &output,
        &[
            "the localfs feature is not supported on wasm32",
            "use default-features = false",
        ],
    );
}

fn probe_cases<'a>() -> Vec<ProbeCase<'a>> {
    vec![
        ProbeCase {
            name: "default-features-without-testkit",
            default_features: true,
            features: &[],
            source: r#"
                use strata_storage_next::testkit::TestBackendKind;

                fn main() {
                    let _ = TestBackendKind::parse("memory");
                }
            "#,
            expected: ProbeExpectation::FailureContaining(&["testkit", "TestBackendKind"]),
        },
        ProbeCase {
            name: "memory-only-without-testkit",
            default_features: false,
            features: &[],
            source: r#"
                use strata_storage_next::testkit::TestBackendKind;

                fn main() {
                    let _ = TestBackendKind::parse("memory");
                }
            "#,
            expected: ProbeExpectation::FailureContaining(&["testkit", "TestBackendKind"]),
        },
        ProbeCase {
            name: "with-testkit",
            default_features: false,
            features: &["testkit"],
            source: r#"
                use strata_storage_next::testkit::{
                    FormatDecodeOutcome, FormatDecoder, TestBackendKind, decode_format_bytes,
                };

                fn main() -> Result<(), Box<dyn std::error::Error>> {
                    let backend = TestBackendKind::parse("memory")?;
                    assert_eq!(backend.name(), "memory");
                    assert_eq!(
                        decode_format_bytes(FormatDecoder::Manifest, &[]),
                        FormatDecodeOutcome::Rejected
                    );
                    Ok(())
                }
            "#,
            expected: ProbeExpectation::Success,
        },
        ProbeCase {
            name: "without-fault-injection",
            default_features: false,
            features: &["testkit"],
            source: r"
                use strata_storage_next::testkit::FaultScript;

                fn main() {
                    let _ = FaultScript::empty();
                }
            ",
            expected: ProbeExpectation::FailureContaining(&["testkit", "FaultScript"]),
        },
        ProbeCase {
            name: "with-fault-injection",
            default_features: false,
            features: &["fault-injection"],
            source: r#"
                use strata_storage_next::testkit::{
                    BackendOperation, FaultKind, FaultRule, FaultScript, FaultingBackend,
                };
                use std::num::NonZeroU64;

                fn main() -> Result<(), Box<dyn std::error::Error>> {
                    let one = NonZeroU64::new(1).ok_or("non-zero call number")?;
                    let script = FaultScript::new([FaultRule::new(
                        BackendOperation::WriteObject,
                        one,
                        FaultKind::Interrupted,
                    )]);
                    let backend = FaultingBackend::new((), script);
                    assert_eq!(
                        backend.before_operation(BackendOperation::WriteObject),
                        Err(FaultKind::Interrupted)
                    );
                    assert_eq!(
                        backend.before_operation(BackendOperation::WriteObject),
                        Ok(())
                    );
                    assert_eq!(backend.calls().len(), 2);
                    Ok(())
                }
            "#,
            expected: ProbeExpectation::Success,
        },
    ]
}

fn run_probe(case: &ProbeCase<'_>, shared_target_dir: &Path) -> Output {
    let temp = tempfile::tempdir().expect("probe package dir");
    write_probe_manifest(temp.path(), case.default_features, case.features);
    write_probe_source(temp.path(), case.source);

    run_cargo_check(temp.path(), shared_target_dir, None)
}

fn run_target_probe(
    default_features: bool,
    features: &[&str],
    source: &str,
    target: &str,
    shared_target_dir: &Path,
) -> Output {
    let temp = tempfile::tempdir().expect("probe package dir");
    write_probe_manifest(temp.path(), default_features, features);
    write_probe_source(temp.path(), source);

    run_cargo_check(temp.path(), shared_target_dir, Some(target))
}

fn run_cargo_check(package_dir: &Path, shared_target_dir: &Path, target: Option<&str>) -> Output {
    let mut command = Command::new(cargo());
    command
        .args(["check", "--quiet", "--offline"])
        .arg("--manifest-path")
        .arg(package_dir.join("Cargo.toml"))
        .env("CARGO_TARGET_DIR", shared_target_dir)
        .env("CARGO_TERM_COLOR", "never")
        .env("CARGO_INCREMENTAL", "0");

    if let Some(target) = target {
        command.arg("--target").arg(target);
    }

    command.output().expect("run cargo check probe")
}

fn write_probe_manifest(path: &Path, default_features: bool, features: &[&str]) {
    let feature_list = features
        .iter()
        .map(|feature| format!("\"{feature}\""))
        .collect::<Vec<_>>()
        .join(", ");
    let storage_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let storage_path = storage_root.to_string_lossy().replace('\\', "\\\\");
    let manifest = format!(
        r#"[package]
name = "storage_next_boundary_probe"
version = "0.0.0"
edition = "2021"
publish = false

[workspace]

[dependencies]
strata-storage-next = {{ path = "{storage_path}", default-features = {default_features}, features = [{feature_list}] }}
"#
    );

    fs::write(path.join("Cargo.toml"), manifest).expect("write probe manifest");
}

fn write_probe_source(path: &Path, source: &str) {
    let source_dir = path.join("src");
    fs::create_dir(&source_dir).expect("create probe source dir");
    fs::write(source_dir.join("main.rs"), source).expect("write probe source");
}

fn cargo() -> OsString {
    std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"))
}

fn assert_failure_contains(probe_name: &str, output: &Output, expected_terms: &[&str]) {
    let stderr = String::from_utf8_lossy(&output.stderr);
    for expected in expected_terms {
        assert!(
            stderr.contains(expected),
            "probe {probe_name} stderr should contain {expected:?}\nstderr:\n{stderr}"
        );
    }
}
