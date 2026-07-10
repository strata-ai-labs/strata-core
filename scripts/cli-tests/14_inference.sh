#!/usr/bin/env bash
# Inference surface. The default (lean) build carries the inference commands
# with cloud providers compiled in — an API key at runtime is the only cloud
# requirement — while local model execution (llama.cpp) is the opt-in
# `inference-local` feature. This suite exercises the offline surface for
# whichever flavor STRATA_BIN is: catalog, capability, cache, and the typed
# error envelopes for the paths the build does not carry. Model downloads and
# real generation need the network and a local-enabled build — those flows
# run only when STRATA_E2E_INFERENCE_NETWORK=1. A binary built with
# --no-default-features still gets the cleanly-absent assertions.
source "$(dirname "${BASH_SOURCE[0]}")/harness.sh"

# Provider keys must not leak in from the developer environment: the
# missing-key error path is part of the asserted surface.
unset ANTHROPIC_API_KEY OPENAI_API_KEY GOOGLE_API_KEY GEMINI_API_KEY

run "$DB" init
check_ok "database opens"

# Detect the feature from the advertised command list — `<unknown> --help`
# renders top-level help with exit 0, so probing the subcommand is ambiguous.
if "$STRATA_BIN" --help 2>/dev/null | grep -qE '^\s+inference\b'; then
  echo "[$SUITE_NAME] inference-enabled binary detected"

  run "$DB" inference --help
  check_ok "inference help renders"
  check_contains "help lists models family" "models" "$OUT"
  check_contains "help lists generate" "generate" "$OUT"

  # Catalog is embedded — works offline in every flavor.
  run --json "$DB" inference models list
  check_ok "models list succeeds"
  count="$(json_field '["data"]["items"].__len__()' 2>/dev/null || echo 0)"
  if [[ "$count" -ge 10 ]]; then _ok; else _fail "catalog lists the model registry" "items=$count"; fi

  run --json "$DB" inference models local
  check_ok "models local succeeds offline"

  expect_json "capability reports a generation model" '["data"]["can_generate"]' true \
    -- "$DB" inference capability tinyllama

  expect_json "cache starts empty" '["data"]["generation_models"].__len__()' 0 \
    -- "$DB" inference cache-status

  run "$DB" inference unload
  check_ok "unload with empty cache is healthy"

  # Error envelope: keyless cloud calls carry the stable missing-key code.
  expect_error_code "keyless anthropic call teaches the missing key" \
    "inference.missing_api_key" \
    -- "$DB" inference generate anthropic:claude-3-5-haiku-20241022 "hi"

  # Flavor split: local builds resolve local model specs (unknown names are
  # missing_model); cloud-only builds refuse the local provider entirely and
  # refuse pulls with the download_disabled precondition.
  run --json "$DB" inference capability tinyllama
  local_enabled="$(json_field '["data"]["provider_feature_enabled"]' 2>/dev/null || echo false)"
  if [[ "$local_enabled" == "true" ]]; then
    expect_error_code "unknown local model yields inference.missing_model" \
      "inference.missing_model" -- "$DB" inference generate no-such-model-xyz "hi"
  else
    expect_error_code "cloud-only build refuses local execution" \
      "inference.unsupported_operation" -- "$DB" inference generate no-such-model-xyz "hi"
    expect_error_code "cloud-only build refuses model pulls" \
      "inference.download_disabled" -- "$DB" inference models pull tinyllama
  fi

  if [[ "$local_enabled" == "true" && "${STRATA_E2E_INFERENCE_NETWORK:-0}" == "1" ]]; then
    echo "[$SUITE_NAME] network flows enabled (STRATA_E2E_INFERENCE_NETWORK=1)"

    run --json "$DB" inference models pull tinyllama
    check_ok "models pull tinyllama"
    check_contains "pull reports a gguf path" "gguf" "$OUT"

    run --json "$DB" inference generate tinyllama "Strata is" --max-tokens 8 --seed 7
    check_ok "generate produces an envelope"
    completion="$(json_field '["data"]["completion_tokens"]' 2>/dev/null || echo 0)"
    if [[ "$completion" -ge 1 ]]; then _ok; else _fail "generation emits tokens" "completion=$completion"; fi

    run --json "$DB" inference tokenize tinyllama "hello world"
    check_ok "tokenize succeeds"
  else
    echo "  SKIP: local + STRATA_E2E_INFERENCE_NETWORK=1 for pull/generate flows."
  fi
else
  echo "[$SUITE_NAME] featureless build: inference surface is cleanly absent"
  run "$DB" inference generate "hello"
  if [[ $STATUS -ne 0 ]]; then _ok; else _fail "inference is absent without features" "exit=0 out=$OUT"; fi
  run command print '{"type":"inference_text"}'
  if [[ $STATUS -ne 0 ]]; then _ok; else _fail "inference commands do not parse without features" "exit=0 out=$OUT"; fi
fi

finish
