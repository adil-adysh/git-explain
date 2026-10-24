#!/usr/bin/env python3
"""Release verification helper for git-explain.

Validates exact expected release artifacts, generates safe SHA256 checksums,
and verifies tag alignment with Cargo.toml package version.
"""

import argparse
import hashlib
import importlib.util
import json
import re
import subprocess
import sys
from pathlib import Path
from typing import List

_spec = importlib.util.spec_from_file_location(
    "package_release", Path(__file__).with_name("package-release.py")
)
_packaging = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_packaging)

_spec_native = importlib.util.spec_from_file_location(
    "package_native", Path(__file__).with_name("package-native.py")
)
_native = importlib.util.module_from_spec(_spec_native)
_spec_native.loader.exec_module(_native)

try:
    import tomllib
except ImportError:
    tomllib = None  # type: ignore


def sha256_file(path: Path) -> str:
    """Calculate SHA256 hexadecimal digest of a file."""
    h = hashlib.sha256()
    with path.open("rb") as f:
        while chunk := f.read(65536):
            h.update(chunk)
    return h.hexdigest()


def get_expected_assets(tag: str) -> List[str]:
    """Return the exact list of expected release assets for a given tag.

    Includes 8 archive/binary assets plus native packages (Debian/RPM)
    for plain stable releases (total 10). Prereleases and build variants
    retain the 8 archive/binary assets.
    """
    _packaging.validate_tag(tag)
    base_assets = [
        f"git-explain-{tag}-windows-x86_64.zip",
        f"git-explain-{tag}-windows-x86_64.exe",
        f"git-explain-{tag}-linux-x86_64-gnu.tar.gz",
        f"git-explain-{tag}-linux-x86_64-gnu",
        f"git-explain-{tag}-linux-x86_64-musl.tar.gz",
        f"git-explain-{tag}-linux-x86_64-musl",
        f"git-explain-{tag}-linux-x86_64.tar.gz",
        f"git-explain-{tag}-linux-x86_64",
    ]
    return base_assets + _native.native_package_names(tag)


def validate_uploaded_assets(names: List[str], tag: str):
    """Require all downloads and the checksum manifest, with no stale assets."""
    expected = set(get_expected_assets(tag)) | {"SHA256SUMS.txt"}
    if set(names) != expected or len(names) != len(expected):
        raise ValueError("Uploaded release assets do not match the expected set")


def validate_remote_tag(repository: str, tag: str, expected_commit: str):
    """Verify that remote tag in repository points to expected_commit.

    Peels annotated tag objects until a commit is reached.
    """
    _packaging.validate_tag(tag)
    if not re.match(r"^[0-9a-fA-F]{40}$", expected_commit.strip()):
        raise ValueError(f"Invalid expected commit SHA '{expected_commit}'")
    if not re.match(r"^[a-zA-Z0-9_.-]+/[a-zA-Z0-9_.-]+$", repository.strip()):
        raise ValueError(f"Invalid repository format '{repository}'")

    repo = repository.strip()
    tag_name = tag.strip()
    expected_sha = expected_commit.strip().lower()

    # Query the ref
    cmd = ["gh", "api", f"repos/{repo}/git/ref/tags/{tag_name}"]
    res = subprocess.run(cmd, capture_output=True, text=True, check=True)
    ref_data = json.loads(res.stdout)
    obj = ref_data.get("object", {})

    current_type = obj.get("type")
    current_sha = obj.get("sha")

    hops = 0
    max_hops = 5
    while current_type == "tag" and hops < max_hops:
        hops += 1
        tag_cmd = ["gh", "api", f"repos/{repo}/git/tags/{current_sha}"]
        tag_res = subprocess.run(tag_cmd, capture_output=True, text=True, check=True)
        tag_data = json.loads(tag_res.stdout)
        inner_obj = tag_data.get("object", {})
        current_type = inner_obj.get("type")
        current_sha = inner_obj.get("sha")

    if current_type != "commit" or not current_sha:
        raise ValueError(
            f"Remote tag '{tag_name}' did not resolve to a commit object (type: {current_type})"
        )

    resolved_commit = current_sha.lower()
    if resolved_commit != expected_sha:
        raise ValueError(
            f"Remote tag commit mismatch: tag '{tag_name}' points to {resolved_commit}, expected {expected_sha}"
        )


def validate_dist_assets(dist_dir: Path, tag: str) -> List[str]:
    """Validate that dist_dir contains exactly the expected release assets.

    Raises ValueError if any expected asset is missing or unexpected files exist.
    """
    expected = set(get_expected_assets(tag))
    present = {p.name for p in dist_dir.iterdir() if p.is_file()}

    # Allow SHA256SUMS.txt if already generated
    non_checksum_present = present - {"SHA256SUMS.txt"}

    missing = expected - non_checksum_present
    unexpected = non_checksum_present - expected

    if missing:
        raise ValueError(f"Missing expected release asset(s) in {dist_dir}: {sorted(missing)}")
    if unexpected:
        raise ValueError(f"Unexpected file(s) found in {dist_dir}: {sorted(unexpected)}")

    return get_expected_assets(tag)


def generate_checksums(dist_dir: Path, tag: str) -> Path:
    """Generate SHA256SUMS.txt for the exact expected release assets."""
    expected = validate_dist_assets(dist_dir, tag)
    checksum_lines = []
    for filename in expected:
        file_path = dist_dir / filename
        digest = sha256_file(file_path)
        checksum_lines.append(f"{digest}  {filename}\n")

    sums_path = dist_dir / "SHA256SUMS.txt"
    sums_path.write_text("".join(checksum_lines), encoding="utf-8")
    return sums_path


