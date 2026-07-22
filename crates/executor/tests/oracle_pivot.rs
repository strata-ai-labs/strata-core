//! TCP4.10 pivot-containment oracle (the PQS analog).
//!
//! Plant seeded rows, pick pivots, and issue queries *guaranteed by
//! construction* to match the pivot — absence is a logic bug, with no
//! reference engine required. The input choice comes from a seeded generator
//! and the verdict from the containment guarantee, so neither embeds the
//! author's mental model (the Phase 4 charter's core requirement).

use std::collections::BTreeSet;

use serde_json::json;

#[path = "parity/support.rs"]
mod support;

/// `SplitMix64` — tiny, seedable, deterministic.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
}

fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(TABLE[(n >> 18) as usize & 0x3f] as char);
        out.push(TABLE[(n >> 12) as usize & 0x3f] as char);
        out.push(if chunk.len() > 1 {
            TABLE[(n >> 6) as usize & 0x3f] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[n as usize & 0x3f] as char
        } else {
            '='
        });
    }
    out
}

const SEEDS: [u64; 4] = [1, 7, 42, 4096];
const KEYS_PER_SEED: usize = 24;

/// Every planted KV key must be found by the two queries constructed to
/// contain it: a byte-prefix listing and an inclusive-start scan.
#[test]
fn every_planted_kv_key_is_found_by_its_prefix_and_its_scan() {
    for seed in SEEDS {
        let mut rng = Rng(seed);
        let mut keys = BTreeSet::new();
        while keys.len() < KEYS_PER_SEED {
            let raw = rng.next().to_be_bytes();
            keys.insert([raw[0], raw[1], raw[2]]);
        }
        let keys: Vec<[u8; 3]> = keys.into_iter().collect();

        let mut executor = support::executor();
        let entries: Vec<_> = keys
            .iter()
            .map(|key| json!({"key": base64(key), "value": "b25l"}))
            .collect();
        support::run(
            &mut executor,
            &json!({"type": "kv_batch_put", "entries": entries}),
        );

        for pivot in &keys {
            // Prefix query: the pivot's own 2-byte prefix must return it.
            let listed = support::run(
                &mut executor,
                &json!({"type": "kv_list", "prefix": base64(&pivot[..2]), "limit": 1000}),
            );
            let contains = listed["data"]["items"]
                .as_array()
                .unwrap_or_else(|| panic!("kv_list carries items: {listed}"))
                .iter()
                .any(|item| item.as_str() == Some(base64(pivot).as_str()));
            assert!(
                contains,
                "[seed={seed}] pivot {pivot:?} missing from its own prefix listing"
            );

            // Inclusive-start scan: seeking at the pivot must return it first.
            let scanned = support::run(
                &mut executor,
                &json!({"type": "kv_scan", "start": base64(pivot), "limit": 1}),
            );
            assert_eq!(
                scanned["data"]["items"][0]["key"].as_str(),
                Some(base64(pivot).as_str()),
                "[seed={seed}] inclusive-start scan at the pivot must return the pivot"
            );
        }
    }
}

/// Every planted JSON document must be found by its prefix listing, and its
/// planted field must read back by path — including a dotted field name via
/// bracket notation (the #2703/#2731 surface).
#[test]
fn every_planted_json_document_reads_back_by_prefix_and_path() {
    for seed in SEEDS {
        let mut executor = support::executor();
        let count = 8 + usize::try_from(seed % 5).expect("small");
        for index in 0..count {
            support::run(
                &mut executor,
                &json!({"type": "json_set", "key": format!("p{index}"), "path": "$",
                        "value": {"marker": index, "a.b": index * 10}}),
            );
        }
        for index in 0..count {
            let key = format!("p{index}");
            let listed = support::run(
                &mut executor,
                &json!({"type": "json_list", "prefix": "p", "limit": 1000}),
            );
            let contains = listed["data"]["items"]
                .as_array()
                .unwrap_or_else(|| panic!("json_list carries items: {listed}"))
                .iter()
                .any(|item| item.as_str() == Some(key.as_str()));
            assert!(
                contains,
                "[seed={seed}] document {key} missing from its prefix listing"
            );

            let marker = support::run(
                &mut executor,
                &json!({"type": "json_get", "key": key, "path": "$.marker"}),
            );
            // Path reads return the versioned variant (#2596's second shape):
            // the read value nests inside the version envelope.
            assert_eq!(
                marker["data"]["value"]["value"].as_u64(),
                Some(u64::try_from(index).expect("small")),
                "[seed={seed}] planted marker must read back by path"
            );

            let dotted = support::run(
                &mut executor,
                &json!({"type": "json_get", "key": key, "path": "$['a.b']"}),
            );
            assert_eq!(
                dotted["data"]["value"]["value"].as_u64(),
                Some(u64::try_from(index * 10).expect("small")),
                "[seed={seed}] dotted field name must read back via bracket notation"
            );
        }
    }
}

/// Every planted event must be found by the type filter constructed to
/// contain it.
#[test]
fn every_planted_event_is_found_by_its_type_filter() {
    for seed in SEEDS {
        let mut rng = Rng(seed);
        let mut executor = support::executor();
        let mut planted: Vec<(u64, String)> = Vec::new();
        for index in 0..12_u64 {
            let event_type = format!("t{}", rng.next() % 3);
            support::run(
                &mut executor,
                &json!({"type": "event_append", "event_type": event_type, "payload": {"i": index}}),
            );
            planted.push((index, event_type));
        }
        for (sequence, event_type) in &planted {
            let listed = support::run(
                &mut executor,
                &json!({"type": "event_list", "event_type": event_type, "limit": 1000}),
            );
            let contains = listed["data"]["items"]
                .as_array()
                .unwrap_or_else(|| panic!("event_list carries items: {listed}"))
                .iter()
                .any(|item| item["event"]["sequence"].as_u64() == Some(*sequence));
            assert!(
                contains,
                "[seed={seed}] event {sequence} missing from its own type filter `{event_type}`"
            );
        }
    }
}
