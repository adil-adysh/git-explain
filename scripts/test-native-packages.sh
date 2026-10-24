#!/usr/bin/env bash
set -euo pipefail

# Smoke test native Linux packages (.deb and .rpm) in disposable containers.
# Verifies:
# - Local package installation with dependency resolution (Git)
# - git explain -h and git-explain -h CLI discovery
# - Payload binary SHA256 match against reference raw musl binary
# - Executable mode 755 and file locations
# - Documentation and license presence
# - Absence of maintainer scriptlets and daemon services
# - Safe reinstallation
# - Clean removal (binary removed, user config preserved)
#
# Usage:
#   scripts/test-native-packages.sh [DIST_DIR] [TAG]

DIST_DIR="${1:-dist}"
TAG="${2:-}"

if [ -z "${TAG}" ]; then
  echo "Error: Tag argument is required. Usage: $0 [DIST_DIR] TAG" >&2
  exit 1
fi

# 1. Validate tag format via packaging validator
python3 scripts/verify-release.py validate-tag --tag "${TAG}"

# 2. Only plain stable releases (vX.Y.Z) have native packages; skip variants
if ! [[ "${TAG}" =~ ^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]]; then
  echo "Notice: Tag '${TAG}' is a prerelease or build variant. Skipping native package container tests."
  exit 0
fi

VERSION="${TAG#v}"
DIST_PATH="$(cd "${DIST_DIR}" && pwd)"

DEB_NAME="git-explain_${VERSION}-1_amd64.deb"
RPM_NAME="git-explain-${VERSION}-1.x86_64.rpm"
REF_RAW_NAME="git-explain-${TAG}-linux-x86_64-musl"

DEB_FILE="${DIST_PATH}/${DEB_NAME}"
RPM_FILE="${DIST_PATH}/${RPM_NAME}"
REF_RAW_FILE="${DIST_PATH}/${REF_RAW_NAME}"

for f in "${DEB_FILE}" "${RPM_FILE}" "${REF_RAW_FILE}"; do
  if [ ! -f "${f}" ]; then
    echo "Error: Required test asset not found: ${f}" >&2
    exit 1
  fi
done

REF_SHA="$(sha256sum "${REF_RAW_FILE}" | awk '{print $1}')"
echo "Reference raw musl binary SHA256: ${REF_SHA}"

UBUNTU_IMAGE="${UBUNTU_IMAGE:-ubuntu:24.04}"
FEDORA_IMAGE="${FEDORA_IMAGE:-fedora:44}"

