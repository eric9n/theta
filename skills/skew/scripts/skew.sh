#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEFAULT_THETA_ROOT="${HOME}/theta"
THETA_ROOT="${THETA_ROOT:-${DEFAULT_THETA_ROOT}}"
THETA_BIN_DIR="${THETA_BIN_DIR:-${THETA_ROOT}/target/release}"
SKEW_BIN="${THETA_BIN_DIR}/skew"

if [[ ! -x "${SKEW_BIN}" ]]; then
  LEGACY_THETA_ROOT="$(cd "${SCRIPT_DIR}/../../.." && pwd)"
  LEGACY_SKEW_BIN="${LEGACY_THETA_ROOT}/target/release/skew"
  if [[ -x "${LEGACY_SKEW_BIN}" ]]; then
    SKEW_BIN="${LEGACY_SKEW_BIN}"
  fi
fi

if [[ ! -x "${SKEW_BIN}" ]]; then
  echo "Missing skew binary at ${SKEW_BIN}." >&2
  echo "Set THETA_BIN_DIR or THETA_ROOT, or build it in ~/theta:" >&2
  echo "  cargo build --release --bin skew" >&2
  exit 1
fi

exec "${SKEW_BIN}" "$@"
