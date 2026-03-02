#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEFAULT_THETA_ROOT="${HOME}/theta"
THETA_ROOT="${THETA_ROOT:-${DEFAULT_THETA_ROOT}}"
THETA_BIN_DIR="${THETA_BIN_DIR:-${THETA_ROOT}/target/release}"
RELATIVE_EXTREME_BIN="${THETA_BIN_DIR}/relative-extreme"

if [[ ! -x "${RELATIVE_EXTREME_BIN}" ]]; then
  LEGACY_THETA_ROOT="$(cd "${SCRIPT_DIR}/../../.." && pwd)"
  LEGACY_RELATIVE_EXTREME_BIN="${LEGACY_THETA_ROOT}/target/release/relative-extreme"
  if [[ -x "${LEGACY_RELATIVE_EXTREME_BIN}" ]]; then
    RELATIVE_EXTREME_BIN="${LEGACY_RELATIVE_EXTREME_BIN}"
  fi
fi

if [[ ! -x "${RELATIVE_EXTREME_BIN}" ]]; then
  echo "Missing relative-extreme binary at ${RELATIVE_EXTREME_BIN}." >&2
  echo "Set THETA_BIN_DIR or THETA_ROOT, or build it in ~/theta:" >&2
  echo "  cargo build --release --bin relative-extreme" >&2
  exit 1
fi

exec "${RELATIVE_EXTREME_BIN}" "$@"
