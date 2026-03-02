#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
THETA_ROOT="$(cd "${SCRIPT_DIR}/../../.." && pwd)"
MARKET_EXTREME_BIN="${THETA_ROOT}/target/release/market-extreme"

if [[ ! -x "${MARKET_EXTREME_BIN}" ]]; then
  echo "Missing market-extreme binary at ${MARKET_EXTREME_BIN}." >&2
  echo "Build it first from the theta repository root:" >&2
  echo "  cargo build --release --bin market-extreme" >&2
  exit 1
fi

exec "${MARKET_EXTREME_BIN}" "$@"
