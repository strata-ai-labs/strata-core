# StrataHub Product Direction

Status: Draft product direction

This document defines the product direction for StrataHub so the rest of the
Strata product and architecture documents can refer to a stable target. It is
not an implementation plan, launch plan, pricing plan, or protocol spec.

StrataHub should be understood as the optional network and coordination layer
around embedded Strata databases. Strata itself must remain useful without
StrataHub.

## Product Thesis

StrataHub is the discovery, distribution, and control plane for Strata
databases.

It has two product pillars:

1. StrataHub Library.
   A place to discover, publish, clone, fork, and share curated Strata datasets.
   This solves the cold-start problem: users can begin from a useful dataset
   instead of building one from scratch.

2. StrataHub Fleet.
   A private control plane for the Strata databases a user or team already runs.
   This solves the visibility and operations problem: users can see where their
   databases exist, what state they are in, and whether they need attention.

These pillars are related but separable. The Library creates network effects and
dataset distribution. The Fleet layer creates operational trust for users who
deploy Strata across laptops, cloud VMs, edge machines, serverless compute,
object storage, browser targets, agent sandboxes, and other environments.

## Non-Negotiables

1. Strata must work without StrataHub.
   StrataHub is optional. A local Strata database must be usable offline and
   without an account.

2. StrataHub is not the primary database runtime.
   StrataHub should not turn embedded Strata into a hosted database dependency.
   It may host bundles, metadata, indexes, and control-plane state, but normal
   application reads and writes should go through Strata instances.

3. Clone produces user-owned Strata state.
   After a user clones a dataset, the destination is a normal Strata database
   under the user's control. The source is not a runtime dependency.

4. Fleet visibility is opt-in.
   Strata databases must not silently register, phone home, upload data, or
   expose health metadata to StrataHub without explicit user or organization
   configuration.

5. Sync is explicit and policy-driven.
   StrataHub should not create hidden automatic replication semantics. If sync
   exists, users should understand what branches, spaces, records, and policies
   are involved.

6. Strata owns the data model.
   StrataHub can expose, index, and coordinate Strata datasets, but it should
   not define storage correctness, branch semantics, primitive semantics, or
   backend capability rules.

## GitHub Analogy

The original analogy is useful but should be applied carefully:

1. Git is useful without GitHub. Strata must be useful without StrataHub.
2. GitHub made repositories discoverable, cloneable, forkable, reviewable, and
   collaborative. StrataHub can do this for Strata datasets.
3. GitHub became workflow infrastructure over time. StrataHub can evolve into
   fleet visibility, backup, sync, governance, and deployment coordination.
4. GitHub does not replace local Git. StrataHub should not replace embedded
   Strata.

The best summary is:

> StrataHub does for Strata datasets and instances what GitHub did for Git
> repositories and workflows, without making the local tool dependent on the
> hosted service.

## Pillar 1: StrataHub Library

The Library is a dataset discovery and distribution product.

Users should be able to:

1. Search for datasets by topic, domain, license, size, primitives used,
   embedding status, branch history, update cadence, provenance, and quality
   signals.
2. Inspect dataset metadata before downloading: description, schema summary,
   data capabilities, branch list, tags, source, license, size, version,
   examples, and warnings.
3. Clone a dataset into a chosen Strata database location.
4. Branch, fork, modify, index, compact, export, and share the cloned database
   locally.
5. Publish a dataset bundle, dataset release, or derived dataset back to
   StrataHub when they choose.
6. Create public, private, or organization-scoped datasets.
7. Track provenance between a published dataset, cloned instances, derived
   bundles, and later published forks.

The core cold-start workflow should be:

```text
strata clone <source> <destination>
Strata.open("<destination>")
```

The source may be a StrataHub URL, local file, HTTP URL, object-storage URL, or
other supported source. The destination may be a local path or supported storage
address. After clone, the destination must behave like any other Strata
database.

### Library Product Shape

StrataHub Library should eventually support:

1. Dataset pages.
2. Dataset search.
3. Versioned releases.
4. Branch previews.
5. Clone URLs.
6. Dataset forks.
7. Licenses and usage terms.
8. Provenance graph.
9. Example queries and snippets.
10. Quality and trust metadata.
11. Optional generated previews or summaries.
12. Security, privacy, and PII warnings where applicable.

