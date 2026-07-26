//! TCP4.2c — graph differential harness against Neo4j, on the 4.2b corpus
//! rails (`tests/corpus/README.md`).
//!
//! A seeded generator drives the same property-graph workload into Strata
//! (the executor wire surface) and Neo4j, then diffs the tiers both engines
//! provably share:
//!
//! - **Tier A (every step)**: the touched node's existence + properties, and
//!   its outgoing neighbor set as `(edge_type, dst)` pairs.
//! - **Tier B (every 16 steps)**: global node and edge counts, plus the full
//!   outgoing adjacency of every live node.
//!
//! The generated domain is the provable intersection: string node ids, a
//! fixed object type, FLAT scalar properties (strings incl. unicode, i64,
//! bools — Neo4j properties cannot nest, and floats are excluded to avoid
//! representation drift), and a whitelisted edge-type set (Cypher cannot
//! parameterize relationship types). Semantics pinned by probing Strata:
//! node re-add is upsert-with-property-replacement (`SET n = ...`), node
//! removal cascades its edges (`DETACH DELETE`), and an edge to a missing
//! endpoint is rejected (`invalid_argument.engine.graph_edge_endpoint`) —
//! the generator only wires live endpoints, and the rejection rides the
//! corpus as a Strata-side regression case.
//!
//! The Neo4j oracle is a dependency-free HTTP/1.1 client over `TcpStream`
//! (the RESP/`OP_MSG` precedent) speaking the transactional Cypher endpoint:
//! connects to `STRATA_NEO4J_URL` (e.g. `http://127.0.0.1:7474`), wipes the
//! scratch graph, and skips loudly when the env var is absent. Positive
//! controls prove both adapters read their own writes before any divergence
//! is trusted (the #2746 lesson).

use serde_json::{json, Value};

#[path = "parity/support.rs"]
mod support;

#[path = "parity/corpus.rs"]
mod corpus;

/// `SplitMix64` — deterministic, seed-replayable.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
    fn below(&mut self, bound: u64) -> u64 {
        self.next() % bound
    }
}

const GRAPH: &str = "diff";
const OBJECT_TYPE: &str = "Item";
/// Cypher relationship types cannot be parameters; both sides draw from
/// this whitelist and the Cypher strings are format-injected from it only.
const EDGE_TYPES: [&str; 2] = ["link", "ref"];
/// Node-id pool: small enough that removals and re-adds collide often.
const NODE_POOL: u64 = 48;

fn node_id(index: u64) -> String {
    format!("n{index:03}")
}

fn generated_properties(rng: &mut Rng) -> Value {
    let mut map = serde_json::Map::new();
    for name in ["kind", "räng", "live"] {
        if rng.below(3) == 0 {
            continue;
        }
        let value = match rng.below(3) {
            0 => Value::from(i64::try_from(rng.below(1_000_000)).expect("bounded") - 500_000),
            1 => Value::from(format!("s{}", rng.below(1_000))),
            _ => Value::from(rng.below(2) == 0),
        };
        map.insert(name.to_owned(), value);
    }
    Value::Object(map)
}

/// The shared property-graph tier both engines implement.
trait GraphOracle {
    fn add_node(&mut self, id: &str, properties: &Value);
    fn remove_node(&mut self, id: &str);
    fn add_edge(&mut self, src: &str, edge_type: &str, dst: &str);
    fn remove_edge(&mut self, src: &str, edge_type: &str, dst: &str);
    fn node_properties(&mut self, id: &str) -> Option<Value>;
    /// Sorted `(edge_type, dst)` pairs of the node's outgoing edges.
    fn neighbors_out(&mut self, id: &str) -> Vec<(String, String)>;
    fn node_count(&mut self) -> u64;
    fn edge_count(&mut self) -> u64;
}

/// Strata through the executor wire surface (cache mode).
struct StrataGraph(strata_executor::Executor);

impl StrataGraph {
    /// No `graph_create` here: the differential records it as the corpus's
    /// first case (a replayed corpus must carry its own setup), and the
    /// controls test issues it explicitly.
    fn new() -> Self {
        Self(support::executor())
    }
}

