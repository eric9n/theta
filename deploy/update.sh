#!/usr/bin/env bash

set -euo pipefail

SERVICE_NAME="${1:-capture-signals@$(whoami)}"
BRANCH="${THETA_BRANCH:-main}"
PROJECT_DIR="${THETA_PROJECT_DIR:-$HOME/theta}"

render_service_template() {
    local template_path="$1"
    local service_name="$2"
    local rendered_path="$3"
    local unit_user=""
    local unit_home=""

    if [[ "${service_name}" == *"@"* ]]; then
        cp "${template_path}" "${rendered_path}"
        return
    fi

    case "${service_name}" in
        capture-signals-*)
            unit_user="${service_name#capture-signals-}"
            ;;
        account-monitor-*)
            unit_user="${service_name#account-monitor-}"
            ;;
        *)
            echo "Unable to infer unit user from service name: ${service_name}" >&2
            exit 1
            ;;
    esac

    unit_home="$(getent passwd "${unit_user}" | cut -d: -f6)"
    if [[ -z "${unit_home}" ]]; then
        echo "Unable to resolve home directory for ${unit_user}" >&2
        exit 1
    fi

    sed \
        -e "s|User=%i|User=${unit_user}|" \
        -e "s|WorkingDirectory=%h|WorkingDirectory=${unit_home}|" \
        -e "s|Environment=HOME=%h|Environment=HOME=${unit_home}|" \
        -e "s|EnvironmentFile=-%h|EnvironmentFile=-${unit_home}|" \
        -e "s|ExecStart=%h|ExecStart=${unit_home}|" \
        "${template_path}" > "${rendered_path}"
}

echo "Updating theta in ${PROJECT_DIR} (branch: ${BRANCH})"

cd "${PROJECT_DIR}"

git fetch origin
git checkout "${BRANCH}"
git pull --ff-only origin "${BRANCH}"

cargo build --release

# Automatically update the systemd service file if it targets capture-signals
if [[ "${SERVICE_NAME}" == capture-signals* ]]; then
    echo "Updating systemd service file for ${SERVICE_NAME}..."
    tmp_unit="$(mktemp)"
    render_service_template "deploy/capture-signals.service" "${SERVICE_NAME}" "${tmp_unit}"
    sudo cp "${tmp_unit}" "/etc/systemd/system/${SERVICE_NAME}.service"
    rm -f "${tmp_unit}"
    sudo systemctl daemon-reload
fi

# Automatically update the systemd service file if it targets account-monitor
if [[ "${SERVICE_NAME}" == account-monitor* ]]; then
    echo "Updating systemd service file for ${SERVICE_NAME}..."
    tmp_unit="$(mktemp)"
    render_service_template "deploy/account-monitor.service" "${SERVICE_NAME}" "${tmp_unit}"
    sudo cp "${tmp_unit}" "/etc/systemd/system/${SERVICE_NAME}.service"
    rm -f "${tmp_unit}"
    sudo systemctl daemon-reload
fi

sudo systemctl restart "${SERVICE_NAME}"
sudo systemctl status "${SERVICE_NAME}" --no-pager --lines=20
