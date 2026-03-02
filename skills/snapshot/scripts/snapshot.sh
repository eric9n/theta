#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
THETA_ROOT="$(cd "${SCRIPT_DIR}/../../.." && pwd)"
SNAPSHOT_BIN="${THETA_ROOT}/target/release/snapshot"

if [[ ! -x "${SNAPSHOT_BIN}" ]]; then
  echo "Missing snapshot binary at ${SNAPSHOT_BIN}." >&2
  echo "Build it first from the theta repository root:" >&2
  echo "  cargo build --release --bin snapshot" >&2
  exit 1
fi

exec "${SNAPSHOT_BIN}" "$@"
