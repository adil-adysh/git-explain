# Native Packages and Scoop Distribution Implementation Plan

> **For agentic workers:** Execute focused implementation and verification with agy; primary owns review, commits, pushes, and publication.

**Goal:** Publish the next stable git-explain release with Debian/RPM downloads and a maintained Scoop entry in adil-adysh/scoop-bucket.

**Architecture:** Reuse the existing x86_64 musl release binary for native Linux packages. Both install /usr/bin/git-explain and declare Git, preserving `git explain` discovery. Keep the existing archive downloads. Scoop uses the verified Windows ZIP and SHA256 manifest, with bucket-owned automation so no cross-repository token is needed.

**Tech Stack:** cargo-deb 3.8.0, cargo-generate-rpm 0.21.0, Python stdlib, GitHub Actions, Scoop JSON, Debian/Ubuntu and Fedora container smoke tests.

**Authorization:** The user explicitly approved the previously discussed package approach, requested implementation, reuse of an existing bucket where suitable, and the next release. Proceed without another design approval.

## Constraints and choices
- Existing bucket is https://github.com/adil-adysh/scoop-bucket; preserve bucket/hn.json.
- Scope: x86_64, matching the current binary release matrix. No APT/DNF repository hosting.
- Native installer packages and Scoop track plain stable vX.Y.Z releases; prerelease/build variants keep archive downloads.
- Include README, Apache-2.0 license, release guide; UI, parsers, and SQLite remain embedded.
- Do not start a daemon, change user profiles, bundle model weights, or add installation scriptlets.
- Native package tests run in disposable containers; verify Git subcommand discovery, payload, dependency, reinstall and removal.
- Extend exact expected asset validation and SHA256 coverage to the native packages before publication.
- Primary chooses v0.2.2 unless the repository changes require otherwise; never move published tags.

## Task 1: Native packages
- [ ] Add focused failing tests for package metadata and stable expected assets.
- [ ] Add Cargo package/license metadata and Debian/RPM asset/dependency declarations.
- [ ] Add package-generation and disposable-container validation scripts, with pinned tooling and exact input binary.
- [ ] Integrate stable package generation into release build/verification and include install tests in CI.
- [ ] Update release documentation and install commands.
- [ ] Run focused tests, workflow lint, real package/container tests, then full task check; review and commit.

## Task 2: Existing Scoop bucket
- [ ] Add failing fixture tests for manifest generation, stable-only version selection, URL/hash validation, and no downgrade.
- [ ] Add bucket/git-explain.json with 64-bit ZIP, executable shim, Git dependency, checkver/autoupdate.
- [ ] Add a narrow updater and bucket-owned schedule/manual workflow that only commits git-explain.json; preserve hn.json.
- [ ] Update bucket README and source release docs with installation/update instructions.
- [ ] Validate fixtures, actual upstream checksums, manifest and isolated Scoop installation; review and commit.

## Task 3: Release and distribution verification
- [ ] Bump Cargo.toml/Cargo.lock to v0.2.2 and push focused commits.
- [ ] Require green source CI (including package installation checks) before tagging.
- [ ] Publish annotated tag and await all release jobs and complete asset verification.
- [ ] Download native packages, verify checksums and inspect/test package-manager installation and `git explain`.
- [ ] Trigger bucket update, verify manifest version/hash, and test Scoop installation.
- [ ] Report release URL, bucket install commands, completed checks, and any material limits.
