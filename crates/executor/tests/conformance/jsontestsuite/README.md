# JSONTestSuite (vendored)

The `test_parsing/` corpus of nst/JSONTestSuite — the "Parsing JSON is a
Minefield" suite (https://github.com/nst/JSONTestSuite, seriot.ch/projects/parsing_json.html)
— vendored for TCP4.4a: every case runs through Strata's real wire ingress
(`guard_json_integers` → `from_str::<Command>` → engine validation →
store → read-back) in `crates/executor/tests/json_conformance.rs`.

- Upstream commit: 1ef36fa01286573e846ac449e8683f8833c5b26a
- Vendored: 2026-09-01, 318 files (95 y_, 188 n_, 35 i_) + upstream LICENSE (MIT)
- `y_` must be accepted and round-trip; `n_` must be refused with a typed
  error at some pipeline stage; `i_` (implementation-defined) verdicts are
  PINNED in `../i_verdicts.txt` — Strata's documented parsing contract.
- Do not edit these files; refresh wholesale from upstream and re-bless.
