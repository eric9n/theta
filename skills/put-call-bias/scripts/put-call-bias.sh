#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
THETA_ROOT="$(cd "${SCRIPT_DIR}/../../.." && pwd)"
PUT_CALL_BIAS_BIN="${THETA_ROOT}/target/release/put-call-bias"

if [[ ! -x "${PUT_CALL_BIAS_BIN}" ]]; then
  echo "Missing put-call-bias binary at ${PUT_CALL_BIAS_BIN}." >&2
  echo "Build it first from the theta repository root:" >&2
  echo "  cargo build --release --bin put-call-bias" >&2
  exit 1
fi

exec "${PUT_CALL_BIAS_BIN}" "$@"
