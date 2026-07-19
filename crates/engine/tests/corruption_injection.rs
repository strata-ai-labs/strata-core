//! Corruption injection at the persistence read boundary (TCP3.15a).
//!
//! Companion to the storage-fault seam. Where `StorageFaultKind` fails an
//! operation outright, `RowCorruption` lets the read succeed and mangles the
//! rows it returns — modelling on-disk bit-rot the storage layer hands back
//! intact but whose *content* the engine's `data_loss.*` decoders must reject.
//! This slice builds the harness and closes the two simplest content-corruption
//! codes (KV value, JSON index definition); TCP3.15b/c/d reuse it for graph,
//! vector, and control-plane records.

#![cfg(feature = "testkit")]

mod common;

use common::{assert_status, branch, key, open_cache_database, space, value};
use strata_engine::testkit::RowCorruption;
use strata_engine::{EngineErrorClass, JsonIndexName, JsonIndexType, JsonPath};

#[test]
fn a_scanned_kv_row_with_no_value_is_rejected_as_corruption() {
    let mut db = open_cache_database().expect("cache database opens");
    {
        let mut kv = db
            .kv(branch("default"), space("default"))
            .expect("kv opens");
        kv.put(key(b"k"), value(b"v")).expect("seed write");
    }

    // The next scan returns the row with its value cell stripped — as if the
    // stored value rotted away. The KV scan decoder must reject it rather than
    // fabricate an empty value.
    db.inject_scan_corruption_for_test(RowCorruption::DropValue);
    let mut kv = db
        .kv(branch("default"), space("default"))
        .expect("kv opens");
    let error = kv
        .scan(None, None)
        .expect_err("a valueless KV row must be rejected");
    assert_status(
        &error,
        EngineErrorClass::Corruption,
        "data_loss.engine.kv_value",
        false,
    );
}

#[test]
fn a_json_index_definition_with_a_bad_version_byte_is_rejected_as_corruption() {
    let mut db = open_cache_database().expect("cache database opens");
    {
        let mut json = db
            .json(branch("default"), space("default"))
            .expect("json opens");
        json.create_index(
            JsonIndexName::new("by_name").expect("index name"),
            "name".parse::<JsonPath>().expect("json path"),
            JsonIndexType::Tag,
        )
        .expect("index create");
    }

    // The next scan returns the index-definition row with malformed bytes whose
    // leading format-version byte is wrong; `decode_index_definition` must
    // reject it.
    db.inject_scan_corruption_for_test(RowCorruption::SetValue(vec![0xFF, 0x00]));
    let mut json = db
        .json(branch("default"), space("default"))
        .expect("json opens");
    let error = json
        .list_indexes()
        .expect_err("a malformed index definition must be rejected");
    assert_status(
        &error,
        EngineErrorClass::Corruption,
        "data_loss.engine.json_index",
        false,
    );
}
