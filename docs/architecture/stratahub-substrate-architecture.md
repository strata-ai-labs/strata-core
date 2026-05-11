# StrataHub Substrate Architecture

Status: V1 architecture draft

## Purpose

This document defines the StrataHub-compatible substrate that V1 architecture
must preserve before StrataHub itself exists.

StrataHub is the canonical future implementation of two product directions:

1. A dataset library where users discover, clone, publish, fork, and derive
   Strata datasets.
2. A fleet control plane where users or organizations can see the Strata
   databases they have deployed, inspect health and capability metadata, and
   optionally coordinate backup, movement, or sync.

Those directions must not hard-code `stratahub.com` into Strata's lower layers.
A company should be able to run a private hub with the same architecture and no
public StrataHub dependency.

The goal for V1 is substrate, not cloud product launch. V1 should define enough
identity, provenance, clone, bundle, capability, health, and command behavior
that StrataHub and private hub implementations can be built later without
rewriting storage or engine.

## Related Documents

Read this with:

1. `docs/product/stratahub-product-direction.md`
2. `docs/product/strata-v1-product-requirements.md`
3. `docs/product/strata-v1-feature-inventory.md`
4. `docs/product/strata-v1-user-pathways.md`
5. `docs/architecture/strata-v1-architecture.md`
6. `docs/architecture/storage-next-architecture.md`
7. `docs/architecture/core-next-architecture.md`
8. `docs/architecture/storage-next/l9-storage-api-boundary.md`
9. `docs/architecture/v1-error-and-diagnostics-contract.md`
10. `docs/architecture/v1-testing-and-conformance-plan.md`

The product direction document owns the StrataHub product thesis. This document
owns the architecture constraints needed to avoid blocking that thesis.

## Product Scenario

The target future workflow is:

```text
strata clone https://stratahub.com/titanic-dataset.strata ~/Documents/Strata/titanic
Strata.open("~/Documents/Strata/titanic")
```

After clone, the destination is a normal local Strata database. The user can
branch it, modify it, search it, add graph relationships, run retrieval, export
data, and work offline without contacting the source.

Later, after sync is designed as a post-V1 product feature, if the user chooses
to publish or sync:

```text
strata sync -m "v1 changes to titanic dataset"
```

The hub receives a new dataset version, fork, backup, branch update, or other
explicit artifact according to the chosen policy.

If the user runs `strata init` and signs in to a hub, the local installation may
also register opt-in fleet metadata: which databases are known on the machine,
their storage modes, health status, Strata version, backend capabilities, and
sync state. It must not upload row contents, secrets, or private metadata
without explicit configuration.

## Non-Goals

This document does not define:

1. The StrataHub web product.
2. A public hosted service API.
3. A sync protocol.
4. A distributed multi-writer database.
5. CRDT merge semantics.
6. Hosted query execution.
7. OpenDAL durability semantics.
8. CLI syntax beyond illustrative examples.
9. Authentication provider implementation.
10. The final `.strata` byte format.

## Binding Decisions

1. **Hub-neutral architecture.**
   StrataHub is one provider. The substrate must also support private or
   enterprise hub implementations.

2. **Storage stays hub-agnostic.**
   Storage-next must not know datasets, accounts, organizations, hub URLs,
   remotes, auth tokens, fleet registrations, or sync policies.

3. **Engine owns hub-compatible product semantics.**
   Engine-next owns dataset identity, provenance, clone/import/export semantics,
   remote refs, branch conflict behavior, and product diagnostics. Storage may
   expose raw capability, health, and row-native bundle facts through engine.

4. **OpenDAL is not StrataHub.**
   OpenDAL and object-store adapters are storage backend mechanisms. StrataHub
   clone, publish, and sync are product data-movement workflows above engine.
   A StrataHub-hosted dataset may be stored on S3 internally, but that does not
   make S3 a StrataHub protocol.

5. **Clone mints local ownership.**
   Cloning a dataset creates a normal Strata database under the user's control.
   It should mint or assign local instance identity while preserving provenance
   back to the source dataset, bundle, branch, and version.

6. **Sync is explicit and post-V1.**
   V1 must leave room for sync, but it must not add hidden replication,
   background upload, or branch push/pull semantics as an incidental feature.

7. **Auto-sync is opt-in.**
   If auto-sync is added later, it must be user or organization configured,
   observable, pausable, and diagnosable. Storage must not contain hidden
   network loops.

8. **Auth and secrets are outside durable database contents.**
   Hub credentials, provider tokens, signing keys, and refresh tokens must live
   in an explicit credential store or process configuration, not inside storage
   rows, WAL records, manifests, snapshots, or clone bundles.