impl GraphOracle for StrataGraph {
    fn add_node(&mut self, id: &str, properties: &Value) {
        support::run(
            &mut self.0,
            &json!({
                "type": "graph_add_node", "graph": GRAPH, "node_id": id,
                "properties": properties, "object_type": OBJECT_TYPE,
            }),
        );
    }
    fn remove_node(&mut self, id: &str) {
        support::run(
            &mut self.0,
            &json!({"type": "graph_remove_node", "graph": GRAPH, "node_id": id}),
        );
    }
    fn add_edge(&mut self, src: &str, edge_type: &str, dst: &str) {
        support::run(
            &mut self.0,
            &json!({
                "type": "graph_add_edge", "graph": GRAPH,
                "src": src, "edge_type": edge_type, "dst": dst,
            }),
        );
    }
    fn remove_edge(&mut self, src: &str, edge_type: &str, dst: &str) {
        support::run(
            &mut self.0,
            &json!({
                "type": "graph_remove_edge", "graph": GRAPH,
                "src": src, "edge_type": edge_type, "dst": dst,
            }),
        );
    }
    fn node_properties(&mut self, id: &str) -> Option<Value> {
        let output = support::run(
            &mut self.0,
            &json!({"type": "graph_get_node", "graph": GRAPH, "node_id": id}),
        );
        if output["data"]["found"].as_bool() != Some(true) {
            return None;
        }
        Some(output["data"]["value"]["properties"].clone())
    }
    fn neighbors_out(&mut self, id: &str) -> Vec<(String, String)> {
        let page = support::run(
            &mut self.0,
            &json!({
                "type": "graph_neighbors", "graph": GRAPH, "node_id": id,
                "direction": "outgoing", "limit": 4096,
            }),
        );
        let mut pairs: Vec<(String, String)> = page["data"]["items"]
            .as_array()
            .unwrap_or_else(|| panic!("neighbor page carries items: {page}"))
            .iter()
            .map(|item| {
                (
                    item["edge_type"].as_str().expect("edge type").to_owned(),
                    item["dst"].as_str().expect("dst id").to_owned(),
                )
            })
            .collect();
        pairs.sort();
        pairs
    }
    fn node_count(&mut self) -> u64 {
        let meta = support::run(
            &mut self.0,
            &json!({"type": "graph_get_meta", "graph": GRAPH}),
        );
        meta["data"]["node_count"]
            .as_u64()
            .unwrap_or_else(|| panic!("graph_meta carries node_count: {meta}"))
    }
    fn edge_count(&mut self) -> u64 {
        let meta = support::run(
            &mut self.0,
            &json!({"type": "graph_get_meta", "graph": GRAPH}),
        );
        meta["data"]["edge_count"]
            .as_u64()
            .unwrap_or_else(|| panic!("graph_meta carries edge_count: {meta}"))
    }
}

/// Neo4j over the transactional Cypher HTTP endpoint — a hand-rolled
/// HTTP/1.1 POST per request, `Connection: close`, no auth (the dev
/// container runs `NEO4J_AUTH=none`).
struct NeoGraph {
    host: String,
    /// Per-instance node label: tests share one Neo4j and run in parallel,
    /// so every instance scopes its nodes, wipes, and counts to its own
    /// label (the 4.2b per-connection-collection lesson, Cypher edition).
    label: String,
}

static NEO_INSTANCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

impl NeoGraph {
    fn connect() -> Option<Self> {
        let url = std::env::var("STRATA_NEO4J_URL").ok()?;
        let host = url
            .trim_start_matches("http://")
            .trim_end_matches('/')
            .to_owned();
        let instance = NEO_INSTANCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let label = format!("Item_{}_{instance}", std::process::id());
        let mut oracle = Self { host, label };
        // Scratch-wipe THIS instance's namespace on the shared dev server.
        oracle.cypher(
            &format!("MATCH (n:{}) DETACH DELETE n", oracle.label),
            &json!({}),
        );
        Some(oracle)
    }

    fn cypher(&mut self, statement: &str, parameters: &Value) -> Value {
        use std::io::{Read, Write};
        let body = json!({
            "statements": [{"statement": statement, "parameters": parameters}]
        })
        .to_string();
        let request = format!(
            "POST /db/neo4j/tx/commit HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\n\
             Accept: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            self.host,
            body.len(),
            body
        );
        let mut stream =
            std::net::TcpStream::connect(&self.host).expect("connect to STRATA_NEO4J_URL");
        stream.write_all(request.as_bytes()).expect("send request");
        let mut response = Vec::new();
        stream.read_to_end(&mut response).expect("read response");
        let text = String::from_utf8_lossy(&response);
        let split = text
            .find("\r\n\r\n")
            .unwrap_or_else(|| panic!("malformed HTTP response: {text}"));
        let payload: Value = decode_http_body(&text[split + 4..]);
        let errors = payload["errors"].as_array().expect("errors array");
        assert!(errors.is_empty(), "cypher failed: {statement}: {errors:?}");
        payload
    }

