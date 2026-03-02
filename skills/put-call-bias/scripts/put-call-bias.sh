#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEFAULT_THETA_ROOT="${HOME}/theta"
THETA_ROOT="${THETA_ROOT:-${DEFAULT_THETA_ROOT}}"
THETA_BIN_DIR="${THETA_BIN_DIR:-${THETA_ROOT}/target/release}"
PUT_CALL_BIAS_BIN="${THETA_BIN_DIR}/put-call-bias"

if [[ ! -x "${PUT_CALL_BIAS_BIN}" ]]; then
  LEGACY_THETA_ROOT="$(cd "${SCRIPT_DIR}/../../.." && pwd)"
  LEGACY_PUT_CALL_BIAS_BIN="${LEGACY_THETA_ROOT}/target/release/put-call-bias"
  if [[ -x "${LEGACY_PUT_CALL_BIAS_BIN}" ]]; then
    PUT_CALL_BIAS_BIN="${LEGACY_PUT_CALL_BIAS_BIN}"
  fi
fi

if [[ ! -x "${PUT_CALL_BIAS_BIN}" ]]; then
  echo "Missing put-call-bias binary at ${PUT_CALL_BIAS_BIN}." >&2
  echo "Set THETA_BIN_DIR or THETA_ROOT, or build it in ~/theta:" >&2
  echo "  cargo build --release --bin put-call-bias" >&2
  exit 1
fi

exec "${PUT_CALL_BIAS_BIN}" "$@"
