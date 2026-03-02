#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
THETA_ROOT="$(cd "${SCRIPT_DIR}/../../.." && pwd)"
MARKET_TONE_BIN="${THETA_ROOT}/target/release/market-tone"

if [[ ! -x "${MARKET_TONE_BIN}" ]]; then
  echo "Missing market-tone binary at ${MARKET_TONE_BIN}." >&2
  echo "Build it first from the theta repository root:" >&2
  echo "  cargo build --release --bin market-tone" >&2
  exit 1
fi

exec "${MARKET_TONE_BIN}" "$@"
