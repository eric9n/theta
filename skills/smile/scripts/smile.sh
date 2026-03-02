#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
THETA_ROOT="$(cd "${SCRIPT_DIR}/../../.." && pwd)"
SMILE_BIN="${THETA_ROOT}/target/release/smile"

if [[ ! -x "${SMILE_BIN}" ]]; then
  echo "Missing smile binary at ${SMILE_BIN}." >&2
  echo "Build it first from the theta repository root:" >&2
  echo "  cargo build --release --bin smile" >&2
  exit 1
fi

exec "${SMILE_BIN}" "$@"
