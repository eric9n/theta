#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
THETA_ROOT="$(cd "${SCRIPT_DIR}/../../.." && pwd)"
RELATIVE_EXTREME_BIN="${THETA_ROOT}/target/release/relative-extreme"

if [[ ! -x "${RELATIVE_EXTREME_BIN}" ]]; then
  echo "Missing relative-extreme binary at ${RELATIVE_EXTREME_BIN}." >&2
  echo "Build it first from the theta repository root:" >&2
  echo "  cargo build --release --bin relative-extreme" >&2
  exit 1
fi

exec "${RELATIVE_EXTREME_BIN}" "$@"