### Library Non-Goals

The Library should not require:

1. Live remote reads for normal Strata usage after clone.
2. Users to publish private data to get value from Strata.
3. A single canonical hosted dataset for every topic.
4. StrataHub to understand every record semantically before it can distribute a
   dataset.

## Pillar 2: StrataHub Fleet

The Fleet layer is an opt-in control plane for deployed Strata databases.

Users should be able to see:

1. Which Strata databases exist.
2. Where they are located.
3. Which backend each database uses.
4. Which Strata version, storage format, and engine version they run.
5. Which capabilities the backend supports.
6. Which branches, spaces, and major data capabilities are present.
7. Whether the database is healthy.
8. Whether durability, checkpoint, retention, backup, or sync tasks need
   attention.
9. Whether an instance is stale, divergent, unreachable, low on space, or
   running an incompatible version.

The Fleet layer should help users operate Strata across many environments
without making those environments dependent on a central runtime.

### Fleet Product Shape

StrataHub Fleet should eventually support:

1. Instance registry.
2. Health and status dashboard.
3. Backend capability inventory.
4. Version and format drift detection.
5. Backup and restore visibility.
6. Dataset and backup bundle exchange.
7. Optional sync policies.
8. Alerts and audit logs.
9. Labels, environments, ownership, and access control.
10. Organization-level governance.

### Fleet Non-Goals

The Fleet layer should not be:

1. A mandatory database coordinator.
2. A global lock manager for all Strata writes.
3. An always-on replication service by default.
4. A hidden telemetry channel.
5. A substitute for local durability and recovery.

## Shared Product Concepts

The Library and Fleet layers need a shared vocabulary.

### Dataset ID

A dataset ID identifies a logical published dataset or dataset family. It may
have releases, branches, forks, and provenance.

### Bundle ID

A bundle ID identifies a portable artifact: a dataset bundle, database bundle,
snapshot bundle, backup bundle, or other cloneable Strata package.

### Instance ID

An instance ID identifies a concrete Strata database after it has been created
or cloned. Cloning a dataset should create a distinct instance ID while
preserving provenance back to the source dataset or bundle.

### Backend Identity

Backend identity describes where and how an instance is stored: local
filesystem, browser cache, object storage, OpenDAL-backed service, serverless
temporary storage, or another supported substrate.

### Capability Report

A capability report describes what the backend and runtime can safely do:
durability mode, locking, atomic publish, compare-and-swap, listing, retention,
snapshot support, object-size limits, latency class, and unsupported features.

### Health Report

A health report describes current operational state: open mode, branch state,
checkpoint state, WAL state, retention status, last successful export or backup,
error state, and version information.

### Provenance

Provenance records where a dataset or instance came from: source bundle,
source dataset, source branch, clone time, publish time, license, creator, and
optional transformation history.

## Evolution Plan

StrataHub should evolve in stages. The stages are product stages, not
implementation milestones.

### Stage 0: V1 Substrate

This is the part V1 Strata must enable even if StrataHub itself is not launched.

V1 should define:

1. Portable `.strata` dataset or bundle semantics.
2. `strata clone <source> <destination>` as the cold-start workflow.
3. Database identity.
4. Instance identity.
5. Dataset and bundle metadata.
6. Provenance metadata.
7. Backend capability reporting.
8. Health reporting.
9. Export, import, and bundle validation.
10. A storage backend contract that can support local filesystem, browser/cache,
    object storage, and OpenDAL-backed adapters where semantics allow.

### Stage 1: Dataset Library

StrataHub can launch first as a dataset discovery and distribution product.

This stage should support:

1. Dataset pages.
2. Public and private dataset visibility.
3. Clone URLs.
4. Dataset metadata search.
5. Versioned releases.
6. Basic provenance.
7. Publishing bundles.
8. Forking dataset metadata.

This stage creates the network effect: useful datasets make Strata more useful,
and Strata's branchable local model makes datasets more useful after download.

### Stage 2: Private Fleet Registry

The Fleet layer should start with read-only, opt-in registration.

