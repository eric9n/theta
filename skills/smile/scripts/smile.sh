#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEFAULT_THETA_ROOT="${HOME}/theta"
THETA_ROOT="${THETA_ROOT:-${DEFAULT_THETA_ROOT}}"
THETA_BIN_DIR="${THETA_BIN_DIR:-${THETA_ROOT}/target/release}"
SMILE_BIN="${THETA_BIN_DIR}/smile"

if [[ ! -x "${SMILE_BIN}" ]]; then
  LEGACY_THETA_ROOT="$(cd "${SCRIPT_DIR}/../../.." && pwd)"
  LEGACY_SMILE_BIN="${LEGACY_THETA_ROOT}/target/release/smile"
  if [[ -x "${LEGACY_SMILE_BIN}" ]]; then
    SMILE_BIN="${LEGACY_SMILE_BIN}"
  fi
fi

if [[ ! -x "${SMILE_BIN}" ]]; then
  echo "Missing smile binary at ${SMILE_BIN}." >&2
  echo "Set THETA_BIN_DIR or THETA_ROOT, or build it in ~/theta:" >&2
  echo "  cargo build --release --bin smile" >&2
  exit 1
fi

exec "${SMILE_BIN}" "$@"
