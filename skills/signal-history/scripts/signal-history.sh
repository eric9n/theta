#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
THETA_ROOT="$(cd "${SCRIPT_DIR}/../../.." && pwd)"
SIGNAL_HISTORY_BIN="${THETA_ROOT}/target/release/signal-history"

if [[ ! -x "${SIGNAL_HISTORY_BIN}" ]]; then
  echo "Missing signal-history binary at ${SIGNAL_HISTORY_BIN}." >&2
  echo "Build it first from the theta repository root:" >&2
  echo "  cargo build --release --bin signal-history" >&2
  exit 1
fi

exec "${SIGNAL_HISTORY_BIN}" "$@"