This stage should support:

1. Registering a Strata instance.
2. Showing backend, version, capability, and health metadata.
3. Grouping instances by project, environment, owner, and label.
4. Showing last-seen status.
5. Reporting warnings without uploading user data.

This stage should answer: "Where are all my Strata databases, and are they
healthy?"

### Stage 3: Backup, Restore, And Bundle Exchange

Once bundle semantics are solid, StrataHub can coordinate movement of data.

This stage should support:

1. Uploading backup bundles.
2. Restoring from backup bundles.
3. Publishing dataset bundles.
4. Pulling dataset bundles.
5. Validating bundle compatibility before restore or import.
6. Reporting backup and restore status across a fleet.

### Stage 4: Explicit Sync

Sync should come after branch, bundle, identity, and conflict semantics are
well-understood.

This stage should support:

1. Push branch.
2. Pull branch.
3. Compare branches.
4. Merge or reject with explicit conflicts.
5. Define sync policies.
6. Audit sync actions.

Sync should not be magic. It should feel closer to branch-aware push and pull
than invisible database replication.

### Stage 5: Governance And Collaboration

The mature product can add team workflows:

1. Organization accounts.
2. Access control.
3. Dataset approvals.
4. Audit trails.
5. Retention policies.
6. Deployment policy checks.
7. Fleet alerts.
8. Compliance exports.

## V1 Requirements Implied By StrataHub

Even though StrataHub is not required for V1 launch, V1 architecture should
prepare the following:

1. Clone as a product pathway.
2. Dataset bundle metadata.
3. Database, instance, and bundle identity.
4. Provenance fields that survive clone and export.
5. Backend capability reporting.
6. Health reporting.
7. Bundle validation.
8. Stable errors for unsupported backend capabilities.
9. No hidden network dependency in core Strata.
10. Clear separation between local database correctness and optional control
    plane metadata.

These requirements should feed the V1 feature inventory, user pathways, NFRs,
storage-next architecture, engine-next architecture, and CLI design.

## Privacy, Trust, And Safety

StrataHub will handle user datasets and operational metadata. That makes privacy
and trust product requirements from the beginning.

The product must make clear:

1. What data is uploaded.
2. What metadata is uploaded.
3. Whether dataset contents are public, private, or organization-scoped.
4. Whether an instance is registered with Fleet.
5. Whether health reports include record-level information.
6. How secrets and credentials are handled.
7. How dataset licenses and provenance are displayed.
8. How users delete published datasets, bundles, backups, and instance records.

Default posture:

1. No automatic data upload.
2. No automatic fleet registration.
3. No hidden telemetry.
4. Explicit publish.
5. Explicit register.
6. Explicit sync.

## Product Boundaries

StrataHub should depend on Strata concepts, not the other way around.

Strata V1 should expose enough identity, clone, bundle, capability, and health
surface for StrataHub to become possible. It should not embed StrataHub-specific
business logic into storage or engine internals.

The right boundary is:

1. Strata defines local database behavior, bundle semantics, backend capability
   semantics, branch semantics, and product APIs.
2. StrataHub indexes, distributes, observes, and coordinates those databases and
   bundles when users opt in.
3. StrataHub may provide hosted conveniences, but the local Strata instance
   remains the unit of use.

## Open Questions

The follow-up product documents should answer:

1. What exact artifact is a `.strata` dataset: full database bundle, branch
   release, snapshot bundle, backup bundle, or a family of bundle types?
2. What metadata is mandatory for clone, and what metadata is optional?
3. Does clone preserve the source database ID, or mint a new instance ID with
   provenance pointing back to the source?
4. What is the minimum publish flow for a user-generated dataset?
5. What license and provenance metadata must be present before a public dataset
   can appear in the Library?
6. What backend capability report is required before a database can register
   with Fleet?
7. What health metadata can be shared without leaking user data?
8. How should StrataHub represent branches and forks in a way that is faithful
   to Strata's local branch semantics?
9. What is the first sync workflow that is valuable enough to build but narrow
   enough to trust?
10. Which StrataHub features are product requirements for V1 substrate, which
    are post-V1 product launches, and which are intentionally deferred?
