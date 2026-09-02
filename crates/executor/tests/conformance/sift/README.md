# SIFT exact-kNN ground truth (vendored)

TCP4.4c: a 2,500-vector subsample of the TEXMEX `siftsmall` corpus
(http://corpus-texmex.irisa.fr — Jégou/Douze/Schmid, the dataset lineage
behind ANN-Benchmarks), with exact top-10 ground truth per metric for all
100 original queries.

- `base-2500x128.u8bin` / `queries-100x128.u8bin`: row-major u8 components
  (SIFT descriptors are integers 0..180 — exactly representable in f32, so
  metric values are precision-unambiguous: dot products stay < 2^24).
- `gt_{euclidean,dot,cosine}.tsv`: `query<TAB>id:score,...` — top-10 per
  query, scores in STRATA's conventions (euclidean = 1/(1+distance),
  cosine clamped [-1,1], dot raw), computed in pure-Python double
  precision (an independent toolchain). Zero boundary ties anywhere, so
  the expected order is strict.
- Oracle validation: before subsampling, the generator's kNN was checked
  against the AUTHORS' `siftsmall_groundtruth.ivecs` on the full 10k base
  (10 queries × top-100 distance profiles, exact match).
- Vendored 2026-09-01. Regeneration: refetch siftsmall, re-run the
  generator (recorded in the PR description), re-validate against the
  authors' ground truth.

Strata's V1 vector search is an exact scan, so recall against this ground
truth must be 1.0 with the exact expected ordering — a conformance
contract, not a benchmark. When a real ANN index lands, this same harness
becomes its recall-regression gate.
