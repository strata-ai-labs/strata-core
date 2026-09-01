//! The dataset directory's self-description (#3004).
//!
//! Field incident, 2026-09-01: pointed at a dataset directory with no signal
//! of what it was, a coding agent reverse-engineered the durable layout and
//! built a data pipeline on raw WAL bytes — which "worked" only because the
//! tiny dataset still sat in one segment. `.git/` and `SQLite` files never get
//! misread this way because they self-describe; this README is that
//! breadcrumb, written at creation and self-healed at open, pointing every
//! reader (human or agent) at the real surfaces instead of the files.
//!
//! The file is ADVISORY, not layout: recovery ignores root-level foreign
//! files, it is never checksummed, and a failure to write it never fails an
//! open. An existing file is never overwritten — user edits are theirs.

use std::path::Path;

/// The advisory file's name — `README.md` because that is the first file
/// both humans and agents reach for in an unknown directory.
pub(crate) const DATASET_README_NAME: &str = "README.md";

/// Content approved 2026-09-01. The warning paragraph is the payload: it
/// explains WHY direct reads lie, which is the sentence that stops an agent
/// from scraping the WAL.
const DATASET_README: &str = "\
# StrataDB database

This directory is a StrataDB database — an embedded, branchable store
(git-like semantics: branches, time travel) holding KV, JSON, events,
vectors, and graphs in one place.

**Do not read or modify the files in this directory directly.** Rows are
spread across the write-ahead log, snapshots, and tables under MVCC with
branches and tombstones — reading the files yields incomplete or wrong
data, and writing to them can corrupt the database. A `strata.sock` here
means a live process is serving this database right now.

Use the `strata` CLI instead (https://stratadb.org/install):

    strata . describe     # what's inside: branches, spaces, counts
    strata . kv list      # keys — also: json list, event types,
                          #   vector collection list, graph list
    strata .              # interactive session

For AI agents:

    strata . mcp serve    # MCP server over this database
    strata agents guide   # the full surface, written for agents

Docs: https://stratadb.org/docs · Python: pip install stratadb
";

/// Writes the advisory README when absent — at creation and as self-healing
/// on every later open (pre-existing datasets gain it on first touch). Never
/// overwrites an existing file, and never fails the open.
pub(crate) fn ensure_dataset_readme(dataset_dir: &Path) {
    let path = dataset_dir.join(DATASET_README_NAME);
    if path.exists() {
        return;
    }
    // Advisory by design: a read-only mount or permission refusal must not
    // fail the open, so the write error is deliberately discarded.
    let _ = std::fs::write(&path, DATASET_README);
}

#[cfg(test)]
mod tests {
    use super::{ensure_dataset_readme, DATASET_README_NAME};

    #[test]
    fn writes_when_absent_and_never_overwrites() {
        let dir = tempfile::tempdir().expect("tmp");
        ensure_dataset_readme(dir.path());
        let path = dir.path().join(DATASET_README_NAME);
        let written = std::fs::read_to_string(&path).expect("readme written");
        assert!(written.contains("Do not read or modify"));
        assert!(written.contains("strata . describe"));
        assert!(written.contains("https://stratadb.org/docs"));

        // A user-edited file is theirs: healing must not stomp it.
        std::fs::write(&path, "my notes\n").expect("user edit");
        ensure_dataset_readme(dir.path());
        assert_eq!(
            std::fs::read_to_string(&path).expect("still readable"),
            "my notes\n"
        );
    }
}
