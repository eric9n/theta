#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEFAULT_THETA_ROOT="${HOME}/theta"
THETA_ROOT="${THETA_ROOT:-${DEFAULT_THETA_ROOT}}"
THETA_BIN_DIR="${THETA_BIN_DIR:-${THETA_ROOT}/target/release}"
PORTFOLIO_BIN="${THETA_BIN_DIR}/portfolio"

if [[ ! -x "${PORTFOLIO_BIN}" ]]; then
  LEGACY_THETA_ROOT="$(cd "${SCRIPT_DIR}/../../.." && pwd)"
  LEGACY_PORTFOLIO_BIN="${LEGACY_THETA_ROOT}/target/release/portfolio"
  if [[ -x "${LEGACY_PORTFOLIO_BIN}" ]]; then
    PORTFOLIO_BIN="${LEGACY_PORTFOLIO_BIN}"
  fi
fi

if [[ ! -x "${PORTFOLIO_BIN}" ]]; then
  echo "Missing portfolio binary at ${PORTFOLIO_BIN}." >&2
  echo "Set THETA_BIN_DIR or THETA_ROOT, or build it in ~/theta:" >&2
  echo "  cargo build --release --bin portfolio" >&2
  exit 1
fi

exec "${PORTFOLIO_BIN}" "$@"
