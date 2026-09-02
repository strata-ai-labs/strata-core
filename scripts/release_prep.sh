#!/usr/bin/env bash
set -euo pipefail

# release_prep.sh — Phase 1 of the weekly release train (RFC #3036, slice 1).
#
# Automates the deterministic, single-repo prep for a release: version bump,
# both lockfiles, README version strings, IDL bundle regen + gates, a scaffolded
# CHANGELOG entry, and a version verification. It does NOT commit, open a PR, or
# tag — the human gates (PR merge, tag push, docs deploy) stay manual by design.
#
# Usage:  scripts/release_prep.sh <X.Y.Z>
#
# Idempotent: re-running for the version already in-tree is a no-op that simply
# re-verifies the lockfiles, IDL bundle, and binary version.

die() { printf '\033[31merror:\033[0m %s\n' "$*" >&2; exit 1; }
step() { printf '\033[1m==>\033[0m %s\n' "$*"; }

NEW="${1:-}"
[[ "$NEW" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || die "usage: release_prep.sh <X.Y.Z> (got '${NEW:-}')"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

OLD="$(grep -m1 '^version = "' Cargo.toml | sed -E 's/^version = "([^"]+)".*/\1/')"
[[ -n "$OLD" ]] || die "could not read current [workspace.package] version from Cargo.toml"
step "current version: $OLD  ->  target: $NEW"

# --- skip-if-empty advisory (the train departs only with a full carriage) ---
LAST_TAG="$(git tag --sort=version:refname | grep '^v' | tail -1 || true)"
if [[ -n "$LAST_TAG" ]]; then
  DELTA="$(git rev-list --count "${LAST_TAG}"..HEAD)"
  step "commits since ${LAST_TAG}: ${DELTA}"
  [[ "$DELTA" -gt 0 ]] || printf '\033[33mwarning:\033[0m no commits since %s — is this an empty week?\n' "$LAST_TAG" >&2
fi

# --- 1. version in Cargo.toml (only the literal workspace.package line) ---
if [[ "$OLD" != "$NEW" ]]; then
  step "bump Cargo.toml [workspace.package] version"
  sed -i -E "s/^version = \"${OLD//./\\.}\"/version = \"${NEW}\"/" Cargo.toml
else
  step "Cargo.toml already at ${NEW} (no-op)"
fi

# --- 2. README version strings (badge, pong, status line) — leave the Rust floor ---
# Only meaningful on an actual bump; when OLD==NEW the strings are already correct
# (and a stale-check for OLD would false-match the current NEW strings).
if [[ "$OLD" != "$NEW" ]]; then
  step "bump README.md version strings"
  sed -i -E \
    -e "s/badge\/version-${OLD//./\\.}-/badge\/version-${NEW}-/g" \
    -e "s/pong ${OLD//./\\.}/pong ${NEW}/g" \
    -e "s/Strata ${OLD//./\\.} is released/Strata ${NEW} is released/g" \
    README.md
  if grep -qE "version-${OLD//./\\.}-|pong ${OLD//./\\.}|Strata ${OLD//./\\.} is released" README.md; then
    die "README still has stale ${OLD} version strings — update the sed patterns"
  fi
else
  step "README already at ${NEW} (no-op)"
fi

# --- 3. both lockfiles (workspace-crate version fields only) ---
step "refresh root Cargo.lock"
cargo update --workspace >/dev/null
step "refresh benchmarks/Cargo.lock"
cargo update --manifest-path benchmarks/Cargo.toml \
  -p strata-core -p strata-engine -p strata-executor -p strata-storage >/dev/null

# --- 4. IDL bundle: regenerate then gate ---
IDL=(cargo run -q -p strata-executor --features idl-tooling,inference,testkit --bin strata-idl --)
step "regenerate IDL bundle (4 generators)"
for g in generate generate-cli generate-docs generate-tests; do "${IDL[@]}" "$g" >/dev/null; done
step "gate IDL bundle (6 checks)"
for c in check check-cli check-docs check-tests verify-examples; do
  "${IDL[@]}" "$c" >/dev/null || die "IDL gate failed: $c"
done
# verify-fixtures pins the version field; a bump legitimately moves it. Update,
# then refuse if anything OTHER than a version line changed.
if ! "${IDL[@]}" verify-fixtures >/dev/null 2>&1; then
  step "re-bless response fixtures for the version field"
  "${IDL[@]}" verify-fixtures --update >/dev/null
  NONVER="$(git diff -- crates/executor/tests/fixtures | grep -E '^[+-]' | grep -vE '^[+-][+-]' | grep -vE '"version": "[0-9]+\.[0-9]+\.[0-9]+"' || true)"
  [[ -z "$NONVER" ]] || die "fixture update touched non-version fields — review manually:\n$NONVER"
fi

# --- 5. CHANGELOG scaffold (skip if the section already exists) ---
TODAY="$(date +%F)"
if grep -qE "^## \[${NEW//./\\.}\]" CHANGELOG.md; then
  step "CHANGELOG already has a ${NEW} section (no-op)"
else
  step "scaffold CHANGELOG entry for ${NEW} (DRAFT — edit before committing)"
  if [[ -n "$LAST_TAG" ]]; then
    RANGE="${LAST_TAG}..HEAD"
    COMMITS="$(git log --oneline "${LAST_TAG}..HEAD" | sed 's/^/  /')"
  else
    RANGE="(no prior tag)"
    COMMITS="$(git log --oneline | sed 's/^/  /')"
  fi
  TMP="$(mktemp)"
  {
    awk 'NR==1{print; print ""; exit}' CHANGELOG.md
    cat <<EOF
## [${NEW}] - ${TODAY}

<!-- DRAFT — write user-facing notes, then delete this block and the raw log below. -->

### Added

### Changed

### Fixed

<!-- raw commits ${RANGE}:
${COMMITS}
-->

EOF
    tail -n +2 CHANGELOG.md
  } > "$TMP"
  mv "$TMP" CHANGELOG.md
fi

# --- 6. build + verify the version actually moved ---
step "build strata-cli and verify --version"
cargo build --release -p strata-cli >/dev/null
GOT="$(./target/release/strata --version | awk '{print $2}')"
[[ "$GOT" == "$NEW" ]] || die "strata --version reports '$GOT', expected '$NEW'"

step "prep complete for ${NEW}"
cat <<EOF

  Next (human gates — not automated):
    1. Edit the DRAFT CHANGELOG entry into real user-facing notes.
    2. Review: git diff --stat
    3. Stage explicit paths and open the release-prep PR:
       git checkout -b release/v${NEW}
       git add Cargo.toml Cargo.lock benchmarks/Cargo.lock README.md CHANGELOG.md \\
               crates/executor/idl/v1/generated crates/executor/tests/fixtures/responses/v1/admin
       git commit && git push -u origin release/v${NEW}
EOF
