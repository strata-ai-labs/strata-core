# Operations And Interface Pathways

Status: Draft pathway group

This document expands the V1 pathways for import/export, inspection, recovery,
configuration, CLI usage, SDK usage, and agent/sandbox workflows.

## Pathway 32: Import And Export Primitive Data

### Goal

A user moves supported primitive data in and out of Strata through stable
formats such as Arrow where supported.

### Flow

1. Select source or destination data.
2. Choose supported format and branch/space scope.
3. Run import or export.
4. Strata validates schema, types, and capability support.
5. The user receives files, records, or a clear report of skipped/failed data.

### Surface

Import/export APIs, CLI import/export commands, Arrow support, JSON/JSONL/CSV
where supported, branch and space filters, diagnostics.

### Guarantees

Import/export must be explicit, deterministic enough for automation, scoped by
branch and space, and honest about which data capabilities and metadata are
preserved.

### Failures

Unsupported format, schema mismatch, invalid data, unsupported capability,
partial write, output path conflict, and backend IO errors should surface
clearly.

### V1 Decision

Optional.

### Cleanup

Keep primitive import/export where it is reliable. Do not preserve legacy branch
bundle commands as the primary data movement story.

## Pathway 33: Inspect Database State

### Goal

A user runs describe, health, metrics, and durability-counter commands to
understand a database.

### Flow

1. Open a database.
2. Run info, describe, health, or metrics.
3. Strata reports bounded database, branch, storage, index, and durability
   state.
4. The user uses output for debugging, automation, or support.

### Surface

Info, describe, health, metrics, durability counters, structured output, CLI
JSON mode, SDK diagnostics.

### Guarantees

Inspection commands must be bounded, safe in read-only mode, stable enough for
automation, and must not require manual maintenance action during normal use.

### Failures

Corruption, unavailable subsystem, unsupported backend metric, permission
error, stale derived state, and partial health failure should surface with
actionable status.

### V1 Decision

Required.

### Cleanup

Keep inspection as an operational product surface. Remove product dependence on
manual flush, compact, checkpoint, or retention operations.

## Pathway 34: Recover From Ordinary Failures

### Goal

A user reopens after crashes, lock conflicts, unsupported backends, or
configuration errors and receives clear outcomes.

### Flow

1. Open a database after ordinary failure or interruption.
2. Strata validates manifests, WAL, checkpoints, and storage state.
3. Strata runs recovery if supported.
4. Strata reports success, degraded recovery, or clear failure.
5. The user continues or takes documented repair action.

### Surface

Open API, recovery system, health output, error codes, durability diagnostics,
backend capability contract.

### Guarantees

Committed data must recover according to durability mode. Recovery must be
deterministic, must not silently lose data, and must distinguish corruption,
unsupported backend behavior, lock conflict, and configuration failure.

### Failures

WAL corruption, manifest corruption, checkpoint corruption, missing files,
unsupported backend atomicity, stale lock, incompatible config, and permission
errors should surface clearly.

### V1 Decision

Required.

### Cleanup

Keep automatic recovery as normal open behavior. Users should not need to run
manual maintenance to recover from ordinary failures.

## Pathway 35: Configure Strata Safely

### Goal

A user manages runtime config, recipes, credentials, and provider settings
without leaking secrets.

### Flow

1. Create or edit Strata config.
2. Configure storage, runtime, recipes, providers, and model settings.
3. Strata validates config at load or first use.
4. Sensitive values remain redacted in output.
5. The user receives clear errors for invalid or unsupported config.

### Surface

Config files, SDK config structs, CLI config commands, recipe config,
credential handling, redacted diagnostics.

### Guarantees

Config must be validated, secrets must be redacted, defaults must be safe, and
backend/model/provider settings must fail before hidden side effects occur.

### Failures

Invalid TOML or JSON, missing credential, unsupported backend, invalid memory
or durability setting, unknown recipe field, and provider misconfiguration
should surface clearly.

### V1 Decision

Required.

### Cleanup

Keep configuration product-facing but remove historical config modes that no
longer map to V1 product concepts, such as disk-backed cache and follower mode.

## Pathway 36: Run Strata From The CLI

### Goal

A user operates databases from scripts, terminals, and JSON-output automation.

### Flow

1. Choose database path, backend, access mode, or cache mode.
2. Run a CLI command.
3. Strata executes through the same product command boundary as SDK/IPC where
   possible.
4. The user receives human output or structured JSON output.
5. Exit codes and errors are stable enough for scripts.

### Surface

CLI commands, flags, JSON output mode, help text, config loading, command
boundary, shell automation.

### Guarantees

CLI behavior must match product semantics, avoid hidden network behavior, expose
clear errors, and keep automation output stable enough for V1.

### Failures

Invalid arguments, unknown command, database open failure, command failure,
unsupported compiled feature, config error, and output serialization failure
should surface clearly.

### V1 Decision

Required.

### Cleanup

Align CLI help with the V1 product surface. Remove or hide non-pathways such as
public transactions, follower mode, branch bundles, and manual maintenance.

## Pathway 37: Use Strata From Application Code

### Goal

An application embeds Strata through the public SDK without depending on
CLI-only behavior.

### Flow

1. Add Strata as a library dependency.
2. Open a database through SDK APIs.
3. Use typed APIs for data, branch, search, graph, vector, and diagnostics.
4. Handle structured errors.
5. Close or drop the database handle normally.

### Surface

SDK APIs, error types, config structs, typed outputs, docs, examples, tests.

### Guarantees

SDK APIs must expose the same product guarantees as CLI, use stable types where
V1 promises them, avoid forcing users through raw command JSON, and make errors
actionable.

### Failures

Open failure, invalid input, unsupported feature, write conflict, history
unavailable, model failure, backend failure, and serialization failure should
surface through structured errors.

### V1 Decision

Required.

### Cleanup

Make SDK/API shape follow product concepts, not historical executor internals.
Remove public-facing transaction and maintenance surfaces that are not V1
pathways.

## Pathway 38: Use Strata In Agent Or Sandbox Workflows

### Goal

An agent runtime opens, clones, queries, mutates, and inspects local datasets
with explicit filesystem and network behavior.

### Flow

1. Agent receives an allowed database path or clone source.
2. Agent opens or clones the database explicitly.
3. Agent inspects schema, branches, spaces, and capabilities.
4. Agent reads, searches, branches, mutates, or exports within permissions.
5. Agent reports structured results and errors.

### Surface

CLI JSON mode, SDK command boundary, describe output, search/RAG output,
clone/import/export, config, filesystem and network policy docs.

### Guarantees

Agent workflows must be explicit about IO and network effects, provide bounded
inspection, avoid hidden provider calls, and return structured results suitable
for automation.

### Failures

Permission denied, unavailable path, unsupported backend, hidden network
attempt, model/provider not configured, output too large, and command failure
should surface clearly.

### V1 Decision

Required.

### Cleanup

Keep agent support as a product lens for CLI and SDK design. Do not add
agent-only semantics that bypass ordinary database guarantees.
