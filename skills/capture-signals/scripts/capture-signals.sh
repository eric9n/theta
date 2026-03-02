#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
THETA_ROOT="$(cd "${SCRIPT_DIR}/../../.." && pwd)"
CAPTURE_SIGNALS_BIN="${THETA_ROOT}/target/release/capture-signals"

if [[ ! -x "${CAPTURE_SIGNALS_BIN}" ]]; then
  echo "Missing capture-signals binary at ${CAPTURE_SIGNALS_BIN}." >&2
  echo "Build it first from the theta repository root:" >&2
  echo "  cargo build --release --bin capture-signals" >&2
  exit 1
fi

exec "${CAPTURE_SIGNALS_BIN}" "$@"
