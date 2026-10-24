#!/usr/bin/env bash
set -euo pipefail

# Pinned packaging tools for Debian (.deb) and RPM (.rpm) generation on ephemeral CI/release runners.
# Pins: cargo-deb 3.8.0, cargo-generate-rpm 0.21.0
cargo install --locked --version 3.8.0 cargo-deb
cargo install --locked --version 0.21.0 cargo-generate-rpm
