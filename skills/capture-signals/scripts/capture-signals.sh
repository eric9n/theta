#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEFAULT_THETA_ROOT="${HOME}/theta"
THETA_ROOT="${THETA_ROOT:-${DEFAULT_THETA_ROOT}}"
THETA_BIN_DIR="${THETA_BIN_DIR:-${THETA_ROOT}/target/release}"
CAPTURE_SIGNALS_BIN="${THETA_BIN_DIR}/capture-signals"

if [[ ! -x "${CAPTURE_SIGNALS_BIN}" ]]; then
  LEGACY_THETA_ROOT="$(cd "${SCRIPT_DIR}/../../.." && pwd)"
  LEGACY_CAPTURE_SIGNALS_BIN="${LEGACY_THETA_ROOT}/target/release/capture-signals"
  if [[ -x "${LEGACY_CAPTURE_SIGNALS_BIN}" ]]; then
    CAPTURE_SIGNALS_BIN="${LEGACY_CAPTURE_SIGNALS_BIN}"
  fi
fi

if [[ ! -x "${CAPTURE_SIGNALS_BIN}" ]]; then
  echo "Missing capture-signals binary at ${CAPTURE_SIGNALS_BIN}." >&2
  echo "Set THETA_BIN_DIR or THETA_ROOT, or build it in ~/theta:" >&2
  echo "  cargo build --release --bin capture-signals" >&2
  exit 1
fi

exec "${CAPTURE_SIGNALS_BIN}" "$@"
