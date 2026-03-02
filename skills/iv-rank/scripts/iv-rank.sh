#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEFAULT_THETA_ROOT="${HOME}/theta"
THETA_ROOT="${THETA_ROOT:-${DEFAULT_THETA_ROOT}}"
THETA_BIN_DIR="${THETA_BIN_DIR:-${THETA_ROOT}/target/release}"
IV_RANK_BIN="${THETA_BIN_DIR}/iv-rank"

if [[ ! -x "${IV_RANK_BIN}" ]]; then
  LEGACY_THETA_ROOT="$(cd "${SCRIPT_DIR}/../../.." && pwd)"
  LEGACY_IV_RANK_BIN="${LEGACY_THETA_ROOT}/target/release/iv-rank"
  if [[ -x "${LEGACY_IV_RANK_BIN}" ]]; then
    IV_RANK_BIN="${LEGACY_IV_RANK_BIN}"
  fi
fi

if [[ ! -x "${IV_RANK_BIN}" ]]; then
  echo "Missing iv-rank binary at ${IV_RANK_BIN}." >&2
  echo "Set THETA_BIN_DIR or THETA_ROOT, or build it in ~/theta:" >&2
  echo "  cargo build --release --bin iv-rank" >&2
  exit 1
fi

exec "${IV_RANK_BIN}" "$@"
