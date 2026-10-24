#!/usr/bin/env bash
set -euo pipefail

CONFIGURATION="release"
FORCE="false"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --configuration|-c)
      if [[ $# -lt 2 ]]; then
        echo "Error: --configuration requires an argument (debug or release)." >&2
        exit 1
      fi
      CONFIGURATION="$2"
      shift 2
      ;;
    --force|-f)
      if [[ $# -ge 2 && ( "$2" == "true" || "$2" == "false" ) ]]; then
        FORCE="$2"
        shift 2
      else
        FORCE="true"
        shift 1
      fi
      ;;
    *)
      echo "Unknown option: $1" >&2
      exit 1
      ;;
  esac
done

if [[ "${CONFIGURATION}" != "release" && "${CONFIGURATION}" != "debug" ]]; then
  echo "Error: Invalid configuration '${CONFIGURATION}'. Must be 'debug' or 'release'." >&2
  exit 1
fi

if [[ "${FORCE}" != "true" && "${FORCE}" != "false" ]]; then
  echo "Error: Invalid force value '${FORCE}'. Must be 'true' or 'false'." >&2
  exit 1
fi

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET_ROOT="${REPO_ROOT}/target"
DESTINATION_DIR="${HOME}/.local/bin"
DESTINATION="${DESTINATION_DIR}/git-explain"

# Binary selection: exact host binary only, reject cross-target ambiguity
SOURCE=""
HOST_TRIPLE=""
if command -v rustc >/dev/null 2>&1; then
  HOST_TRIPLE="$(rustc -vV 2>/dev/null | grep '^host:' | cut -d' ' -f2 || true)"
fi

if [[ -f "${TARGET_ROOT}/${CONFIGURATION}/git-explain" ]]; then
  SOURCE="${TARGET_ROOT}/${CONFIGURATION}/git-explain"
elif [[ -n "${HOST_TRIPLE}" && -f "${TARGET_ROOT}/${HOST_TRIPLE}/${CONFIGURATION}/git-explain" ]]; then
  SOURCE="${TARGET_ROOT}/${HOST_TRIPLE}/${CONFIGURATION}/git-explain"
fi

if [[ -z "${SOURCE}" ]]; then
  echo "Error: Built git-explain binary was not found for configuration '${CONFIGURATION}'. Run the matching build task first." >&2
  exit 1
fi

mkdir -p "${DESTINATION_DIR}"

# Safe daemon status inspection (exact CLI contract: "Daemon: running" vs "not running")
RESTART_DAEMON=0
if [[ -x "${DESTINATION}" ]]; then
  STATUS_OUT="$("${DESTINATION}" daemon status 2>/dev/null || true)"
  if [[ "${STATUS_OUT}" == *"Daemon: running"* ]]; then
    RESTART_DAEMON=1
    echo "Stopping running git-explain daemon before installation..."
    "${DESTINATION}" daemon stop >/dev/null 2>&1 || true

    DEADLINE=$((SECONDS + 5))
    while [[ $SECONDS -lt $DEADLINE ]]; do
      STATUS_CHECK="$("${DESTINATION}" daemon status 2>/dev/null || true)"
      if [[ "${STATUS_CHECK}" != *"Daemon: running"* ]]; then
        break
      fi
      sleep 0.2
    done

    FINAL_STATUS="$("${DESTINATION}" daemon status 2>/dev/null || true)"
    if [[ "${FINAL_STATUS}" == *"Daemon: running"* ]]; then
      if [[ "${FORCE}" != "true" ]]; then
        echo "Error: git-explain daemon is still running. Retry with: task install FORCE=true" >&2
        exit 1
      fi

      DAEMON_PID="$(echo "${FINAL_STATUS}" | grep '^PID:' | awk '{print $2}' || true)"
      # A status PID alone is insufficient: it may have been reused. Only stop
      # a process owned by this user that is executing this installed binary.
      if [[ "${DAEMON_PID}" =~ ^[0-9]+$ &&
            -d "/proc/${DAEMON_PID}" &&
            "$(stat -c %u "/proc/${DAEMON_PID}" 2>/dev/null)" == "$(id -u)" &&
            "$(readlink "/proc/${DAEMON_PID}/exe" 2>/dev/null)" == "${DESTINATION}" ]]; then
        echo "Force-stopping the busy git-explain daemon (PID ${DAEMON_PID})..."
        kill -9 "${DAEMON_PID}" 2>/dev/null || true
        sleep 0.3
      else
        echo "Error: Cannot verify the daemon executable and owner. Stop it manually before retrying." >&2
        exit 1
      fi
    fi
  fi
fi

# Atomic installation using temporary file with cleanup trap
TMP_DEST="${DESTINATION}.tmp.$$"
trap 'rm -f "${TMP_DEST:-}"' EXIT INT TERM

cp -f "${SOURCE}" "${TMP_DEST}"
chmod +x "${TMP_DEST}"

# Verify hash of temp copy matches source before atomic replace
get_hash() {
  local file="$1"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "${file}" | cut -d' ' -f1
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "${file}" | cut -d' ' -f1
  else
    echo ""
  fi
}

SOURCE_HASH="$(get_hash "${SOURCE}")"
TMP_HASH="$(get_hash "${TMP_DEST}")"

if [[ -n "${SOURCE_HASH}" && -n "${TMP_HASH}" && "${SOURCE_HASH}" != "${TMP_HASH}" ]]; then
  echo "Error: Staged binary hash does not match source binary." >&2
  exit 1
fi

mv -f "${TMP_DEST}" "${DESTINATION}"
INSTALLED_HASH="$(get_hash "${DESTINATION}")"

echo "Installed ${SOURCE}"
echo "        to ${DESTINATION}"
if [[ -n "${INSTALLED_HASH}" ]]; then
  echo "SHA256: ${INSTALLED_HASH}"
fi

# Only restart if daemon was known to be running previously
if [[ ${RESTART_DAEMON} -eq 1 ]]; then
  echo "Restarting the git-explain daemon..."
  nohup "${DESTINATION}" daemon start >/dev/null 2>&1 &
  echo "Daemon restart requested."
fi