def verify_checksums(dist_dir: Path):
    """Verify all checksums listed in SHA256SUMS.txt."""
    sums_path = dist_dir / "SHA256SUMS.txt"
    if not sums_path.exists():
        raise FileNotFoundError(f"Checksum file not found: {sums_path}")

    for line in sums_path.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line:
            continue
        parts = line.split(maxsplit=1)
        if len(parts) != 2:
            raise ValueError(f"Malformed checksum line: '{line}'")
        expected_hash, fname = parts[0], parts[1].lstrip("* ")
        file_path = dist_dir / fname
        if not file_path.exists():
            raise FileNotFoundError(f"Asset '{fname}' listed in SHA256SUMS.txt not found.")
        actual_hash = sha256_file(file_path)
        if actual_hash.lower() != expected_hash.lower():
            raise ValueError(
                f"Checksum mismatch for '{fname}': expected {expected_hash}, got {actual_hash}"
            )


def parse_cargo_version(cargo_toml_path: Path) -> str:
    """Extract package.version from Cargo.toml."""
    content = cargo_toml_path.read_text(encoding="utf-8")
    if tomllib is not None:
        data = tomllib.loads(content)
        return str(data["package"]["version"])

    # Fallback if tomllib is unavailable
    in_package = False
    for line in content.splitlines():
        trimmed = line.strip()
        if trimmed == "[package]":
            in_package = True
            continue
        if in_package:
            if trimmed.startswith("["):
                break
            m = re.match(r'^version\s*=\s*"([^"]+)"', trimmed)
            if m:
                return m.group(1)
    raise ValueError(f"Could not parse package.version from {cargo_toml_path}")


def matches_cargo_version(cargo_toml_path: Path, tag: str) -> bool:
    """Return True if tag equals 'v' + cargo package version (exact full match)."""
    if not tag.startswith("v"):
        return False
    cargo_version = parse_cargo_version(cargo_toml_path)
    return tag[1:] == cargo_version


def main():
    parser = argparse.ArgumentParser(description="git-explain release verification helper.")
    subparsers = parser.add_subparsers(dest="command", required=True)

    p_tag = subparsers.add_parser("validate-tag")
    p_tag.add_argument("--tag", required=True)

    p_uploaded = subparsers.add_parser("validate-uploaded")
    p_uploaded.add_argument("--tag", required=True)
    p_uploaded.add_argument("--names-file", required=True)

    p_remote_tag = subparsers.add_parser("validate-remote-tag")
    p_remote_tag.add_argument("--repository", required=True, help="GitHub repository (owner/repo)")
    p_remote_tag.add_argument("--tag", required=True, help="Release tag")
    p_remote_tag.add_argument("--commit", required=True, help="Expected commit SHA")

    # validate-cargo
    p_cargo = subparsers.add_parser("validate-cargo")
    p_cargo.add_argument("--cargo-toml", default="Cargo.toml", help="Path to Cargo.toml")
    p_cargo.add_argument("--tag", required=True, help="Release tag")

    # validate-assets
    p_assets = subparsers.add_parser("validate-assets")
    p_assets.add_argument("--dist-dir", default="dist", help="Path to dist directory")
    p_assets.add_argument("--tag", required=True, help="Release tag")

    # generate-checksums
    p_sums = subparsers.add_parser("generate-checksums")
    p_sums.add_argument("--dist-dir", default="dist", help="Path to dist directory")
    p_sums.add_argument("--tag", required=True, help="Release tag")

    # verify-checksums
    p_check = subparsers.add_parser("verify-checksums")
    p_check.add_argument("--dist-dir", default="dist", help="Path to dist directory")

    args = parser.parse_args()

    if args.command == "validate-tag":
        _packaging.validate_tag(args.tag)

    elif args.command == "validate-uploaded":
        names = Path(args.names_file).read_text(encoding="utf-8").splitlines()
        validate_uploaded_assets(names, args.tag)

    elif args.command == "validate-remote-tag":
        validate_remote_tag(args.repository, args.tag, args.commit)
        print(f"Validated remote tag '{args.tag}' matches expected commit '{args.commit}'.")

    elif args.command == "validate-cargo":
        cargo_path = Path(args.cargo_toml)
        if not matches_cargo_version(cargo_path, args.tag):
            actual_version = parse_cargo_version(cargo_path)
            print(
                f"Error: Tag '{args.tag}' does not match Cargo.toml version '{actual_version}'",
                file=sys.stderr,
            )
            sys.exit(1)
        print(f"Tag '{args.tag}' matches Cargo.toml version.")

    elif args.command == "validate-assets":
        dist_dir = Path(args.dist_dir)
        assets = validate_dist_assets(dist_dir, args.tag)
        print(f"Validated all {len(assets)} expected release assets in {dist_dir}.")

    elif args.command == "generate-checksums":
        dist_dir = Path(args.dist_dir)
        sums_file = generate_checksums(dist_dir, args.tag)
        print(f"Generated {sums_file}.")

    elif args.command == "verify-checksums":
        dist_dir = Path(args.dist_dir)
        verify_checksums(dist_dir)
        print("All checksums in SHA256SUMS.txt successfully verified.")


if __name__ == "__main__":
    main()
