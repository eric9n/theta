#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEFAULT_THETA_ROOT="${HOME}/theta"
THETA_ROOT="${THETA_ROOT:-${DEFAULT_THETA_ROOT}}"
THETA_BIN_DIR="${THETA_BIN_DIR:-${THETA_ROOT}/target/release}"
SIGNAL_HISTORY_BIN="${THETA_BIN_DIR}/signal-history"

if [[ ! -x "${SIGNAL_HISTORY_BIN}" ]]; then
  LEGACY_THETA_ROOT="$(cd "${SCRIPT_DIR}/../../.." && pwd)"
  LEGACY_SIGNAL_HISTORY_BIN="${LEGACY_THETA_ROOT}/target/release/signal-history"
  if [[ -x "${LEGACY_SIGNAL_HISTORY_BIN}" ]]; then
    SIGNAL_HISTORY_BIN="${LEGACY_SIGNAL_HISTORY_BIN}"
  fi
fi

if [[ ! -x "${SIGNAL_HISTORY_BIN}" ]]; then
  echo "Missing signal-history binary at ${SIGNAL_HISTORY_BIN}." >&2
  echo "Set THETA_BIN_DIR or THETA_ROOT, or build it in ~/theta:" >&2
  echo "  cargo build --release --bin signal-history" >&2
  exit 1
fi

exec "${SIGNAL_HISTORY_BIN}" "$@"
