# Release Artifacts and Distribution

This document defines the release artifacts, platform matrix, checksum verification, runtime dependencies, and maintainer release procedures for `git-explain`.

## Supported Platforms & Artifact Matrix

Each release publishes standalone binaries and archive bundles containing the application binary, `README.md`, `LICENSE`, and `docs/releases.md`. In addition, plain stable releases (`vX.Y.Z`) publish native Debian and RPM packages:

| Target Platform | Package / Archive | Binary / Target Path | Description |
|---|---|---|---|
| Windows x86_64 | `git-explain-<tag>-windows-x86_64.zip` | `git-explain-<tag>-windows-x86_64.exe` | MSVC build with static CRT (`+crt-static`), zero VC++ redistributable dependency |
| Linux x86_64 (musl) | `git-explain-<tag>-linux-x86_64-musl.tar.gz` | `git-explain-<tag>-linux-x86_64-musl` | Statically linked musl binary for Linux x86_64 |
| Linux x86_64 (canonical) | `git-explain-<tag>-linux-x86_64.tar.gz` | `git-explain-<tag>-linux-x86_64` | Canonical alias to static musl build |
| Linux x86_64 (GNU) | `git-explain-<tag>-linux-x86_64-gnu.tar.gz` | `git-explain-<tag>-linux-x86_64-gnu` | Built on Ubuntu 22.04 LTS (requires glibc 2.35+) |
| Debian / Ubuntu x86_64 | `git-explain_<version>-1_amd64.deb` | `/usr/bin/git-explain` | Native Debian package from static musl binary (stable only) |
| Fedora / RHEL x86_64 | `git-explain-<version>-1.x86_64.rpm` | `/usr/bin/git-explain` | Native RPM package from static musl binary (stable only) |

Plain stable releases produce exactly 10 downloadable assets accompanied by `SHA256SUMS.txt` (11 total assets). Prereleases and build variants produce the 8 archive and standalone binary assets accompanied by `SHA256SUMS.txt` (9 total assets).

## Runtime Dependencies & Bundling Scope

`git-explain` is distributed as a completely self-contained single executable:
- **Embedded UI**: The Svelte 5 accessible workspace and legacy UI are embedded directly into the binary at compile time.
- **Embedded Parsers**: Tree-sitter parsers for all supported languages are statically compiled and linked.
- **Embedded Database**: SQLite is compiled from bundled C source (`rusqlite` bundled).
- **No Node.js or Rust toolchain required**: Users do **not** need Node.js, npm, Rust, or web asset files installed.

### External Runtime Prerequisites
The application only depends on:
1. **Operating System**: Windows 10/11 x86_64 or Linux x86_64.
2. **Git**: Standard `git` executable on your `PATH` for repository discovery and diff analysis.
3. **Model Server Endpoint**: A local or remote OpenAI-compatible model server (e.g., local [Ollama](https://ollama.ai) or [llama.cpp](https://github.com/ggerganov/llama.cpp)).
4. **Browser**: Any modern web browser (Edge, Chrome, Firefox, Safari) for visual workspace sessions.

> **Security Note:** `git-explain` binaries **never** bundle model weights, GGUF model files, or API credentials. Model configuration and API keys remain strictly under user control via CLI profiles (`git explain profile add`) or environment variables.

## Verifying Release Integrity

Download `SHA256SUMS.txt` alongside your target package.

### On Linux
```bash
sha256sum --check --ignore-missing SHA256SUMS.txt
```

### On Windows (PowerShell)
```powershell
Get-FileHash -Algorithm SHA256 git-explain-*.zip
Get-Content SHA256SUMS.txt
```
Verify that the output hash matches the hash recorded in `SHA256SUMS.txt`.

## Installation

### Native Linux Packages (Debian and RPM)

Native packages are published for plain stable releases (`vX.Y.Z`).

#### Debian / Ubuntu (.deb)
```bash
sudo apt install ./git-explain_<version>-1_amd64.deb
```
Removal:
```bash
sudo apt remove git-explain
```

#### Fedora / RHEL (.rpm)
```bash
sudo dnf install ./git-explain-<version>-1.x86_64.rpm
```
Removal:
```bash
sudo dnf remove git-explain
```

Packages install `/usr/bin/git-explain` (mode `755`), enabling immediate discovery by Git as `git explain`. Packages declare a runtime dependency on `git` and contain no daemon services, autostart entries, or maintainer scriptlets. User configuration in `~/.config/git-explain/` remains untouched on upgrade and removal. Note that no APT/DNF repository hosting or repository signing is configured; download package files directly. For unsigned local RPM files, `dnf` installs locally without disabling repository GPG checks globally.

### Windows (Scoop)
Install via the community bucket:
```text
scoop bucket add adil-adysh https://github.com/adil-adysh/scoop-bucket
scoop install adil-adysh/git-explain
```

### Installation from Archive Bundles

#### Linux
1. Extract the tarball:
   ```bash
   tar -xzf git-explain-<tag>-linux-x86_64.tar.gz
   ```
2. Move the executable to a directory on your `PATH`:
   ```bash
   mkdir -p ~/.local/bin
   mv git-explain ~/.local/bin/
   chmod +x ~/.local/bin/git-explain
   ```

#### Windows
1. Extract `git-explain-<tag>-windows-x86_64.zip`.
2. Move `git-explain.exe` into a directory on your `PATH` (such as `$HOME\.local\bin` or your user tools directory).

---

## Maintainer Release Procedure

Follow these steps to produce a verified GitHub release:

### 1. Version Bump & Consistency Check
Ensure the working tree is clean and `Cargo.toml` reflects the desired release version:
1. Update `version = "X.Y.Z"` in `Cargo.toml`. For prereleases, include the full prerelease suffix (e.g. `version = "0.2.0-rc.1"` for tag `v0.2.0-rc.1`).
2. Run `cargo check` to update `Cargo.lock` (note: `cargo check --locked` checks without updating).
3. Verify locked consistency, formatting, and the test suite:
   ```bash
   cargo check --locked
   task check
   ```

### 2. Commit and Tag
Create a Git tag matching `v` + the Cargo package version:
```bash
git add Cargo.toml Cargo.lock
git commit -m "chore: bump version to vX.Y.Z"
git tag -a vX.Y.Z -m "Release vX.Y.Z"
```

### 3. Push and Workflow Execution
Push the commit and tag to GitHub:
```bash
git push origin main
git push origin vX.Y.Z
```
Alternatively, trigger the release workflow via manual dispatch specifying the existing tag:
```bash
gh workflow run release.yml -f tag=vX.Y.Z
```

### 4. Automated Release Pipeline
The GitHub Actions release workflow executes the following steps:
1. **Verification**: Checks that the tag exists, matches `vSEMVER`, and aligns exactly with `Cargo.toml`.
2. **Matrix Build**: Compiles Windows static CRT, Linux musl, and Linux GNU binaries with locked dependencies.
3. **Packaging**: Invokes `scripts/package-release.py` on each runner to stage and produce archives and standalone executables.
4. **Draft Creation, Upload & Publication**: The release job gathers all release artifacts (10 assets for stable releases including Debian and RPM packages, or 8 for variants), verifies checksums in `SHA256SUMS.txt`, creates a **Draft** release, uploads all assets (11 total for stable, 9 for variants), validates the uploaded asset count, and publishes the release after uploads succeed.

> **Repository Security Note:** Maintainers are recommended to configure GitHub Tag Protection rules (preventing tag deletion or force-push) and repository release immutability settings. The release workflow verifies that the remote tag commit remains unchanged across build and publish stages, but repository settings must be enabled by repository administrators.
