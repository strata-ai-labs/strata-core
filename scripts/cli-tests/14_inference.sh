#!/usr/bin/env bash
# Inference surface. The default build compiles without inference features, so
# this suite verifies the surface is cleanly absent; when a feature-enabled
# binary is supplied via STRATA_BIN, it smoke-tests the command shape instead.
source "$(dirname "${BASH_SOURCE[0]}")/harness.sh"

run "$DB" init
check_ok "database opens"

if "$STRATA_BIN" inference --help >/dev/null 2>&1; then
  echo "[$SUITE_NAME] inference-enabled binary detected"
  run "$DB" inference --help
  check_ok "inference help renders"
  echo "  NOTE: model-dependent behavior (generate/embed) needs provider keys or"
  echo "  local models and is not asserted here."
else
  echo "[$SUITE_NAME] default build: inference surface is cleanly absent"
  run "$DB" inference generate "hello"
  if [[ $STATUS -ne 0 ]]; then _ok; else _fail "inference is absent without features" "exit=0 out=$OUT"; fi
  run command print '{"type":"inference_text"}'
  if [[ $STATUS -ne 0 ]]; then _ok; else _fail "inference commands do not parse without features" "exit=0 out=$OUT"; fi
  echo "  SKIP: build with -p strata-cli-next --features inference-<provider> to exercise inference."
fi

finish
