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

# Automatically update the systemd service file if it targets capture-signals
if [[ "${SERVICE_NAME}" == capture-signals* ]]; then
    echo "Updating systemd service file for ${SERVICE_NAME}..."
    sudo cp deploy/capture-signals.service "/etc/systemd/system/${SERVICE_NAME}.service"
    sudo systemctl daemon-reload
fi

# Automatically update the systemd service file if it targets account-monitor
if [[ "${SERVICE_NAME}" == account-monitor* ]]; then
    echo "Updating systemd service file for ${SERVICE_NAME}..."
    sudo cp deploy/account-monitor.service "/etc/systemd/system/${SERVICE_NAME}.service"
    sudo systemctl daemon-reload
fi

sudo systemctl restart "${SERVICE_NAME}"
sudo systemctl status "${SERVICE_NAME}" --no-pager --lines=20
