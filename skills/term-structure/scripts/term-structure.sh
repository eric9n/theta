#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
THETA_ROOT="$(cd "${SCRIPT_DIR}/../../.." && pwd)"
TERM_STRUCTURE_BIN="${THETA_ROOT}/target/release/term-structure"

if [[ ! -x "${TERM_STRUCTURE_BIN}" ]]; then
  echo "Missing term-structure binary at ${TERM_STRUCTURE_BIN}." >&2
  echo "Build it first from the theta repository root:" >&2
  echo "  cargo build --release --bin term-structure" >&2
  exit 1
fi

exec "${TERM_STRUCTURE_BIN}" "$@"
