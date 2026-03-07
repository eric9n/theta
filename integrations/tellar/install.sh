#!/usr/bin/env bash
set -euo pipefail

THETA_REPO="${THETA_REPO:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}"
GUILD_DIR="${GUILD_DIR:-$HOME/.tellar/guild}"
SKILLS_DIR="${GUILD_DIR}/skills"
SKILL_COMPILE_TIMEOUT="${SKILL_COMPILE_TIMEOUT:-180}"

SKILL_NAMES=(
  "theta"
  "theta-snapshot"
  "theta-portfolio"
  "theta-signals"
  "theta-structure"
  "theta-ops"
)

LEGACY_SKILL_NAMES=(
  "account-monitor"
  "capture-signals"
  "iv-rank"
  "market-extreme"
  "market-tone"
  "portfolio"
  "put-call-bias"
  "relative-extreme"
  "signal-history"
  "skew"
  "smile"
  "snapshot"
  "term-structure"
)

if ! command -v tellarctl >/dev/null 2>&1; then
  echo "tellarctl is required but not found in PATH." >&2
  exit 1
fi

SKILL_SOURCE_ROOT="${THETA_REPO}/integrations/tellar/skills"

if [[ ! -d "${SKILL_SOURCE_ROOT}" ]]; then
  echo "Missing Tellar skill directory: ${SKILL_SOURCE_ROOT}" >&2
  exit 1
fi

mkdir -p "${SKILLS_DIR}"

echo "Theta repo : ${THETA_REPO}"
echo "Guild dir  : ${GUILD_DIR}"
echo
echo "Removing legacy theta skills..."
for skill in "${LEGACY_SKILL_NAMES[@]}"; do
  rm -rf "${SKILLS_DIR}/${skill}"
done

echo "Removing grouped theta skills..."
for skill in "${SKILL_NAMES[@]}"; do
  rm -rf "${SKILLS_DIR}/${skill}"
done

echo "Installing grouped theta skills..."
failures=0
for skill in "${SKILL_NAMES[@]}"; do
  skill_source_dir="${SKILL_SOURCE_ROOT}/${skill}"
  skill_target_dir="${SKILLS_DIR}/${skill}"
  compile_dir="$(mktemp -d)"

  cp -R "${skill_source_dir}/." "${compile_dir}/"

  if command -v timeout >/dev/null 2>&1; then
    if ! timeout "${SKILL_COMPILE_TIMEOUT}" tellarctl install-skill "${compile_dir}" --force; then
      echo "Failed to compile ${skill} within ${SKILL_COMPILE_TIMEOUT}s." >&2
      rm -rf "${compile_dir}"
      failures=$((failures + 1))
      continue
    fi
  else
    if ! tellarctl install-skill "${compile_dir}" --force; then
      echo "Failed to compile ${skill}." >&2
      rm -rf "${compile_dir}"
      failures=$((failures + 1))
      continue
    fi
  fi

  rm -rf "${skill_target_dir}"
  mkdir -p "${skill_target_dir}"
  cp -R "${compile_dir}/." "${skill_target_dir}/"
  rm -rf "${compile_dir}"
done

echo
echo "Installed skills:"
find "${SKILLS_DIR}" -maxdepth 1 -type d \( -name 'theta' -o -name 'theta-*' \) | sort

if [[ "${failures}" -gt 0 ]]; then
  echo
  echo "${failures} skill(s) failed to compile and were not synced to the guild." >&2
  exit 1
fi