9. **Fleet visibility is opt-in.**
   A database or machine must not register with a hub by default. Fleet reports
   are metadata reports, not implicit data upload.

10. **Local correctness remains independent.**
    A local Strata database must remain correct, recoverable, and usable when
    the hub is unreachable, unconfigured, disabled, or rejected by policy.

## Identity Model

The hub substrate needs several identities that must not be collapsed into one
field.

### Database Identity

Database identity names a concrete Strata database created or opened by the
local runtime.

Storage-next may own a durable database UUID for durable mode when that identity
is needed for manifests, recovery, diagnostics, or bundle validation. Cache mode
may report only ephemeral runtime identity.

Database identity is not a hub account identity and not a dataset identity.

### Instance Identity

Instance identity names a user-owned local installation of a database after it
is created or cloned. Two clones of the same dataset should have different
instance identities.

Instance identity is product metadata. It belongs above storage. Engine-next may
persist it as engine-owned rows or sidecar metadata, but storage must treat that
metadata as ordinary engine data.

### Dataset Identity

Dataset identity names a logical dataset family in a hub or bundle ecosystem.
It may have releases, forks, branch previews, licenses, provenance, and
visibility rules.

Dataset identity is not storage-native. It belongs to engine/hub metadata.

### Bundle Identity

Bundle identity names a portable artifact: a dataset bundle, database bundle,
backup bundle, snapshot-derived bundle, or release package.

Bundle identity must be stable enough for validation, provenance, retry, and
duplicate detection. It may be content-addressed later, but this document does
not require a specific hash or signing scheme.

### Dataset Version Or Change Identity

Future sync and publish workflows need an idempotent way to name a submitted
change, release, or derived dataset version.

That identity should be engine/hub-level. Storage commit versions remain local
ordering tokens; they are not globally unique dataset version IDs.

### Branch Identity

`BranchId` remains an opaque local identity. It must not become content-rooted
or remote-rooted to satisfy hub workflows.

Remote association belongs in remote refs and provenance metadata, not inside
the branch ID.

### Remote Ref

A remote ref records an association between local state and a remote provider:

1. Provider or hub URL.
2. Dataset identity.
3. Optional remote branch or release name.
4. Last known remote version or change identity.
5. Local branch or database state that corresponds to that remote point.
6. Capability and policy metadata needed to decide whether operations are
   allowed.

Remote refs are engine-owned product metadata. Storage must not inspect them.

### Install Or Machine Identity

Fleet registration needs an identity for a local Strata installation or machine.
This identity is created by CLI, SDK, or product runtime setup such as
`strata init`.

It must not be required for local database correctness. It must not be stored in
clone bundles by default unless the user is intentionally exporting deployment
metadata.

When configured, install or machine identity is engine-owned hub-substrate
metadata under storage-space ID `0x34`. Storage treats it as ordinary engine
data and must not interpret it.

### Account And Organization Identity

Account, user, team, and organization identity are hub provider concepts. They
must not appear in storage-next and should enter engine only through explicit
provider configuration or product metadata.

## Clone Artifact Model

The `.strata` artifact family should be a provider-neutral clone substrate.
The exact byte format belongs in the storage format and bundle specs, but the
architecture should assume the artifact contains:

1. Bundle identity and format version.
2. Required storage format version.
3. Required engine data-capability registry version.
4. Dataset metadata where applicable.
5. Provenance metadata.
6. License and trust metadata where applicable.
7. Backend requirements or constraints.
8. Branches and branch points included in the artifact.
9. Commit timeline bounds for included branch history.
10. Row-native storage snapshots or chunked row data sufficient to reconstruct
    committed storage rows.
11. Engine-owned metadata rows needed to reconstruct data capabilities.
12. Optional derived-state sections that are rebuildable or explicitly
    validated.
13. Checksums, sizes, and compatibility declarations.

The artifact must not contain:

1. Hub credentials.
2. Machine credentials.
3. Provider refresh tokens.
4. Hidden fleet registration state.
5. Unvalidated executable code.

Clone validation must happen before the destination is made usable. Validation
should check format compatibility, required capabilities, declared sizes,
checksums, branch metadata, provenance, and any mandatory engine metadata.

## Clone Flow

The normal clone flow is:

1. Resolve source.
   The source may be a StrataHub URL, private hub URL, local file, HTTP URL, or
   other supported source.

2. Fetch artifact.
   Fetching belongs to CLI, SDK, or a future data-movement layer above engine,
   not to storage-next.

3. Validate artifact.
   Validate bundle identity, format, checksums, capabilities, branch metadata,
   provenance, and compatibility.

