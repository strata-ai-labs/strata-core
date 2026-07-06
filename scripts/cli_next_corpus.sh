#!/usr/bin/env bash
set -euo pipefail

# Scenario corpus for the handwritten V1 Strata CLI.
#
# The command matrix script is a broad command-presence smoke test. This corpus
# is workflow-oriented: each scenario owns a fresh durable database and verifies
# multi-command behavior across process boundaries.

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

if ! command -v python3 >/dev/null 2>&1; then
  echo "python3 is required for JSON assertions" >&2
  exit 2
fi

echo "== Build CLI =="
cargo build -q -p strata-cli-next --bin strata

export STRATA="$ROOT/target/debug/strata"
export CLI_CORPUS_TMP="$(mktemp -d)"
export CLI_CORPUS_FILES="$CLI_CORPUS_TMP/files"
mkdir -p "$CLI_CORPUS_FILES"
export STRATA_HOME="$CLI_CORPUS_TMP/home"

cleanup() {
  rm -rf "$CLI_CORPUS_TMP"
}
trap cleanup EXIT

SCENARIO_DIR="$ROOT/scripts/cli-corpus"

if (($# > 0)); then
  scenarios=()
  for name in "$@"; do
    case "$name" in
      */*) scenarios+=("$name") ;;
      *.sh) scenarios+=("$SCENARIO_DIR/$name") ;;
      *) scenarios+=("$SCENARIO_DIR/$name.sh") ;;
    esac
  done
else
  scenarios=()
  while IFS= read -r scenario; do
    scenarios+=("$scenario")
  done < <(find "$SCENARIO_DIR" -maxdepth 1 -type f -name '[0-9][0-9]_*.sh' | sort)
fi

for scenario in "${scenarios[@]}"; do
  [[ -x "$scenario" ]] || {
    echo "scenario is missing or not executable: $scenario" >&2
    exit 2
  }
  echo
  echo "== Scenario: $(basename "$scenario" .sh) =="
  "$scenario"
done

echo
echo "CLI scenario corpus passed (${#scenarios[@]} scenarios)"
