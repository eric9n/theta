#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEFAULT_THETA_ROOT="${HOME}/theta"
THETA_ROOT="${THETA_ROOT:-${DEFAULT_THETA_ROOT}}"
THETA_BIN_DIR="${THETA_BIN_DIR:-${THETA_ROOT}/target/release}"
SNAPSHOT_BIN="${THETA_BIN_DIR}/snapshot"

if [[ ! -x "${SNAPSHOT_BIN}" ]]; then
  LEGACY_THETA_ROOT="$(cd "${SCRIPT_DIR}/../../.." && pwd)"
  LEGACY_SNAPSHOT_BIN="${LEGACY_THETA_ROOT}/target/release/snapshot"
  if [[ -x "${LEGACY_SNAPSHOT_BIN}" ]]; then
    SNAPSHOT_BIN="${LEGACY_SNAPSHOT_BIN}"
  fi
fi

if [[ ! -x "${SNAPSHOT_BIN}" ]]; then
  echo "Missing snapshot binary at ${SNAPSHOT_BIN}." >&2
  echo "Set THETA_BIN_DIR or THETA_ROOT, or build it in ~/theta:" >&2
  echo "  cargo build --release --bin snapshot" >&2
  exit 1
fi

exec "${SNAPSHOT_BIN}" "$@"
