#!/usr/bin/env python3
"""Native Linux packaging helper (Debian and RPM) for git-explain.

Packages the already-compiled x86_64 musl static release binary into:
- git-explain_VERSION-1_amd64.deb
- git-explain-VERSION-1.x86_64.rpm

Policy:
Only plain stable tags (vX.Y.Z) generate native Linux packages.
Prerelease or build-metadata SemVer tags retain archive-only distribution.
"""

import argparse
import importlib.util
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

PLAIN_STABLE_REGEX = re.compile(r"^v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$")


def is_plain_stable_tag(tag: str) -> bool:
    """Return True if tag is a plain stable release tag (vX.Y.Z)."""
    _packaging.validate_tag(tag)
    return bool(PLAIN_STABLE_REGEX.match(tag))


def native_package_names(tag: str) -> List[str]:
    """Return the expected native package filenames for a given tag.

    Returns empty list for valid prerelease/build-variant tags per policy.
    Raises ValueError for invalid tag formats.
    """
    _packaging.validate_tag(tag)
    if not is_plain_stable_tag(tag):
        return []

    version = tag[1:]
    return [
        f"git-explain_{version}-1_amd64.deb",
        f"git-explain-{version}-1.x86_64.rpm",
    ]


def package_native(
    repo_root: Path,
    tag: str,
    dist_dir: Path,
) -> List[Path]:
    """Generate native Debian and RPM packages for a plain stable tag.

    Reuses the prebuilt musl static binary at:
    target/x86_64-unknown-linux-musl/release/git-explain

    Returns empty list if tag is a valid non-stable tag.
    """
    _packaging.validate_tag(tag)
    if not is_plain_stable_tag(tag):
        print(f"Notice: Tag '{tag}' is not a plain stable release (vX.Y.Z). Skipping native packages.")
        return []

    repo_root = repo_root.resolve()
    dist_dir = dist_dir.resolve()
    dist_dir.mkdir(parents=True, exist_ok=True)

    # 1. Require exact musl binary without fallback
    musl_bin = (
        repo_root
        / "target"
        / "x86_64-unknown-linux-musl"
        / "release"
        / "git-explain"
    )
    if not musl_bin.is_file():
        raise FileNotFoundError(
            f"Exact release binary not found at '{musl_bin}'. Stale fallbacks are rejected."
        )

    # 2. Require documentation and license files
    required_docs = [
        repo_root / "README.md",
        repo_root / "LICENSE",
        repo_root / "docs" / "releases.md",
    ]
    for doc in required_docs:
        if not doc.is_file():
            raise FileNotFoundError(
                f"Required release document not found at '{doc}'. Cannot package native release."
            )

    version = tag[1:]
    deb_filename = f"git-explain_{version}-1_amd64.deb"
    rpm_filename = f"git-explain-{version}-1.x86_64.rpm"
    deb_path = dist_dir / deb_filename
    rpm_path = dist_dir / rpm_filename

    # 3. Generate Debian package using cargo-deb
    # Must use --no-build and --no-strip to reuse exact built binary without recompilation
    deb_cmd = [
        "cargo",
        "deb",
        "--no-build",
        "--no-strip",
        "--target",
        "x86_64-unknown-linux-musl",
        "--deb-version",
        f"{version}-1",
        "--output",
        str(deb_path),
    ]
    subprocess.run(deb_cmd, cwd=repo_root, check=True)

    # 4. Generate RPM package using cargo-generate-rpm
    rpm_cmd = [
        "cargo",
        "generate-rpm",
        "--target",
        "x86_64-unknown-linux-musl",
        "--output",
        str(rpm_path),
        "--set-metadata",
        f'version = "{version}"',
    ]
    subprocess.run(rpm_cmd, cwd=repo_root, check=True)

    # 5. Check outputs created before returning
    if not deb_path.is_file():
        raise FileNotFoundError(
            f"Expected deb package was not created at '{deb_path}'"
        )
    if not rpm_path.is_file():
        raise FileNotFoundError(
            f"Expected rpm package was not created at '{rpm_path}'"
        )

    return [deb_path, rpm_path]


def main():
    parser = argparse.ArgumentParser(
        description="git-explain native Linux packaging helper (Debian and RPM)."
    )
    parser.add_argument(
        "--repo-root",
        default=".",
        help="Path to repository root (default: current directory)",
    )
    parser.add_argument(
        "--tag",
        required=True,
        help="Release tag to package (e.g. v0.2.1)",
    )
    parser.add_argument(
        "--dist-dir",
        default="dist",
        help="Path to output dist directory (default: dist)",
    )

    args = parser.parse_args()
    repo_root = Path(args.repo_root)
    dist_dir = Path(args.dist_dir)

    packages = package_native(repo_root=repo_root, tag=args.tag, dist_dir=dist_dir)
    for pkg in packages:
        print(f"Generated native package: {pkg}")


if __name__ == "__main__":
    main()
