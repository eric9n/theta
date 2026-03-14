#!/usr/bin/env bash

set -euo pipefail

TASKCTL_BIN="${TASKCTL_BIN:-}"
CONFIG_PATH="${CONFIG_PATH:-/etc/taskd/tasks.yaml}"
THETA_BIN="${THETA_BIN:-/usr/local/bin/theta}"
TIMEZONE="${TIMEZONE:-America/New_York}"
ACCOUNT="${ACCOUNT:-firstrade}"

usage() {
  cat <<'EOF'
Usage:
  install-taskd.sh [options]

Options:
  --account ACCOUNT              Account monitor account id. Default: firstrade
  --config PATH                  taskd config path. Default: /etc/taskd/tasks.yaml
  --taskctl PATH                 taskctl binary path
  --theta-bin PATH               theta binary path. Default: /usr/local/bin/theta
  --timezone TZ                  Cron timezone. Default: America/New_York
  --help                         Show this help

This script merges or updates theta taskd jobs in the target tasks.yaml.
It does not manage theta-daemon; keep theta-daemon under systemd.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --account)
      ACCOUNT="${2:?missing account value}"
      shift 2
      ;;
    --config)
      CONFIG_PATH="${2:?missing config path}"
      shift 2
      ;;
    --taskctl)
      TASKCTL_BIN="${2:?missing taskctl path}"
      shift 2
      ;;
    --theta-bin)
      THETA_BIN="${2:?missing theta path}"
      shift 2
      ;;
    --timezone)
      TIMEZONE="${2:?missing timezone}"
      shift 2
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
done

if [[ -z "${TASKCTL_BIN}" ]]; then
  if command -v taskctl >/dev/null 2>&1; then
    TASKCTL_BIN="$(command -v taskctl)"
  elif [[ -x /opt/taskd/taskctl ]]; then
    TASKCTL_BIN="/opt/taskd/taskctl"
  else
    echo "taskctl not found in PATH and /opt/taskd/taskctl is missing" >&2
    exit 1
  fi
fi

if [[ ! -x "${TASKCTL_BIN}" ]]; then
  echo "taskctl is not executable: ${TASKCTL_BIN}" >&2
  exit 1
fi

if [[ ! -x "${THETA_BIN}" ]]; then
  echo "theta binary is not executable: ${THETA_BIN}" >&2
  exit 1
fi

install -d "$(dirname "${CONFIG_PATH}")"
if [[ ! -f "${CONFIG_PATH}" ]]; then
  printf 'version: 1\ntasks: []\n' > "${CONFIG_PATH}"
fi

remove_if_present() {
  local task_id="$1"
  "${TASKCTL_BIN}" --config "${CONFIG_PATH}" remove "${task_id}" >/dev/null 2>&1 || true
}

add_cron_task() {
  "${TASKCTL_BIN}" --config "${CONFIG_PATH}" add-cron "$@"
}

remove_if_present "theta-capture-signals"
remove_if_present "theta-account-monitor"
remove_if_present "theta-healthcheck"

add_cron_task \
  --enabled \
  --timezone "${TIMEZONE}" \
  --timeout-seconds 300 \
  --concurrency-policy forbid \
  --max-running 1 \
  --retry-max-attempts 2 \
  --retry-delay-seconds 15 \
  theta-capture-signals \
  "theta capture signals" \
  "0 */5 9-15 ? * Mon-Fri" \
  "${THETA_BIN}" \
  -- \
  signals capture --symbol TSLA.US --market-hours-only

add_cron_task \
  --enabled \
  --timezone "${TIMEZONE}" \
  --timeout-seconds 300 \
  --concurrency-policy forbid \
  --max-running 1 \
  --retry-max-attempts 2 \
  --retry-delay-seconds 15 \
  theta-account-monitor \
  "theta account monitor" \
  "0 */5 9-15 ? * Mon-Fri" \
  "${THETA_BIN}" \
  -- \
  ops account-monitor --once --account "${ACCOUNT}"

add_cron_task \
  --enabled \
  --timezone "${TIMEZONE}" \
  --timeout-seconds 300 \
  --concurrency-policy forbid \
  --max-running 1 \
  --retry-max-attempts 1 \
  --retry-delay-seconds 30 \
  theta-healthcheck \
  "theta healthcheck" \
  "0 0 18 * * *" \
  "${THETA_BIN}" \
  -- \
  ops health-check

"${TASKCTL_BIN}" --config "${CONFIG_PATH}" validate
"${TASKCTL_BIN}" --config "${CONFIG_PATH}" list

cat <<EOF
theta taskd tasks are configured in ${CONFIG_PATH}
- theta-daemon remains under systemd
- updated tasks: theta-capture-signals, theta-account-monitor, theta-healthcheck
- taskd will reload the watched config automatically
EOF
