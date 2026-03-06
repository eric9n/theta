#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEFAULT_THETA_ROOT="${HOME}/theta"
THETA_ROOT="${THETA_ROOT:-${DEFAULT_THETA_ROOT}}"
THETA_BIN_DIR="${THETA_BIN_DIR:-${THETA_ROOT}/target/release}"
MONITOR_BIN="${THETA_BIN_DIR}/account-monitor"

if [[ ! -x "${MONITOR_BIN}" ]]; then
  LEGACY_THETA_ROOT="$(cd "${SCRIPT_DIR}/../../.." && pwd)"
  LEGACY_MONITOR_BIN="${LEGACY_THETA_ROOT}/target/release/account-monitor"
  if [[ -x "${LEGACY_MONITOR_BIN}" ]]; then
    MONITOR_BIN="${LEGACY_MONITOR_BIN}"
  fi
fi

if [[ ! -x "${MONITOR_BIN}" ]]; then
  echo "Missing account-monitor binary at ${MONITOR_BIN}." >&2
  echo "Set THETA_BIN_DIR or THETA_ROOT, or build it in ~/theta:" >&2
  echo "  cargo build --release --bin account-monitor" >&2
  exit 1
fi

exec "${MONITOR_BIN}" "$@"
