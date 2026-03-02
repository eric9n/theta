#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
THETA_ROOT="$(cd "${SCRIPT_DIR}/../../.." && pwd)"
PORTFOLIO_BIN="${THETA_ROOT}/target/release/portfolio"

if [[ ! -x "${PORTFOLIO_BIN}" ]]; then
  echo "Missing portfolio binary at ${PORTFOLIO_BIN}." >&2
  echo "Build it first from the theta repository root:" >&2
  echo "  cargo build --release --bin portfolio" >&2
  exit 1
fi

exec "${PORTFOLIO_BIN}" "$@"