    fn rows(&mut self, statement: &str, parameters: &Value) -> Vec<Value> {
        let payload = self.cypher(statement, parameters);
        payload["results"][0]["data"]
            .as_array()
            .expect("result data")
            .iter()
            .map(|entry| entry["row"].clone())
            .collect()
    }
}

/// The endpoint answers chunked (Transfer-Encoding) or plain; both appear in
/// practice depending on response size.
fn decode_http_body(raw: &str) -> Value {
    if let Ok(value) = serde_json::from_str(raw.trim()) {
        return value;
    }
    // Chunked: join size-prefixed chunks.
    let mut body = String::new();
    let mut rest = raw;
    while let Some(line_end) = rest.find("\r\n") {
        let size = usize::from_str_radix(rest[..line_end].trim(), 16).unwrap_or(0);
        if size == 0 {
            break;
        }
        let start = line_end + 2;
        body.push_str(&rest[start..start + size]);
        rest = &rest[start + size + 2..];
    }
    serde_json::from_str(body.trim())
        .unwrap_or_else(|error| panic!("undecodable HTTP body ({error}): {raw}"))
}

impl GraphOracle for NeoGraph {
    fn add_node(&mut self, id: &str, properties: &Value) {
        // Upsert with full property replacement, mirroring Strata's re-add.
        self.cypher(
            &format!(
                "MERGE (n:{} {{id: $id}}) SET n = $props, n.id = $id",
                self.label
            ),
            &json!({"id": id, "props": properties}),
        );
    }
    fn remove_node(&mut self, id: &str) {
        self.cypher(
            &format!("MATCH (n:{} {{id: $id}}) DETACH DELETE n", self.label),
            &json!({"id": id}),
        );
    }
    fn add_edge(&mut self, src: &str, edge_type: &str, dst: &str) {
        assert!(EDGE_TYPES.contains(&edge_type), "edge type whitelist");
        self.cypher(
            &format!(
                "MATCH (a:{label} {{id: $src}}), (b:{label} {{id: $dst}}) MERGE (a)-[:{edge_type}]->(b)", label = self.label
            ),
            &json!({"src": src, "dst": dst}),
        );
    }
    fn remove_edge(&mut self, src: &str, edge_type: &str, dst: &str) {
        assert!(EDGE_TYPES.contains(&edge_type), "edge type whitelist");
        self.cypher(
            &format!(
                "MATCH (a:{label} {{id: $src}})-[r:{edge_type}]->(b:{label} {{id: $dst}}) DELETE r",
                label = self.label
            ),
            &json!({"src": src, "dst": dst}),
        );
    }
    fn node_properties(&mut self, id: &str) -> Option<Value> {
        let rows = self.rows(
            &format!("MATCH (n:{} {{id: $id}}) RETURN properties(n)", self.label),
            &json!({"id": id}),
        );
        let row = rows.first()?;
        let mut properties = row[0].clone();
        // `id` is our identity key, not a generated property.
        properties
            .as_object_mut()
            .expect("props object")
            .remove("id");
        Some(properties)
    }
    fn neighbors_out(&mut self, id: &str) -> Vec<(String, String)> {
        let mut pairs: Vec<(String, String)> = self
            .rows(
                &format!(
                    "MATCH (n:{label} {{id: $id}})-[r]->(m:{label}) RETURN type(r), m.id",
                    label = self.label
                ),
                &json!({"id": id}),
            )
            .iter()
            .map(|row| {
                (
                    row[0].as_str().expect("rel type").to_owned(),
                    row[1].as_str().expect("dst id").to_owned(),
                )
            })
            .collect();
        pairs.sort();
        pairs
    }
    fn node_count(&mut self) -> u64 {
        self.rows(
            &format!("MATCH (n:{}) RETURN count(n)", self.label),
            &json!({}),
        )[0][0]
            .as_u64()
            .expect("node count")
    }
    fn edge_count(&mut self) -> u64 {
        self.rows(
            &format!(
                "MATCH (:{label})-[r]->(:{label}) RETURN count(r)",
                label = self.label
            ),
            &json!({}),
        )[0][0]
            .as_u64()
            .expect("edge count")
    }
}

/// Generator shadow bookkeeping (drives op choices only — the reference
/// engine, never this list, is the oracle).
#[derive(Default)]
struct Live {
    nodes: Vec<String>,
    edges: Vec<(String, String, String)>,
}

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

