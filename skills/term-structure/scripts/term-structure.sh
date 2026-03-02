#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEFAULT_THETA_ROOT="${HOME}/theta"
THETA_ROOT="${THETA_ROOT:-${DEFAULT_THETA_ROOT}}"
THETA_BIN_DIR="${THETA_BIN_DIR:-${THETA_ROOT}/target/release}"
TERM_STRUCTURE_BIN="${THETA_BIN_DIR}/term-structure"

if [[ ! -x "${TERM_STRUCTURE_BIN}" ]]; then
  LEGACY_THETA_ROOT="$(cd "${SCRIPT_DIR}/../../.." && pwd)"
  LEGACY_TERM_STRUCTURE_BIN="${LEGACY_THETA_ROOT}/target/release/term-structure"
  if [[ -x "${LEGACY_TERM_STRUCTURE_BIN}" ]]; then
    TERM_STRUCTURE_BIN="${LEGACY_TERM_STRUCTURE_BIN}"
  fi
fi

if [[ ! -x "${TERM_STRUCTURE_BIN}" ]]; then
  echo "Missing term-structure binary at ${TERM_STRUCTURE_BIN}." >&2
  echo "Set THETA_BIN_DIR or THETA_ROOT, or build it in ~/theta:" >&2
  echo "  cargo build --release --bin term-structure" >&2
  exit 1
fi

exec "${TERM_STRUCTURE_BIN}" "$@"
