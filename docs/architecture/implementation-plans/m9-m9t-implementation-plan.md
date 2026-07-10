# M9 / M9T Implementation Plan: StrataHub V1 Integration

Status: draft implementation plan

## Goal

Add the strata-core side of the StrataHub V1 read-only library path:
Hub-compatible protocol types, clone transport, `strata clone`, `strata info`,
opt-in telemetry client behavior, and conformance coverage. This milestone
makes public dataset clone and inspection part of the release gate before the
final CLI/cutover milestone.

M9 does not build StrataHub itself. It makes the core Strata database, engine,
and CLI ready to talk to a StrataHub-compatible service. Push, auth, catalog
hosting, dataset publishing, private datasets, forks, multi-branch Hub
collaboration, hosted runtimes, sync, and fleet visibility remain outside this
milestone.

## Inputs

1. `docs/stratahub/docs/product/stratahub-user-pathways.md`
2. `docs/stratahub/docs/product/stratahub-v1-cli-commands.md`
3. `docs/stratahub/docs/product/stratahub-v1-prd.md`
4. `docs/stratahub/docs/product/stratahub-v1-pathways-detailed.md`
5. `docs/stratahub/docs/product/stratahub-dataset-card-spec.md`
6. `docs/stratahub/docs/architecture/stratahub-v1-architecture.md`
7. `docs/stratahub/docs/architecture/stratahub-v1-protocol-spec.md`
8. `docs/stratahub/docs/architecture/stratahub-v1-bundle-format.md`
9. `docs/stratahub/docs/architecture/stratahub-v1-testing-and-ci.md`
10. `docs/architecture/engine/dataset-clone-artifact-contract.md`
11. `docs/architecture/stratahub-substrate-architecture.md`

StrataHub product and architecture documents are inputs only so strata-core can
match the wire contracts it consumes. They do not move StrataHub-owned hosting,
catalog, or publishing responsibilities into this repo.

All slices must follow the V1 engineering standards: permanent domain names,
concept-budget discipline, file/function thresholds, comment standards, and no
roadmap labels in production code vocabulary.

## Implementation Track

| Epic | Title | Scope | Exit gate |
|---|---|---|---|
| `M9A` | Hub protocol and identity types | Add Hub URL resolution, dataset-name validation, manifest/object hash types, dataset card DTOs, remote-ref facts, and protocol error mapping. | Protocol shapes match StrataHub V1 docs and do not depend on storage internals. |
| `M9B` | Clone transport and assembler | Fetch manifests and content-addressed objects, verify hashes, resume partial downloads where supported, assemble a local database atomically, open-check the result, and write the engine-owned `origin` remote ref. | Interrupted or corrupt clone attempts never produce an accepted database. |
| `M9C` | CLI clone, info, config, and telemetry | Add `strata clone`, `strata info`, `hub.default`, `telemetry.enabled`, `--hub`, `--force`, `--format`, and `--field` behavior through the existing CLI conventions. | The V1 CLI exposes exactly the Hub commands in the product doc and no push/auth/list/search Hub verbs. |
| `M9D` | Hub conformance fixture | Provide only the deterministic test double needed for CLI and engine conformance: dataset info, refs, manifests, objects, telemetry capture, and fixtures. | CLI and clone tests can run without relying on the hosted StrataHub service or building a production Hub. |
| `M9E` | Dataset card consumption | Render the README, license, schema, preview, primitive list, requirements, download count, provenance, and clone command fields returned by a Hub-compatible service. | `strata info` output is a faithful CLI rendering of the dataset-card contract without owning catalog generation. |
| `M9F` | Hub neutrality and privacy guardrails | Enforce no hidden network behavior, no hardcoded hosted Hub except the default config value, no forbidden telemetry fields, and no storage-to-Hub imports. | Hub behavior is explicit, opt-in where required, and self-host compatible. |
| `M9G` | StrataHub closeout and cutover handoff | Record the StrataHub V1 conformance result, update cutover prerequisites, docs, and readiness inputs for M10/M11. | M10 cutover can treat Hub clone/info as part of the required public V1 surface. |

## Test Track

