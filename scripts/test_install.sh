#!/usr/bin/env bash
set -euo pipefail

# Confined temporary fixture directory
TMP_DIR="$(mktemp -d /tmp/git-explain-install-test.XXXXXX)"
trap 'rm -rf "${TMP_DIR}"' EXIT INT TERM

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
INSTALL_SRC="${REPO_ROOT}/scripts/install.sh"

echo "=== Running Linux installer fixture regression tests ==="

# Set up isolated HOME and fake repository structure
FAKE_HOME="${TMP_DIR}/fake_home"
mkdir -p "${FAKE_HOME}"
export HOME="${FAKE_HOME}"

FAKE_REPO="${TMP_DIR}/fake_repo"
mkdir -p "${FAKE_REPO}/scripts"
cp "${INSTALL_SRC}" "${FAKE_REPO}/scripts/install.sh"
chmod +x "${FAKE_REPO}/scripts/install.sh"

# Mock rustc to provide a deterministic host triple
MOCK_BIN="${TMP_DIR}/bin"
mkdir -p "${MOCK_BIN}"
cat << 'EOF' > "${MOCK_BIN}/rustc"
#!/usr/bin/env bash
if [[ "$*" == *"-vV"* ]]; then
  echo "host: x86_64-unknown-linux-gnu"
  exit 0
fi
exit 1
EOF
chmod +x "${MOCK_BIN}/rustc"
export PATH="${MOCK_BIN}:${PATH}"

# 1. Argument validation: missing or invalid options
echo "Test 1: Validating argument rejection..."
if bash "${FAKE_REPO}/scripts/install.sh" --configuration invalid >/dev/null 2>&1; then
  echo "FAIL: Expected error on invalid configuration" >&2
  exit 1
fi

if bash "${FAKE_REPO}/scripts/install.sh" --force maybe >/dev/null 2>&1; then
  echo "FAIL: Expected error on invalid force flag" >&2
  exit 1
fi

# 2. Missing binary rejection (no stale fallback)
echo "Test 2: Validating missing binary rejection..."
if bash "${FAKE_REPO}/scripts/install.sh" --configuration release >/dev/null 2>&1; then
  echo "FAIL: Expected error when target binary does not exist" >&2
  exit 1
fi

# 3. Successful atomic installation to isolated HOME
echo "Test 3: Testing basic atomic installation..."
mkdir -p "${FAKE_REPO}/target/release"
cat << 'EOF' > "${FAKE_REPO}/target/release/git-explain"
#!/usr/bin/env bash
if [[ "${1:-}" == "daemon" && "${2:-}" == "status" ]]; then
  echo "not running"
  exit 0
fi
echo "git-explain mock v0.1.0"
EOF
chmod +x "${FAKE_REPO}/target/release/git-explain"

INSTALL_OUT="$(bash "${FAKE_REPO}/scripts/install.sh" --configuration release)"
INSTALLED_BIN="${FAKE_HOME}/.local/bin/git-explain"

if [[ ! -x "${INSTALLED_BIN}" ]]; then
  echo "FAIL: Installed binary does not exist or is not executable" >&2
  exit 1
fi

if [[ "$("${INSTALLED_BIN}")" != *"git-explain mock v0.1.0"* ]]; then
  echo "FAIL: Installed binary execution output mismatch" >&2
  exit 1
fi

# Verify no leftover temporary files in destination directory
TMP_LEFTOVERS=( "${FAKE_HOME}/.local/bin"/*.tmp.* )
if [[ -e "${TMP_LEFTOVERS[0]:-}" ]]; then
  echo "FAIL: Found uncleaned temporary file in destination directory" >&2
  exit 1
fi

# 4. Confirm no daemon restart when daemon was not running
echo "Test 4: Verifying no daemon restart when inactive..."
if [[ "${INSTALL_OUT}" == *"Restarting the git-explain daemon"* ]]; then
  echo "FAIL: Installer attempted daemon restart when daemon was not running" >&2
  exit 1
fi

# 5. Exact host selection and rejection of cross-target ambiguity
echo "Test 5: Testing rejection of cross-target ambiguity..."
rm -f "${FAKE_REPO}/target/release/git-explain"
mkdir -p "${FAKE_REPO}/target/aarch64-unknown-linux-gnu/release"
cp "${INSTALLED_BIN}" "${FAKE_REPO}/target/aarch64-unknown-linux-gnu/release/git-explain"

if bash "${FAKE_REPO}/scripts/install.sh" --configuration release >/dev/null 2>&1; then
  echo "FAIL: Installer picked cross-target binary unexpectedly" >&2
  exit 1
fi

# But when matching host triple is present, it succeeds
mkdir -p "${FAKE_REPO}/target/x86_64-unknown-linux-gnu/release"
cp "${INSTALLED_BIN}" "${FAKE_REPO}/target/x86_64-unknown-linux-gnu/release/git-explain"
if ! bash "${FAKE_REPO}/scripts/install.sh" --configuration release >/dev/null 2>&1; then
  echo "FAIL: Installer failed to pick matching host triple binary" >&2
  exit 1
fi

# 6. Unrelated processes are never killed
echo "Test 6: Verifying unrelated processes are safe..."
sleep 100 &
UNRELATED_PID=$!

# Mock an existing installed binary whose status claims to be running PID $UNRELATED_PID
cat << EOF > "${INSTALLED_BIN}"
#!/usr/bin/env bash
if [[ "\${1:-}" == "daemon" && "\${2:-}" == "status" ]]; then
  echo -e "Daemon: running\nPID: ${UNRELATED_PID}"
  exit 0
fi
EOF
chmod +x "${INSTALLED_BIN}"

# Running installer with --force true must refuse to kill the unrelated process because /proc/PID/exe does not match
if bash "${FAKE_REPO}/scripts/install.sh" --configuration release --force true >/dev/null 2>&1; then
  echo "FAIL: Installer should have failed to verify unrelated daemon process" >&2
  kill -9 "${UNRELATED_PID}" 2>/dev/null || true
  exit 1
fi

# Verify unrelated process is still running
if ! kill -0 "${UNRELATED_PID}" 2>/dev/null; then
  echo "FAIL: Unrelated process was killed!" >&2
  exit 1
fi
kill -9 "${UNRELATED_PID}" 2>/dev/null || true

echo "=== All Linux installer tests passed successfully ==="
