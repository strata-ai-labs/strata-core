#!/usr/bin/env bash
# Runs every CLI end-to-end suite and prints one aggregate summary.
#
#   cargo build -p strata-cli-next
#   scripts/cli-tests/run_all.sh
set -u -o pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
failed_suites=()
total=0

for suite in "$here"/[0-9][0-9]_*.sh; do
  name="$(basename "$suite")"
  echo "================ $name ================"
  if ! "$suite"; then
    failed_suites+=("$name")
  fi
  total=$((total + 1))
  echo
done

echo "========================================"
if [[ ${#failed_suites[@]} -eq 0 ]]; then
  echo "ALL $total SUITES PASSED"
  exit 0
fi
echo "${#failed_suites[@]} of $total suites FAILED:"
printf '  %s\n' "${failed_suites[@]}"
exit 1
