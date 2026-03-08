#!/usr/bin/env bash

set -euo pipefail

SCRIPT_PATH="${BASH_SOURCE[0]-}"
SCRIPT_DIR=""
ROOT_DIR=""
if [[ -n "${SCRIPT_PATH}" ]]; then
  SCRIPT_DIR="$(cd "$(dirname "${SCRIPT_PATH}")" && pwd)"
  ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
fi
REPO="${THETA_REPO:-eric9n/theta}"
VERSION="${THETA_VERSION:-latest}"
PREFIX="${PREFIX:-/usr/local/bin}"
SHARE_DIR="${SHARE_DIR:-/usr/local/share/theta}"
SYSTEMD_DIR="${SYSTEMD_DIR:-/etc/systemd/system}"
INSTALL_SYSTEMD="${INSTALL_SYSTEMD:-1}"
INSTALL_SKILLS="${INSTALL_SKILLS:-1}"
REMOVE_LEGACY_ROOT="${REMOVE_LEGACY_ROOT:-0}"
FORCE_INSTALL="${THETA_FORCE_INSTALL:-0}"
INTERNAL_BUNDLE_DIR=""
RESOLVED_VERSION=""
TMP_DIR=""

cleanup() {
  if [[ -n "${TMP_DIR}" && -d "${TMP_DIR}" ]]; then
    rm -rf "${TMP_DIR}"
  fi
}
trap cleanup EXIT

usage() {
  cat <<'EOF'
Usage: install.sh [--version <tag>|latest]

Environment:
  THETA_REPO         GitHub repo in owner/name form (default: eric9n/theta)
  THETA_VERSION      Release tag or "latest" (default: latest)
  PREFIX             Binary install prefix (default: /usr/local/bin)
  SHARE_DIR          Shared data dir for skills (default: /usr/local/share/theta)
  SYSTEMD_DIR        systemd unit dir (default: /etc/systemd/system)
  INSTALL_SYSTEMD    Install systemd templates when 1 (default: 1)
  INSTALL_SKILLS     Install skills when 1 (default: 1)
  THETA_FORCE_INSTALL
                     Reinstall even if target version is already present when set to 1
  REMOVE_LEGACY_ROOT Remove /root/theta after a successful install when set to 1
EOF
}

resolve_latest_version() {
  curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
    | sed -n 's/.*"tag_name":[[:space:]]*"\([^"]*\)".*/\1/p' \
    | head -n 1
}

resolve_archive_name() {
  case "$(uname -m)" in
    x86_64|amd64)
      echo "theta-linux-x86_64"
      ;;
    *)
      echo "Unsupported architecture: $(uname -m)" >&2
      return 1
      ;;
  esac
}

installed_version() {
  if [[ -f "${SHARE_DIR}/VERSION" ]]; then
    cat "${SHARE_DIR}/VERSION"
  fi
}

install_bundle() {
  local bundle_dir="$1"
  local version="$2"
  local current_version=""

  if [[ -z "${version}" || "${version}" == "latest" ]]; then
    if [[ -f "${bundle_dir}/VERSION" ]]; then
      version="$(cat "${bundle_dir}/VERSION")"
    else
      version=""
    fi
  fi

  current_version="$(installed_version || true)"
  if [[ -n "${version}" && "${FORCE_INSTALL}" != "1" && "${current_version}" == "${version}" && -x "${PREFIX}/theta" && -x "${PREFIX}/theta-daemon" && -x "${PREFIX}/theta-mcp" ]]; then
    echo "theta ${version} is already installed in ${PREFIX}"
    return 0
  fi

  if [[ -n "${current_version}" && -n "${version}" && "${current_version}" != "${version}" ]]; then
    echo "Replacing installed theta ${current_version} with ${version}"
  fi

  install -d "${PREFIX}"
  install -m 0755 "${bundle_dir}/bin/theta" "${PREFIX}/theta"
  install -m 0755 "${bundle_dir}/bin/theta-daemon" "${PREFIX}/theta-daemon"
  install -m 0755 "${bundle_dir}/bin/theta-mcp" "${PREFIX}/theta-mcp"

  echo "Installed theta binaries to ${PREFIX}:"
  echo "  ${PREFIX}/theta"
  echo "  ${PREFIX}/theta-daemon"
  echo "  ${PREFIX}/theta-mcp"

  install -d "${SHARE_DIR}"
  if [[ "${INSTALL_SKILLS}" != "0" ]] && [[ -d "${bundle_dir}/skills" ]]; then
    rm -rf "${SHARE_DIR}/skills"
    cp -R "${bundle_dir}/skills" "${SHARE_DIR}/skills"
    echo "Installed theta skills to ${SHARE_DIR}/skills"
  fi

  if [[ -n "${version}" ]]; then
    printf '%s\n' "${version}" > "${SHARE_DIR}/VERSION"
    echo "Installed theta version ${version}"
  fi

  if [[ "${INSTALL_SYSTEMD}" != "0" ]]; then
    install -d "${SYSTEMD_DIR}"
    install -m 0644 "${bundle_dir}/deploy/theta-daemon.service" "${SYSTEMD_DIR}/theta-daemon@.service"
    install -m 0644 "${bundle_dir}/deploy/capture-signals.service" "${SYSTEMD_DIR}/capture-signals@.service"
    install -m 0644 "${bundle_dir}/deploy/account-monitor.service" "${SYSTEMD_DIR}/account-monitor@.service"
    if command -v systemctl >/dev/null 2>&1; then
      systemctl daemon-reload
    fi
    echo "Installed systemd templates to ${SYSTEMD_DIR}"
  fi

  if [[ "${REMOVE_LEGACY_ROOT}" == "1" ]] && [[ -d /root/theta ]]; then
    rm -rf /root/theta
    echo "Removed legacy source checkout: /root/theta"
  fi
}