4. Create destination database.
   The destination becomes a normal Strata database in a supported storage
   mode.

5. Install committed rows and engine metadata.
   Storage installs row-native committed state through storage-owned mechanics.
   Engine installs product metadata, data-capability state, indexes, or
   rebuildable derived state according to engine rules.

6. Mint local identity.
   The local database and instance receive local identity while preserving
   provenance back to the source.

7. Open offline.
   The destination must open without source or hub access.

## Publish And Sync Direction

Publish and sync are above-engine workflows. They should be designed later as
provider-neutral operations, but the substrate must leave room for them.

The expected shape is:

1. Read local remote refs and provenance.
2. Compute a change set, release bundle, backup bundle, or branch update.
3. Validate the outgoing artifact.
4. Upload content with idempotent content or request identity.
5. Publish a remote ref or dataset version atomically according to provider
   rules.
6. Record the acknowledged remote state locally.

Ambiguous outcomes must be explicit. If upload or publish fails after the
provider may have accepted the change, the error should surface as an
ambiguous-commit style product error and require idempotent retry, status
lookup, or user action.

Storage commit versions are local. Future sync must not assume that two
databases share the same commit-version sequence. It should compare through
engine-owned branch state, provenance, bundle metadata, remote refs, and
content/change identities.

## Auto-Sync Direction

Auto-sync is a policy layer, not a storage layer.

If added later, auto-sync should:

1. Be disabled by default.
2. Require explicit provider configuration.
3. Declare which database, branches, spaces, or datasets are in scope.
4. Use IPC or the engine command boundary when another local owner has the
   database open.
5. Report last attempt, last success, pending changes, provider errors, and
   ambiguous outcomes.
6. Respect offline mode, disabled network policy, read-only mode, and
   organization policy.
7. Never upload secrets or excluded data by default.

Auto-sync should not become a background thread inside storage-next.

## Fleet Metadata Model

Fleet registration is opt-in metadata reporting.

A fleet report may include:

1. Install or machine identity.
2. Strata version.
3. Storage format version.
4. Engine data-capability registry version.
5. Database or instance identity.
6. Backend type and declared capabilities.
7. Storage mode.
8. Health summary.
9. Recovery status.
10. Last-open or last-seen time.
11. Known dataset and remote refs, if configured.
12. Sync status, if configured.
13. Redacted local path or user-approved path metadata.
14. Size, branch count, space count, and index capability summaries.
15. Selected runtime resource profile and effective budget summaries.

A fleet report must not include by default:

1. Row contents.
2. Query contents.
3. Secrets.
4. Provider credentials.
5. Full local paths if the user or organization has not opted into them.
6. Record-level metadata that leaks private data.

Fleet reporting should consume engine-owned health and storage-owned raw health
facts translated through engine. It should not call storage directly during
normal product operation.

## Storage-Next Requirements

Storage-next must support the hub substrate by providing generic mechanics, not
hub semantics.

Storage-next should provide:

1. Durable database identity where needed for storage correctness and
   diagnostics.
2. Row-native snapshot, export, and install mechanics.
3. Commit timeline substrate sufficient for branch-from-time and retained
   history.
4. Backend capability reports.
5. Recovery health facts.
6. Storage format version facts.
7. Storage-space registry enforcement.
8. Deterministic checksums for durable objects and bundle validation.
9. Clear unsupported-capability errors.
10. Resolved storage runtime budget facts for diagnostics.
11. Fault-injection surfaces for clone/install/recovery testing.

Storage-next must not provide:

1. Hub URLs.
2. Remote refs.
3. Dataset accounts or organizations.
4. Sync policy.
5. Fleet registration.
6. Network clients.
7. Credential storage.
8. Branch merge product policy.
9. Dataset search or discovery behavior.
10. Hidden upload or background sync.

## Engine-Next Requirements

Engine-next is the natural owner of hub-compatible product semantics.

Engine-next should provide or reserve:

1. Dataset identity metadata.
2. Instance identity metadata.
3. Bundle identity metadata.
4. Provenance metadata.
5. Remote refs.
6. Clone/import/export product workflows.
7. Bundle validation orchestration.
8. Branch and conflict semantics for future publish/sync.
9. Engine-owned health and capability translation.
10. Product errors for unsupported providers, incompatible bundles, ambiguous
    publish, disabled network, invalid credentials, and policy rejection.
11. A provider-neutral remote abstraction for future hub implementations.
12. Redaction rules for fleet reports and diagnostics.

Engine-next should not force a StrataHub dependency into ordinary local opens.
Hub behavior is optional product behavior layered over normal engine APIs.