echo "=== Testing Debian package on ${UBUNTU_IMAGE} ==="
docker run --rm \
  -v "${DIST_PATH}:/dist:ro" \
  -e DEBIAN_FRONTEND=noninteractive \
  "${UBUNTU_IMAGE}" \
  bash -euo pipefail -c "
    echo '--> Adding package-specific path-include for /usr/share/doc/git-explain'
    mkdir -p /etc/dpkg/dpkg.cfg.d
    printf 'path-include=/usr/share/doc/git-explain\npath-include=/usr/share/doc/git-explain/*\n' > /etc/dpkg/dpkg.cfg.d/git-explain-doc

    echo '--> Inspecting deb package control fields and absence of scriptlets'
    mkdir -p /tmp/git-explain-control
    dpkg-deb --control '/dist/${DEB_NAME}' /tmp/git-explain-control
    test -f /tmp/git-explain-control/control
    grep -E '^Depends:.*git' /tmp/git-explain-control/control || (echo 'Error: git dependency missing in deb control' >&2; exit 1)
    for forbidden in preinst postinst prerm postrm; do
      if [ -e \"/tmp/git-explain-control/\${forbidden}\" ]; then
        echo \"Error: Forbidden scriptlet '\${forbidden}' found in deb control archive\" >&2
        exit 1
      fi
    done

    echo '--> Installing Debian package with apt (resolving git dependency)'
    apt-get update -qq
    apt-get install -y --no-install-recommends '/dist/${DEB_NAME}'

    echo '--> Verifying binary location, permissions, and SHA256'
    test -f /usr/bin/git-explain
    test -x /usr/bin/git-explain
    test \"\$(stat -c %a /usr/bin/git-explain)\" = '755'
    INSTALLED_SHA=\"\$(sha256sum /usr/bin/git-explain | awk '{print \$1}')\"
    if [ \"\${INSTALLED_SHA}\" != '${REF_SHA}' ]; then
      echo \"Error: Installed binary SHA256 (\${INSTALLED_SHA}) does not match reference (${REF_SHA})\" >&2
      exit 1
    fi

    echo '--> Verifying documentation and license presence'
    test -f /usr/share/doc/git-explain/README.md
    test -f /usr/share/doc/git-explain/LICENSE
    test -f /usr/share/doc/git-explain/releases.md

    echo '--> Verifying no service files installed'
    test ! -e /etc/systemd/system/git-explain.service
    test ! -e /lib/systemd/system/git-explain.service

    echo '--> Testing CLI discovery: git explain -h and git-explain -h'
    git explain -h >/dev/null
    git-explain -h >/dev/null

    echo '--> Testing user config preservation across reinstall and purge removal'
    mkdir -p ~/.config/git-explain
    echo 'user-setting=1' > ~/.config/git-explain/config.toml

    apt-get install -y --reinstall '/dist/${DEB_NAME}'
    test -x /usr/bin/git-explain
    test -f ~/.config/git-explain/config.toml
    test \"\$(cat ~/.config/git-explain/config.toml)\" = 'user-setting=1'

    apt-get purge -y git-explain
    test ! -e /usr/bin/git-explain
    test -f ~/.config/git-explain/config.toml
    test \"\$(cat ~/.config/git-explain/config.toml)\" = 'user-setting=1'
    echo 'Debian package test passed successfully.'
"

echo "=== Testing RPM package on ${FEDORA_IMAGE} ==="
docker run --rm \
  -v "${DIST_PATH}:/dist:ro" \
  "${FEDORA_IMAGE}" \
  bash -euo pipefail -c "
    echo '--> Inspecting RPM package metadata and absence of scriptlets'
    rpm -qp --requires '/dist/${RPM_NAME}' > /tmp/rpm-reqs.txt
    grep -E '^git' /tmp/rpm-reqs.txt || (echo 'Error: git dependency missing in RPM metadata' >&2; exit 1)
    SCRIPTS=\"\$(rpm -qp --scripts '/dist/${RPM_NAME}')\"
    if [ -n \"\${SCRIPTS}\" ]; then
      echo \"Error: Forbidden scriptlets found in RPM: \${SCRIPTS}\" >&2
      exit 1
    fi

    echo '--> Installing RPM package with dnf (scoped localpkg_gpgcheck=0 and reset tsflags for docs)'
    dnf install -y --setopt=localpkg_gpgcheck=0 --setopt=tsflags= '/dist/${RPM_NAME}'

    echo '--> Verifying binary location, permissions, and SHA256'
    test -f /usr/bin/git-explain
    test -x /usr/bin/git-explain
    test \"\$(stat -c %a /usr/bin/git-explain)\" = '755'
    INSTALLED_SHA=\"\$(sha256sum /usr/bin/git-explain | awk '{print \$1}')\"
    if [ \"\${INSTALLED_SHA}\" != '${REF_SHA}' ]; then
      echo \"Error: Installed binary SHA256 (\${INSTALLED_SHA}) does not match reference (${REF_SHA})\" >&2
      exit 1
    fi

    echo '--> Verifying documentation and license presence'
    test -f /usr/share/doc/git-explain/README.md
    test -f /usr/share/licenses/git-explain/LICENSE
    test -f /usr/share/doc/git-explain/releases.md

    echo '--> Verifying no service files installed'
    test ! -e /etc/systemd/system/git-explain.service
    test ! -e /usr/lib/systemd/system/git-explain.service

    echo '--> Testing CLI discovery: git explain -h and git-explain -h'
    git explain -h >/dev/null
    git-explain -h >/dev/null

    echo '--> Testing user config preservation across reinstall and removal'
    mkdir -p ~/.config/git-explain
    echo 'user-setting=1' > ~/.config/git-explain/config.toml

    dnf reinstall -y --setopt=localpkg_gpgcheck=0 --setopt=tsflags= '/dist/${RPM_NAME}'
    test -x /usr/bin/git-explain
    test -f ~/.config/git-explain/config.toml
    test \"\$(cat ~/.config/git-explain/config.toml)\" = 'user-setting=1'

    dnf remove -y git-explain
    test ! -e /usr/bin/git-explain
    test -f ~/.config/git-explain/config.toml
    test \"\$(cat ~/.config/git-explain/config.toml)\" = 'user-setting=1'
    echo 'RPM package test passed successfully.'
"

echo "All native package container tests passed successfully."
