//! Doc↔surface parity guard (TCP3.1, Phase 3 Tier-1 tracker).
//!
//! `core-architecture.md`'s "Implemented M1 Boundary" section is the
//! authoritative statement of what core exports. This guard ties it to the
//! committed public-API snapshot: every type in the Public Exports table
//! and every backticked associated item must appear in
//! `snapshots/public_api.txt`, so a surface change without a doc update
//! (or a doc claim with no implementation) fails CI — the same anti-drift
//! discipline as the storage charter guard.
//!
//! It also locks the one deliberate derive asymmetry as a stated invariant
//! rather than an accident: `BranchId` must NOT gain `Default` (core does
//! not create sentinel branch identities), while `CommitVersion` and
//! `Timestamp` keep theirs.

use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root above crates/core")
        .to_path_buf()
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

/// Extract one `### `-delimited subsection of the doc.
fn subsection<'doc>(doc: &'doc str, heading: &str) -> &'doc str {
    let start = doc
        .find(heading)
        .unwrap_or_else(|| panic!("core-architecture.md keeps the {heading} subsection"));
    let body = &doc[start + heading.len()..];
    let end = body.find("\n### ").unwrap_or(body.len());
    &body[..end]
}

/// Backticked surface tokens: table first columns and numbered items.
fn backticked_items(section: &str) -> Vec<String> {
    let mut items = Vec::new();
    for line in section.lines() {
        let trimmed = line.trim_start();
        // Table rows (`| `Type` | ...`) and numbered items (`1. `name``).
        let is_table_row = trimmed.starts_with("| `");
        let is_numbered = trimmed
            .chars()
            .next()
            .is_some_and(|first| first.is_ascii_digit())
            && trimmed.contains(". `");
        if !is_table_row && !is_numbered {
            continue;
        }
        // First backticked token only: the name column / the item itself.
        let Some(start) = trimmed.find('`') else {
            continue;
        };
        let rest = &trimmed[start + 1..];
        let Some(end) = rest.find('`') else { continue };
        let token = &rest[..end];
        // Skip prose-only tokens ("Serialize` and `Deserialize" style
        // splits produce clean idents; compound prose like "Ordering
        // traits" is not backticked and never reaches here).
        if token.is_empty() || token.contains(' ') {
            continue;
        }
        items.push(token.to_owned());
    }
    items.sort();
    items.dedup();
    items
}

#[test]
fn every_documented_boundary_item_exists_in_the_public_api_snapshot() {
    let root = repo_root();
    let doc = read(&root.join("docs/architecture/core-architecture.md"));
    let snapshot = read(&root.join("crates/core/tests/snapshots/public_api.txt"));

    let mut documented = Vec::new();
    for heading in [
        "### Public Exports",
        "### Public Associated Items",
        "### Public Trait Surface",
    ] {
        documented.extend(backticked_items(subsection(&doc, heading)));
    }
    documented.sort();
    documented.dedup();

    assert!(
        documented.len() >= 10,
        "the M1 boundary subsections parsed to almost nothing \
         ({documented:?}) — the guard's parser no longer matches the doc's \
         shape; fix the parser, not the doc"
    );
    let missing: Vec<&String> = documented
        .iter()
        .filter(|item| !snapshot.contains(item.as_str()))
        .collect();
    assert!(
        missing.is_empty(),
        "documented in core-architecture.md's M1 boundary but absent from \
         snapshots/public_api.txt (doc claim with no implementation, or a \
         removed export whose doc row survived): {missing:?}"
    );
}

/// The inverse direction: every type the doc explicitly rejects from M1
/// must stay out of the public surface. A rejected type quietly becoming
/// `pub` is exactly the "product surface by accident" the doc forbids.
#[test]
fn explicitly_rejected_types_stay_out_of_the_public_surface() {
    let root = repo_root();
    let doc = read(&root.join("docs/architecture/core-architecture.md"));
    let snapshot = read(&root.join("crates/core/tests/snapshots/public_api.txt"));

    let rejected = backticked_items(subsection(&doc, "### Explicitly Rejected From M1"));
    assert!(
        rejected.len() >= 3,
        "the rejected-exports table parsed to almost nothing ({rejected:?}) \
         — fix the parser, not the doc"
    );
    let leaked: Vec<&String> = rejected
        .iter()
        .filter(|name| {
            snapshot.contains(&format!("pub struct {name}"))
                || snapshot.contains(&format!("pub enum {name}"))
                || snapshot.contains(&format!("pub trait {name}"))
        })
        .collect();
    assert!(
        leaked.is_empty(),
        "types explicitly rejected from the M1 boundary have appeared in \
         the public surface: {leaked:?}"
    );
}

#[test]
fn branch_id_never_gains_default_while_the_transparent_atoms_keep_theirs() {
    let snapshot = read(&repo_root().join("crates/core/tests/snapshots/public_api.txt"));

    let derive_line_for = |type_name: &str| -> String {
        let type_decl = format!("pub struct {type_name}");
        let decl_at = snapshot
            .find(&type_decl)
            .unwrap_or_else(|| panic!("snapshot declares {type_name}"));
        snapshot[..decl_at]
            .lines()
            .rev()
            .find(|line| line.contains("#[derive("))
            .unwrap_or_else(|| panic!("derive line above {type_name}"))
            .to_owned()
    };

    assert!(
        !derive_line_for("BranchId").contains("Default"),
        "BranchId must not gain Default: core does not create sentinel \
         branch identities (core-architecture.md, Public Exports table)"
    );
    for keeps_default in ["CommitVersion", "Timestamp"] {
        assert!(
            derive_line_for(keeps_default).contains("Default"),
            "{keeps_default} is documented with a Default (ZERO/EPOCH) — \
             removing it is a public-surface break"
        );
    }
}
