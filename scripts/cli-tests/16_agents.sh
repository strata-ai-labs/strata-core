#!/usr/bin/env bash
# The self-describing agent surface (first-run D3): guide, command catalog,
# error registry, and repo onboarding.
source "$(dirname "${BASH_SOURCE[0]}")/harness.sh"

echo "[$SUITE_NAME] the guide is offline, version-matched markdown"
run agents guide
check_ok "guide renders without a database"
check_contains "guide states the binary version" "$("$STRATA_BIN" --version | awk '{print $2}')" "$OUT"
check_contains "guide teaches database targeting" "STRATA_DB" "$OUT"
check_contains "guide teaches the refusal code" "invalid_argument.cli.no_database" "$OUT"
check_contains "guide teaches branches" "branch fork" "$OUT"
check_contains "guide teaches time travel" "--as-of" "$OUT"
check_contains "guide includes the command catalog" "## Command catalog" "$OUT"
check_contains "guide points at the error registry" "agents errors" "$OUT"

echo "[$SUITE_NAME] the command catalog is machine-readable"
run --json agents commands
check_ok "catalog renders"
check_eq "catalog carries commands" "yes" "$(python3 -c '
import json,sys
d=json.load(sys.stdin)
assert d["type"]=="agents_commands"
print("yes" if len(d["data"]["commands"])>0 else "no")' <<<"$OUT")"

echo "[$SUITE_NAME] the error registry is machine-readable"
run --json agents errors
check_ok "registry renders"
check_eq "every error carries code/class/hint/ref" "ok" "$(python3 -c '
import json,sys
d=json.load(sys.stdin)
errors=d["data"]["errors"]
assert d["data"]["count"]==len(errors) and errors
for e in errors:
    assert e["code"] and e["class"] and e["hint"]
    assert e["ref"]=="https://stratadb.org/e/"+e["code"], e["ref"]
print("ok")' <<<"$OUT")"

echo "[$SUITE_NAME] repo onboarding"
repo="$WORK_DIR/agents-repo"
mkdir -p "$repo"
cd "$repo" || exit 1
printf '# My repo\n' > CLAUDE.md
run --json agents init
check_ok "agents init runs"
check_eq "pointer is pending without --apply" "pending" "$(json_field '["data"]["pointer"]')"
if [[ -f .strata/AGENTS.md ]]; then _ok; else _fail "writes .strata/AGENTS.md" "missing file"; fi
run --json agents init --apply
check_eq "--apply appends the pointer" "appended" "$(json_field '["data"]["pointer"]')"
run --json agents init --apply
check_eq "re-runs are idempotent" "present" "$(json_field '["data"]["pointer"]')"
check_eq "the pointer block lands exactly once" "1" "$(grep -c '## Strata' CLAUDE.md)"
check_contains "the block teaches the guide command" "strata agents guide" "$(cat CLAUDE.md)"

finish
