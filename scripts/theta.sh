#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
THETA_BIN_DIR="${THETA_BIN_DIR:-/usr/local/bin}"
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
  echo "Install theta into /usr/local/bin or set THETA_BIN_DIR." >&2
  echo "Fallback for local development:" >&2
  echo "  cargo build --release --bin theta" >&2
  exit 1
fi

exec "${THETA_BIN}" "$@"