## Core-Next Considerations

Core-next should not absorb hub concepts unless a later document proves they
are true cross-layer contracts.

Likely core-next candidates:

1. `BranchId`
2. `CommitVersion`
3. Timestamp vocabulary
4. Stable database identity, if both storage and engine need the exact same
   serialized type

Likely engine-owned concepts:

1. Dataset identity
2. Bundle identity
3. Instance identity
4. Remote refs
5. Provenance records
6. Provider identity
7. Fleet report shape

The burden of proof should remain high. Hub convenience should not turn
core-next into a product metadata crate.

## CLI, SDK, IPC, And Strata AI

The product surfaces that will eventually use this substrate are above engine.

Expected CLI direction:

```text
strata init
strata clone <source> <destination>
strata status
strata health
```

Post-V1 illustrative sync direction:

```text
strata sync -m <message>
```

Expected SDK direction:

1. Open cloned databases as normal Strata databases.
2. Inspect provenance and remote refs.
3. Export or validate bundles.
4. Configure provider credentials explicitly.
5. Disable network behavior explicitly.

Expected IPC direction:

1. Local shared access should go through engine IPC.
2. Strata AI should use IPC or engine product APIs, not direct storage access.
3. Sync tools should use engine command boundaries when a database is already
   owned by another local process.

None of these surfaces should require StrataHub for normal embedded use.

## Private Hub Support

The same substrate must support public StrataHub and private hubs.

Architecture requirements:

1. Remote provider URLs must not be hard-coded to `stratahub.com`.
2. Provider capability negotiation should be explicit.
3. Auth configuration should be provider-specific and outside storage.
4. Dataset, bundle, and fleet metadata should have provider-neutral local
   representations.
5. Error codes should distinguish unsupported provider, disabled network,
   authentication failure, authorization failure, incompatible bundle, and
   ambiguous publish outcomes.
6. Private hubs should be able to disable public discovery while retaining
   clone, publish, fleet, backup, and policy workflows.

## Testing Implications

The V1 test plan should eventually include hub-substrate tests even before the
cloud product exists.

Required test families:

1. Clone from a local `.strata` artifact into a durable database.
2. Clone validation rejects incompatible, corrupt, truncated, oversized, or
   capability-mismatched artifacts before partial install becomes visible.
3. Cloned databases open offline and do not contact the source.
4. Clone mints local identity while preserving provenance.
5. Export/import round trips storage rows, engine metadata, branch state, and
   commit timeline bounds.
6. Fleet report redaction excludes row contents, secrets, and unapproved local
   path detail.
7. Network-disabled mode rejects remote clone/sync/register with stable errors.
8. Fake provider tests exercise idempotent publish and ambiguous publish
   outcomes without a real StrataHub service.
9. Storage tests prove no hub concepts enter storage-next.
10. Engine tests prove provider failures map to product errors without storage
    leakage.

## Acceptance Criteria

The substrate is adequate when:

1. A `.strata` artifact can be described without reference to `stratahub.com`.
2. A cloned dataset becomes a normal local Strata database.
3. Storage-next can implement its layers without hub URLs, auth, remotes,
   accounts, fleet registration, or sync policies.
4. Engine-next has a clear home for dataset identity, instance identity,
   provenance, remote refs, and bundle validation.
5. Future sync can be added above engine without changing storage's core row,
   WAL, manifest, checkpoint, or recovery architecture.
6. Fleet reporting can be implemented as opt-in metadata translation from
   engine and storage health facts.
7. Private hubs can implement the same clone, publish, and fleet workflows as
   public StrataHub.
8. No ordinary local open depends on network availability.

## Open Questions

These questions should be resolved before V1 format and engine-next freeze:

1. What exact artifact family does `.strata` name: full database bundle,
   dataset release, backup bundle, branch-scoped release, or a manifest that
   can point to multiple artifact types?
2. Which hash, signature, and content-addressing scheme names bundle contents?
3. What local metadata schema represents provenance and remote refs?
4. Which local identity is minted during clone, and which source identities are
   preserved only as provenance?
5. Does `strata sync` publish a branch update, dataset release, backup bundle,
   or provider-selected operation in its first version?
6. Does auto-sync live in a CLI daemon, application process, Strata AI process,
   or SDK scheduler?
7. Which credential stores are supported on macOS, Linux, Windows, browser, and
   serverless targets?
8. Which fleet metadata fields are reported by default after opt-in, and which
   require additional user or organization approval?
9. How should hub providers express capability differences without leaking
   provider-specific behavior into engine APIs?
10. What is the minimal fake-provider conformance suite required before any
    hosted StrataHub implementation is trusted?
