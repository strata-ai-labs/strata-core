# LDBC Graphalytics validation graphs (vendored)

The official validation resources of the LDBC Graphalytics benchmark
(https://github.com/ldbc/ldbc_graphalytics, Apache-2.0, upstream commit
7b8bde76cf7aab5e90b25ecd4b38829e2f98b292, vendored 2026-09-01):
per-kernel input graphs and reference outputs for the six analytics
kernels — BFS, PageRank, WCC, CDLP, LCC, SSSP — in directed and
undirected variants, plus the example graphs.

Reference parameters (from the upstream validation test sources):
- BFS / SSSP: source vertex 1
- PageRank: damping 0.85; 14 iterations (directed), 26 (undirected);
  relative epsilon 1e-4
- CDLP: 5 iterations
- SSSP: relative epsilon 1e-4; unreachable prints `Infinity`

Runner: `crates/executor/tests/graph_conformance.rs` — Strata's six
analytics commands against these references through the real wire
surface. File naming: `<kernel>_<dir|undir>-input` / `-output`
(flattened from the upstream directory tree); sssp inputs are `.v`/`.e`
(weighted). Do not edit; refresh wholesale from upstream.
