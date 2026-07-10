//! KV pagination across the internal raw-page boundary (64 rows).

mod common;

use common::{branch, key, open_cache_database, space, value};
use strata_engine::KvKey;

fn name(index: usize) -> String {
    format!("key-{index:03}")
}

/// Walking `list_page` by cursor returns every key exactly once in order, and a
/// scan past interleaved tombstones works across the raw-page boundary — both
/// require more than the 64-row internal page, which smaller suites never hit.
#[test]
fn kv_list_page_cursor_walk_and_tombstone_skip_cross_raw_page_boundary() {
    let total = 200usize;
    let mut db = open_cache_database().expect("cache database opens");
    let mut kv = db
        .kv(branch("default"), space("default"))
        .expect("kv service opens");

    for index in 0..total {
        kv.put(key(name(index).as_bytes()), value(b"v"))
            .expect("put");
    }

    // Walk the whole key set in pages of 30 via the cursor.
    let mut collected: Vec<String> = Vec::new();
    let mut cursor: Option<KvKey> = None;
    loop {
        let page = kv.list_page(None, cursor.as_ref(), 30).expect("list page");
        for stored in page.keys() {
            collected.push(String::from_utf8(stored.as_bytes().to_vec()).expect("utf8 key"));
        }
        if let Some(next) = page.cursor() {
            cursor = Some(next.clone());
        } else {
            assert!(!page.has_more(), "the final page must not report more");
            break;
        }
    }
    let mut expected: Vec<String> = (0..total).map(name).collect();
    expected.sort();
    assert_eq!(
        collected, expected,
        "cursor walk returns every key once, in order"
    );

    // Tombstone every other key, leaving 100 tombstones interleaved among 200
    // rows, then scan: the live keys come back in order across the raw pages.
    for index in (0..total).step_by(2) {
        kv.delete(key(name(index).as_bytes())).expect("delete");
    }
    let live: Vec<String> = kv
        .list(None)
        .expect("list")
        .iter()
        .map(|stored| String::from_utf8(stored.as_bytes().to_vec()).expect("utf8 key"))
        .collect();
    let mut expected_live: Vec<String> = (0..total)
        .filter(|index| index % 2 == 1)
        .map(name)
        .collect();
    expected_live.sort();
    assert_eq!(
        live, expected_live,
        "scan skips tombstones across raw pages"
    );
}