| Test epic | Title | Scope | Exit gate |
|---|---|---|---|
| `M9TA` | Protocol conformance tests | Validate URL resolution, dataset-name grammar, hash parsing, manifest references, dataset cards, error DTOs, and remote-ref facts. | Protocol tests fail on drift from StrataHub V1 docs. |
| `M9TB` | Clone atomicity and integrity tests | Cover complete clone, interrupted clone, resume, hash mismatch, manifest/object missing, destination conflict, force overwrite, disk/write failure, and open-check failure. | Clone failures are structured and never leave a trusted partial destination. |
| `M9TC` | CLI clone/info/config tests | Exercise text/json/short/field output, TTY and pipe defaults, `--hub` precedence, `hub.default`, usage errors, and stable exit codes. | CLI output and errors are fixture-stable for users and AI assistants. |
| `M9TD` | Privacy and hub-neutrality tests | Assert telemetry default-off behavior, first-run prompt behavior, payload allowlist, endpoint selection, redaction, and no forbidden host or dataset leaks. | Telemetry cannot collect dataset names, URLs, paths, Hub URLs, error text, or identifying data. |
| `M9TE` | Hub-client endpoint tests | Run the strata-core client against deterministic responses for `/v1/datasets`, `/v1/datasets/<name>`, `/v1/refs/*`, `/v1/manifests/*`, `/v1/objects/*`, and `/v1/telemetry`. | The client consumes the read API required by clone/info without owning Hub server behavior. |
| `M9TF` | End-to-end Hub pathway tests | Run `strata info` and `strata clone` against deterministic Hub fixtures, then open the cloned database and verify normal local use. | V1 cold-start pathway works without the hosted service and without network side effects beyond the selected Hub. |

## Consumed Endpoint Surface

M9 does not own or implement production StrataHub endpoints. It owns the
strata-core client expectations for the read API needed by V1 clone/info:

1. `GET /v1/info`
2. `GET /v1/datasets`
3. `GET /v1/datasets/<name>`
4. `GET /v1/datasets/<name>/refs`
5. `GET /v1/refs/<dataset>/<branch>`
6. `GET /v1/manifests/<hash>`
7. `GET /v1/objects/<hash>`
8. `POST /v1/telemetry`

The deterministic fixture in M9D may expose these paths only to test the
strata-core client. Health, readiness, metrics, catalog filtering/sorting,
download-count mutation, object-store administration, and production serving
remain StrataHub responsibilities unless a separate StrataHub-owned plan says
otherwise.

## CLI Surface

M9 adds exactly the V1 Hub commands:

1. `strata clone <source> <destination> [--hub <url>] [--force] [--format <fmt>]`
2. `strata info <dataset> [--hub <url>] [--format <fmt>] [--field <name>]`

M9 also registers these config keys with the existing config system:

1. `hub.default`, defaulting to `https://stratahub.io`
2. `telemetry.enabled`, defaulting to `false`

No Hub `list`, Hub `search`, `push`, `auth`, `fork`, `deploy`, or `fleet`
commands are V1 work. Local database `search` remains an engine command.

## Convergence Notes

1. `M9A` must land before `M9B`, `M9C`, and `M9D` encode protocol shapes.
2. `M9B` consumes the M6 dataset clone artifact substrate and writes only
   engine-owned remote-ref metadata.
3. `M9C` uses existing CLI config, output, exit-code, and error conventions
   rather than inventing a Hub-specific CLI framework.
4. `M9D` must allow tests to run offline against deterministic fixtures without
   becoming a production Hub implementation.
5. `M9F` lands with the first network-capable code path and remains in the
   closeout gate.
6. `M9G` updates M10 and M11 inputs so cutover/readiness include Hub pathways.

## Slice Policy

Prefer small protocol/client/CLI slices with fixtures checked in before behavior
depends on them. Network code must be explicit in tests and must not run from
ordinary storage or engine operations unless a Hub command selected it.

## Non-Goals

1. No push or upload command.
2. No authentication, API keys, private datasets, organizations, or RBAC.
3. No remote branch collaboration, forks, or pull-request-like flow.
4. No hosted runtime, deploy, sync, pull, or fleet command.
5. No server-side query execution for clone/info.
6. No production StrataHub server, hosted catalog, dataset publishing pipeline,
   object-store administration, download-count mutation, or web UI.
7. No hard dependency on the hosted `stratahub.io` service in tests.
8. No hidden telemetry or telemetry enabled by default.

## Milestone Exit Gate

M9 is complete when Strata can inspect and clone a public StrataHub-compatible
dataset through stable CLI and protocol surfaces, verify the resulting local
database, preserve Hub provenance through engine-owned remote refs, and prove
privacy/hub-neutrality through tests. The roadmap Test Gate Summary remains the
canonical milestone gate; this plan explains how M9 reaches it.
