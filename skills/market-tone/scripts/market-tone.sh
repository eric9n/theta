#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEFAULT_THETA_ROOT="${HOME}/theta"
THETA_ROOT="${THETA_ROOT:-${DEFAULT_THETA_ROOT}}"
THETA_BIN_DIR="${THETA_BIN_DIR:-${THETA_ROOT}/target/release}"
MARKET_TONE_BIN="${THETA_BIN_DIR}/market-tone"

if [[ ! -x "${MARKET_TONE_BIN}" ]]; then
  LEGACY_THETA_ROOT="$(cd "${SCRIPT_DIR}/../../.." && pwd)"
  LEGACY_MARKET_TONE_BIN="${LEGACY_THETA_ROOT}/target/release/market-tone"
  if [[ -x "${LEGACY_MARKET_TONE_BIN}" ]]; then
    MARKET_TONE_BIN="${LEGACY_MARKET_TONE_BIN}"
  fi
fi

if [[ ! -x "${MARKET_TONE_BIN}" ]]; then
  echo "Missing market-tone binary at ${MARKET_TONE_BIN}." >&2
  echo "Set THETA_BIN_DIR or THETA_ROOT, or build it in ~/theta:" >&2
  echo "  cargo build --release --bin market-tone" >&2
  exit 1
fi

exec "${MARKET_TONE_BIN}" "$@"
