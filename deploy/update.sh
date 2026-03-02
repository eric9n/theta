#!/usr/bin/env bash

set -euo pipefail

SERVICE_NAME="${1:-capture-signals@$(whoami)}"
BRANCH="${THETA_BRANCH:-main}"
PROJECT_DIR="${THETA_PROJECT_DIR:-$HOME/theta}"

echo "Updating theta in ${PROJECT_DIR} (branch: ${BRANCH})"

cd "${PROJECT_DIR}"

git fetch origin
git checkout "${BRANCH}"
git pull --ff-only origin "${BRANCH}"

cargo build --release

sudo systemctl restart "${SERVICE_NAME}"
sudo systemctl status "${SERVICE_NAME}" --no-pager --lines=20
