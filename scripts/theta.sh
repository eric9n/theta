#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEFAULT_THETA_ROOT="${HOME}/theta"
THETA_ROOT="${THETA_ROOT:-${DEFAULT_THETA_ROOT}}"
THETA_BIN_DIR="${THETA_BIN_DIR:-${THETA_ROOT}/target/release}"
THETA_BIN="${THETA_BIN_DIR}/theta"

if [[ ! -x "${THETA_BIN}" ]]; then
  LEGACY_THETA_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
  LEGACY_THETA_BIN="${LEGACY_THETA_ROOT}/target/release/theta"
  if [[ -x "${LEGACY_THETA_BIN}" ]]; then
    THETA_BIN="${LEGACY_THETA_BIN}"
  fi
fi

if [[ ! -x "${THETA_BIN}" ]]; then
  echo "Missing theta binary at ${THETA_BIN}." >&2
  echo "Set THETA_BIN_DIR or THETA_ROOT, or build it in ~/theta:" >&2
  echo "  cargo build --release --bin theta" >&2
  exit 1
fi

exec "${THETA_BIN}" "$@"