#[expect(
    clippy::too_many_lines,
    reason = "the generator loop is one deliberately linear randomized workload"
)]
fn run_differential(seed: u64, ops: u64, mut recorder: Option<&mut Vec<corpus::CorpusCase>>) {
    let Some(mut neo) = NeoGraph::connect() else {
        eprintln!("SKIP: STRATA_NEO4J_URL not set; graph differential needs a live Neo4j");
        return;
    };
    let mut strata = StrataGraph::new();
    let mut rng = Rng(seed);
    let mut live = Live::default();

    let mut record = |strata: &mut StrataGraph, op: Value| {
        let observed = corpus::execute_canonicalized(&mut strata.0, &op);
        if let Some(cases) = recorder.as_deref_mut() {
            cases.push(corpus::CorpusCase {
                op,
                expect: observed.clone(),
            });
        }
        observed
    };

    let _ = record(&mut strata, json!({"type": "graph_create", "graph": GRAPH}));

    for step in 0..ops {
        let touched: String = match rng.below(100) {
            // Add or upsert a node.
            0..=34 => {
                let id = node_id(rng.below(NODE_POOL));
                let properties = generated_properties(&mut rng);
                let _ = record(
                    &mut strata,
                    json!({
                        "type": "graph_add_node", "graph": GRAPH, "node_id": id,
                        "properties": properties, "object_type": OBJECT_TYPE,
                    }),
                );
                neo.add_node(&id, &properties);
                if !live.nodes.contains(&id) {
                    live.nodes.push(id.clone());
                }
                id
            }
            // Wire an edge between live nodes.
            35..=59 if live.nodes.len() >= 2 => {
                #[allow(clippy::cast_possible_truncation)] // pool-bounded index
                let src = live.nodes[rng.below(live.nodes.len() as u64) as usize].clone();
                #[allow(clippy::cast_possible_truncation)] // pool-bounded index
                let dst = live.nodes[rng.below(live.nodes.len() as u64) as usize].clone();
                #[allow(clippy::cast_possible_truncation)] // tiny bound
                let edge_type = EDGE_TYPES[rng.below(EDGE_TYPES.len() as u64) as usize];
                let _ = record(
                    &mut strata,
                    json!({
                        "type": "graph_add_edge", "graph": GRAPH,
                        "src": src, "edge_type": edge_type, "dst": dst,
                    }),
                );
                neo.add_edge(&src, edge_type, &dst);
                let key = (src.clone(), edge_type.to_owned(), dst);
                if !live.edges.contains(&key) {
                    live.edges.push(key);
                }
                src
            }
            // Remove a live edge.
            60..=69 if !live.edges.is_empty() => {
                #[allow(clippy::cast_possible_truncation)] // pool-bounded index
                let index = rng.below(live.edges.len() as u64) as usize;
                let (src, edge_type, dst) = live.edges.remove(index);
                let _ = record(
                    &mut strata,
                    json!({
                        "type": "graph_remove_edge", "graph": GRAPH,
                        "src": src, "edge_type": edge_type, "dst": dst,
                    }),
                );
                neo.remove_edge(&src, &edge_type, &dst);
                src
            }
            // Remove a node — cascades on both sides.
            70..=79 if !live.nodes.is_empty() => {
                #[allow(clippy::cast_possible_truncation)] // pool-bounded index
                let index = rng.below(live.nodes.len() as u64) as usize;
                let id = live.nodes.remove(index);
                let _ = record(
                    &mut strata,
                    json!({"type": "graph_remove_node", "graph": GRAPH, "node_id": id}),
                );
                neo.remove_node(&id);
                live.edges.retain(|(src, _, dst)| src != &id && dst != &id);
                id
            }
            // Read probes on a pool id (live or absent) — recorded, and the
            // Tier A diff below is the primary check.
            _ => {
                let id = node_id(rng.below(NODE_POOL));
                let _ = record(
                    &mut strata,
                    json!({"type": "graph_get_node", "graph": GRAPH, "node_id": id}),
                );
                let _ = record(
                    &mut strata,
                    json!({
                        "type": "graph_neighbors", "graph": GRAPH, "node_id": id,
                        "direction": "outgoing", "limit": 4096,
                    }),
                );
                id
            }
        };

        diff_tiers(&mut strata, &mut neo, &touched, &live, seed, step);
    }
}