download_and_install_release() {
  local archive_name archive_url checksum_url

  if ! command -v curl >/dev/null 2>&1; then
    echo "curl is required" >&2
    exit 1
  fi

  if ! command -v tar >/dev/null 2>&1; then
    echo "tar is required" >&2
    exit 1
  fi

  archive_name="$(resolve_archive_name)"
  if [[ "${VERSION}" == "latest" ]]; then
    VERSION="$(resolve_latest_version)"
  fi

  if [[ -z "${VERSION}" ]]; then
    echo "Failed to resolve release version for ${REPO}" >&2
    exit 1
  fi

  archive_url="https://github.com/${REPO}/releases/download/${VERSION}/${archive_name}.tar.gz"
  checksum_url="${archive_url}.sha256"
  TMP_DIR="$(mktemp -d)"

  echo "Downloading ${archive_url}"
  curl -fsSL "${archive_url}" -o "${TMP_DIR}/${archive_name}.tar.gz"
  curl -fsSL "${checksum_url}" -o "${TMP_DIR}/${archive_name}.tar.gz.sha256"

  if command -v sha256sum >/dev/null 2>&1; then
    (cd "${TMP_DIR}" && sha256sum -c "${archive_name}.tar.gz.sha256")
  elif command -v shasum >/dev/null 2>&1; then
    local expected actual
    expected="$(awk '{print $1}' "${TMP_DIR}/${archive_name}.tar.gz.sha256")"
    actual="$(shasum -a 256 "${TMP_DIR}/${archive_name}.tar.gz" | awk '{print $1}')"
    if [[ "${expected}" != "${actual}" ]]; then
      echo "Checksum verification failed" >&2
      exit 1
    fi
  else
    echo "sha256sum or shasum is required" >&2
    exit 1
  fi

  mkdir -p "${TMP_DIR}/bundle"
  tar -xzf "${TMP_DIR}/${archive_name}.tar.gz" -C "${TMP_DIR}/bundle"
  install_bundle "${TMP_DIR}/bundle" "${VERSION}"
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --version)
      VERSION="${2:?missing version value}"
      shift 2
      ;;
    --install-bundle)
      INTERNAL_BUNDLE_DIR="${2:?missing bundle dir}"
      shift 2
      ;;
    --resolved-version)
      RESOLVED_VERSION="${2:?missing version value}"
      shift 2
      ;;
    -h|--help)
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

if [[ -n "${INTERNAL_BUNDLE_DIR}" ]]; then
  install_bundle "${INTERNAL_BUNDLE_DIR}" "${RESOLVED_VERSION}"
elif [[ -n "${ROOT_DIR}" && -f "${ROOT_DIR}/deploy/install.sh" && -x "${ROOT_DIR}/bin/theta" && -x "${ROOT_DIR}/bin/theta-daemon" && -x "${ROOT_DIR}/bin/theta-mcp" ]]; then
  install_bundle "${ROOT_DIR}" "${VERSION}"
else
  download_and_install_release
fi
