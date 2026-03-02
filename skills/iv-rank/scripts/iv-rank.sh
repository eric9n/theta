#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
THETA_ROOT="$(cd "${SCRIPT_DIR}/../../.." && pwd)"
IV_RANK_BIN="${THETA_ROOT}/target/release/iv-rank"

if [[ ! -x "${IV_RANK_BIN}" ]]; then
  echo "Missing iv-rank binary at ${IV_RANK_BIN}." >&2
  echo "Build it first from the theta repository root:" >&2
  echo "  cargo build --release --bin iv-rank" >&2
  exit 1
fi

exec "${IV_RANK_BIN}" "$@"