/// Tier A (touched node) every step; Tier B (global counts + adjacency)
/// every 16 steps.
fn diff_tiers(
    strata: &mut StrataGraph,
    neo: &mut NeoGraph,
    touched: &str,
    live: &Live,
    seed: u64,
    step: u64,
) {
    assert_eq!(
        strata.node_properties(touched),
        neo.node_properties(touched),
        "seed={seed} step={step}: node {touched} properties diverged"
    );
    assert_eq!(
        strata.neighbors_out(touched),
        neo.neighbors_out(touched),
        "seed={seed} step={step}: node {touched} outgoing neighbors diverged"
    );
    if step % 16 == 15 {
        assert_eq!(
            strata.node_count(),
            neo.node_count(),
            "seed={seed} step={step}: node counts diverged"
        );
        assert_eq!(
            strata.edge_count(),
            neo.edge_count(),
            "seed={seed} step={step}: edge counts diverged"
        );
        for id in &live.nodes {
            assert_eq!(
                strata.neighbors_out(id),
                neo.neighbors_out(id),
                "seed={seed} step={step}: adjacency of {id} diverged"
            );
        }
    }
}

#[test]
fn graph_differential_vs_neo4j() {
    let seeds = env_u64("STRATA_GRAPH_DIFF_SEEDS", 2);
    let ops = env_u64("STRATA_GRAPH_DIFF_OPS", 200);
    for seed in 1..=seeds {
        run_differential(seed, ops, None);
    }
}

/// Sabotage + positive controls: both adapters read their own writes, and a
/// deliberately skewed write is caught (the #2746 lesson — a differential
/// harness must prove it can see divergence before its green runs count).
#[test]
fn graph_adapters_pass_positive_controls() {
    let Some(mut neo) = NeoGraph::connect() else {
        eprintln!("SKIP: STRATA_NEO4J_URL not set");
        return;
    };
    let mut strata = StrataGraph::new();
    support::run(
        &mut strata.0,
        &json!({"type": "graph_create", "graph": GRAPH}),
    );
    let properties = json!({"kind": 7, "live": true});
    strata.add_node("ctl", &properties);
    neo.add_node("ctl", &properties);
    strata.add_node("ctl2", &json!({}));
    neo.add_node("ctl2", &json!({}));
    strata.add_edge("ctl", "link", "ctl2");
    neo.add_edge("ctl", "link", "ctl2");

    // Positive controls: both sides read their own writes.
    assert_eq!(strata.node_properties("ctl"), Some(properties.clone()));
    assert_eq!(neo.node_properties("ctl"), Some(properties));
    assert_eq!(
        strata.neighbors_out("ctl"),
        vec![("link".to_owned(), "ctl2".to_owned())]
    );
    assert_eq!(
        neo.neighbors_out("ctl"),
        vec![("link".to_owned(), "ctl2".to_owned())]
    );

    // Sabotage: skew Neo4j only; the tier compare MUST diverge.
    neo.add_node("ctl", &json!({"kind": 8}));
    assert_ne!(
        strata.node_properties("ctl"),
        neo.node_properties("ctl"),
        "sabotaged property write was invisible to the diff"
    );
}

/// Corpus recorder — `STRATA_CORPUS_RECORD=1` (local, never CI); requires a
/// live Neo4j so every recorded expectation was reference-validated.
#[test]
fn record_graph_corpus() {
    const RECORD_SEEDS: [u64; 4] = [21, 22, 23, 24];
    const RECORD_OPS: u64 = 250;
    if std::env::var("STRATA_CORPUS_RECORD").is_err() {
        return;
    }
    assert!(
        std::env::var("STRATA_NEO4J_URL").is_ok(),
        "corpus recording requires live Neo4j validation (corpus/README.md)"
    );
    for seed in RECORD_SEEDS {
        let mut first = Vec::new();
        run_differential(seed, RECORD_OPS, Some(&mut first));
        let mut second = Vec::new();
        run_differential(seed, RECORD_OPS, Some(&mut second));
        assert_eq!(first.len(), second.len(), "seed {seed}: op count differs");
        for (index, (a, b)) in first.iter().zip(&second).enumerate() {
            assert_eq!(
                a.op, b.op,
                "seed {seed} case {index}: generator is not deterministic"
            );
            assert_eq!(
                a.expect, b.expect,
                "seed {seed} case {index}: output is not deterministic after \
                 canonicalization — a volatile field is missing from the scrub list"
            );
        }
        let name = format!("graph-{seed:04}");
        let contents = corpus::corpus_file_contents(
            &name,
            "graph",
            seed,
            "differential_graph v1",
            "neo4j (record time)",
            &first,
        );
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/corpus")
            .join(format!("{name}.jsonl"));
        std::fs::write(&path, contents).expect("write corpus file");
        eprintln!("recorded {} cases to {}", first.len(), path.display());
    }
}
