#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
THETA_ROOT="$(cd "${SCRIPT_DIR}/../../.." && pwd)"
SKEW_BIN="${THETA_ROOT}/target/release/skew"

if [[ ! -x "${SKEW_BIN}" ]]; then
  echo "Missing skew binary at ${SKEW_BIN}." >&2
  echo "Build it first from the theta repository root:" >&2
  echo "  cargo build --release --bin skew" >&2
  exit 1
fi

exec "${SKEW_BIN}" "$@"
