#!/usr/bin/env bash
set -euo pipefail

# Optional compatibility wrapper for users who want to run theta over SSH.
# This script is intentionally generic and may be hand-edited for a fixed host,
# user, prefix, or remote binary path if a local environment prefers that.

usage() {
  cat <<'EOF'
Usage:
  remote-theta.sh [--host HOST] [--bin PATH] [--] <theta args...>

Environment:
  THETA_REMOTE_HOST    Default SSH host if --host is omitted
  THETA_REMOTE_USER    Optional SSH username
  THETA_REMOTE_PREFIX  Remote install prefix; resolves theta as ${PREFIX}/theta
  THETA_REMOTE_BIN     Explicit remote theta binary path
  THETA_REMOTE_SSH     SSH executable to use (default: ssh)

Examples:
  remote-theta.sh --host srv1313960 -- --version
  remote-theta.sh --host srv1313960 -- portfolio report --offline
  THETA_REMOTE_HOST=srv1313960 remote-theta.sh -- signals history --limit 20
EOF
}

host="${THETA_REMOTE_HOST:-}"
remote_bin="${THETA_REMOTE_BIN:-}"
remote_prefix="${THETA_REMOTE_PREFIX:-}"
ssh_bin="${THETA_REMOTE_SSH:-ssh}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --host)
      host="${2:?missing host value}"
      shift 2
      ;;
    --bin)
      remote_bin="${2:?missing remote binary path}"
      shift 2
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    --)
      shift
      break
      ;;
    *)
      break
      ;;
  esac
done

if [[ -z "${host}" ]]; then
  echo "Missing remote host. Use --host or THETA_REMOTE_HOST." >&2
  usage >&2
  exit 1
fi

if [[ $# -eq 0 ]]; then
  echo "Missing theta arguments." >&2
  usage >&2
  exit 1
fi

target="${host}"
if [[ -n "${THETA_REMOTE_USER:-}" ]]; then
  target="${THETA_REMOTE_USER}@${host}"
fi

if [[ -z "${remote_bin}" && -n "${remote_prefix}" ]]; then
  remote_bin="${remote_prefix%/}/theta"
fi

if [[ -z "${remote_bin}" ]]; then
  remote_bin="theta"
fi

remote_cmd=("${remote_bin}" "$@")
quoted_remote_cmd="$(printf '%q ' "${remote_cmd[@]}")"

exec "${ssh_bin}" "${target}" -- bash -lc "${quoted_remote_cmd% }"
