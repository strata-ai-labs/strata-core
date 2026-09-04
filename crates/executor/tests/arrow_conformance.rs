//! Arrow conformance ledger lint.
//!
//! `arrow_conformance_ledger.yaml` is the tracked map of what the Arrow
//! import/export surface HAS and HAS NOT been tested (see
//! `docs/architecture/arrow-conformance.md`). This lint keeps the ledger honest
//! so coverage can't silently rot the way #3063 slipped through:
//!
//!   - `covered` cells must name a real `#[test] fn` in an arrow test source;
//!   - `gap` cells must name a tracking issue;
//!   - `accepted` cells must carry a rationale note.
//!
//! Adding an Arrow conformance test means moving its cell `gap -> covered` (and
//! pointing `test:` at the new fn). Adding a scenario means adding a cell.

use std::collections::BTreeSet;
use std::path::PathBuf;

use serde::Deserialize;

#[derive(Deserialize)]
struct Ledger {
    cells: Vec<Cell>,
}

#[derive(Deserialize)]
struct Cell {
    id: String,
    #[allow(dead_code)]
    scenario: String,
    status: String,
    #[serde(default)]
    test: Option<String>,
    #[serde(default)]
    issue: Option<u64>,
    #[serde(default)]
    note: Option<String>,
}

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Concatenated source of every file that may hold an Arrow conformance test,
/// so `covered` cells can be checked against real `#[test] fn`s.
fn arrow_test_sources() -> String {
    let root = manifest_dir();
    let files = [
        "tests/arrow_behavior.rs",
        "tests/arrow_disabled_behavior.rs",
        "tests/arrow_conformance.rs",
        "src/arrow/schema.rs",
    ];
    let mut body = String::new();
    for file in files {
        if let Ok(text) = std::fs::read_to_string(root.join(file)) {
            body.push_str(&text);
        }
    }
    body
}

fn load_ledger() -> Ledger {
    let path = manifest_dir().join("tests/arrow_conformance_ledger.yaml");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    serde_yaml::from_str(&text).expect("parse arrow conformance ledger")
}

#[test]
fn arrow_conformance_ledger_is_consistent() {
    let ledger = load_ledger();
    let sources = arrow_test_sources();
    let mut seen = BTreeSet::new();
    let mut violations = Vec::new();
    let (mut covered, mut gap, mut accepted) = (0u32, 0u32, 0u32);

    for cell in &ledger.cells {
        if !seen.insert(cell.id.clone()) {
            violations.push(format!("duplicate cell id `{}`", cell.id));
        }
        match cell.status.as_str() {
            "covered" => {
                covered += 1;
                match cell.test.as_deref() {
                    None => violations.push(format!("`{}`: covered but no `test:`", cell.id)),
                    Some(name) if !sources.contains(&format!("fn {name}")) => violations.push(
                        format!(
                            "`{}`: covered by `{name}` but no such #[test] fn exists in the arrow test sources",
                            cell.id
                        ),
                    ),
                    Some(_) => {}
                }
            }
            "gap" => {
                gap += 1;
                if cell.issue.is_none() {
                    violations.push(format!(
                        "`{}`: gap but no `issue:` (a gap must name its tracking issue)",
                        cell.id
                    ));
                }
            }
            "accepted" => {
                accepted += 1;
                if cell.note.is_none() {
                    violations.push(format!("`{}`: accepted but no `note:` rationale", cell.id));
                }
            }
            other => violations.push(format!("`{}`: unknown status `{other}`", cell.id)),
        }
    }

    println!(
        "arrow conformance ledger: {covered} covered / {gap} gap / {accepted} accepted \
         ({} cells total)",
        ledger.cells.len()
    );
    assert!(
        violations.is_empty(),
        "arrow conformance ledger inconsistencies:\n  {}",
        violations.join("\n  ")
    );
    assert!(covered + gap + accepted > 0, "ledger is empty");
}
