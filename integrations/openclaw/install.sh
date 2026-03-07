#!/usr/bin/env bash
set -euo pipefail

THETA_REPO="${THETA_REPO:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}"
OPENCLAW_HOME="${OPENCLAW_HOME:-$HOME/.openclaw}"
OPENCLAW_WORKSPACE="${OPENCLAW_WORKSPACE:-workspace-dagow}"
SOURCE_DIR="${THETA_REPO}/integrations/openclaw/skills/theta"
TARGET_DIR="${OPENCLAW_HOME}/${OPENCLAW_WORKSPACE}/skills/theta"

if [[ ! -f "${SOURCE_DIR}/SKILL.md" ]]; then
  echo "Missing OpenClaw skill source: ${SOURCE_DIR}/SKILL.md" >&2
  exit 1
fi

mkdir -p "${TARGET_DIR}"
cp "${SOURCE_DIR}/SKILL.md" "${TARGET_DIR}/SKILL.md"

echo "Installed OpenClaw theta skill:"
echo "  source: ${SOURCE_DIR}/SKILL.md"
echo "  target: ${TARGET_DIR}/SKILL.md"
