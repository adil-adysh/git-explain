#!/usr/bin/env python3
"""Cross-platform release packaging script for git-explain.

Packages compiled release binaries into platform archives (.zip for Windows,
.tar.gz for Linux) alongside documentation (README.md, LICENSE, docs/releases.md),
and exposes standalone executables and optional canonical aliases with distinct
staging directories to avoid collisions.
"""

import argparse
import hashlib
import os
import re
import shutil
import stat
import subprocess
import sys
import tarfile
import zipfile
from pathlib import Path
from typing import List, Optional

SEMVER_TAG_REGEX = re.compile(
    r"^v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)"
    r"(?:-((?:0|[1-9]\d*|\d*[a-zA-Z-][0-9a-zA-Z-]*)(?:\.(?:0|[1-9]\d*|\d*[a-zA-Z-][0-9a-zA-Z-]*))*))?"
    r"(?:\+([0-9a-zA-Z-]+(?:\.[0-9a-zA-Z-]+)*))?$"
)

ALLOWED_TARGET_NAMES = {
    "x86_64-pc-windows-msvc": "windows-x86_64",
    "x86_64-unknown-linux-gnu": "linux-x86_64-gnu",
    "x86_64-unknown-linux-musl": "linux-x86_64-musl",
}

ALLOWED_CANONICAL_ALIASES = {"linux-x86_64", None, ""}


def is_valid_tag(tag: str) -> bool:
    """Return True if tag matches vSEMVER format without whitespace or newlines."""
    if not tag or tag != tag.strip() or "\n" in tag or "\r" in tag:
        return False
    return bool(SEMVER_TAG_REGEX.match(tag))


def validate_tag(tag: str) -> str:
    """Validate and return tag or raise ValueError."""
    if not is_valid_tag(tag):
        raise ValueError(
            f"Invalid tag format '{tag}'. Must follow vSEMVER without whitespace (e.g. v0.1.0 or v0.1.0-rc.1)"
        )
    return tag


def sha256_file(path: Path) -> str:
    """Calculate SHA256 hexadecimal digest of a file."""
    h = hashlib.sha256()
    with path.open("rb") as f:
        while chunk := f.read(65536):
            h.update(chunk)
    return h.hexdigest()


def _add_to_tar(tf: tarfile.TarFile, src_path: Path, arcname: str, is_executable: bool = False):
    """Add a file to a tar archive with controlled permissions."""
    info = tf.gettarinfo(str(src_path), arcname=arcname)
    info.uid = 0
    info.gid = 0
    info.uname = ""
    info.gname = ""
    info.mode = 0o755 if is_executable else 0o644
    with src_path.open("rb") as f:
        tf.addfile(info, f)


def create_archive_from_dir(
    stage_dir: Path,
    output_archive: Path,
    is_windows: bool,
):
    """Create .zip or .tar.gz archive from the contents of stage_dir."""
    output_archive.parent.mkdir(parents=True, exist_ok=True)
    if is_windows:
        with zipfile.ZipFile(output_archive, "w", zipfile.ZIP_DEFLATED) as zf:
            for item in sorted(stage_dir.rglob("*")):
                if item.is_file():
                    arcname = item.relative_to(stage_dir).as_posix()
                    zf.write(item, arcname)
    else:
        with tarfile.open(output_archive, "w:gz") as tf:
            for item in sorted(stage_dir.rglob("*")):
                if item.is_file():
                    arcname = item.relative_to(stage_dir).as_posix()
                    is_exec = (
                        arcname == "git-explain"
                        or bool(item.stat().st_mode & stat.S_IXUSR)
                    )
                    _add_to_tar(tf, item, arcname, is_executable=is_exec)


def _validate_staging_path(staging_dir: Path, stage_dir: Path):
    """Ensure stage_dir is strictly inside staging_dir to prevent arbitrary deletions."""
    try:
        stage_dir.resolve().relative_to(staging_dir.resolve())
    except ValueError:
        raise ValueError(f"Staging path '{stage_dir}' escapes staging directory '{staging_dir}'")


def _stage_and_build_archive(
    source_bin: Path,
    repo_root: Path,
    stage_dir: Path,
    staging_dir: Path,
    archive_path: Path,
    is_windows: bool,
):
    """Stage required release files into stage_dir and compress into archive_path."""
    _validate_staging_path(staging_dir, stage_dir)
    if stage_dir.exists():
        shutil.rmtree(stage_dir)
    stage_dir.mkdir(parents=True, exist_ok=True)

    try:
        # 1. Require documentation files
        required_docs = [
            (repo_root / "README.md", stage_dir / "README.md"),
            (repo_root / "LICENSE", stage_dir / "LICENSE"),
            (repo_root / "docs" / "releases.md", stage_dir / "docs" / "releases.md"),
        ]
        for src, dst in required_docs:
            if not src.exists():
                raise FileNotFoundError(
                    f"Required release document not found at '{src}'. Cannot package release."
                )
            dst.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(src, dst)

        # 2. Binary with canonical release name
        bin_target_name = "git-explain.exe" if is_windows else "git-explain"
        dest_bin = stage_dir / bin_target_name
        shutil.copy2(source_bin, dest_bin)
        if not is_windows:
            dest_bin.chmod(0o755)

        # 3. Create archive
        create_archive_from_dir(stage_dir, archive_path, is_windows=is_windows)
    finally:
        if stage_dir.exists():
            shutil.rmtree(stage_dir)


def package_target(
    repo_root: Path,
    target: str,
    name: str,
    tag: str,
    target_dir: Optional[Path] = None,
    dist_dir: Optional[Path] = None,
    staging_dir: Optional[Path] = None,
    canonical_alias: Optional[str] = None,
) -> List[Path]:
    """Package a single target into dist_dir with archives and raw executables."""
    valid_tag = validate_tag(tag)

    if target not in ALLOWED_TARGET_NAMES:
        raise ValueError(
            f"Unsupported target '{target}'. Allowed: {list(ALLOWED_TARGET_NAMES.keys())}"
        )
    if ALLOWED_TARGET_NAMES[target] != name:
        raise ValueError(
            f"Target '{target}' expects platform name '{ALLOWED_TARGET_NAMES[target]}', got '{name}'"
        )
    if canonical_alias not in ALLOWED_CANONICAL_ALIASES:
        raise ValueError(
            f"Unsupported canonical alias '{canonical_alias}'. Allowed: {ALLOWED_CANONICAL_ALIASES}"
        )

    repo_root = repo_root.resolve()
    target_dir = (target_dir or repo_root / "target").resolve()
    dist_dir = (dist_dir or repo_root / "dist").resolve()
    staging_dir = (staging_dir or repo_root / "staging").resolve()

    dist_dir.mkdir(parents=True, exist_ok=True)
    staging_dir.mkdir(parents=True, exist_ok=True)

    is_windows = "windows" in target.lower()
    bin_name = "git-explain.exe" if is_windows else "git-explain"
    source_bin = target_dir / target / "release" / bin_name

    if not source_bin.exists():
        raise FileNotFoundError(
            f"Exact release binary not found at '{source_bin}'. Stale fallbacks are rejected."
        )

    if not is_windows:
        source_bin.chmod(0o755)

    created_artifacts: List[Path] = []

    # 1. Main platform package
    pkg_basename = f"git-explain-{valid_tag}-{name}"
    archive_ext = ".zip" if is_windows else ".tar.gz"
    archive_path = dist_dir / f"{pkg_basename}{archive_ext}"
    stage_path = staging_dir / f"stage_{pkg_basename}"

    _stage_and_build_archive(
        source_bin=source_bin,
        repo_root=repo_root,
        stage_dir=stage_path,
        staging_dir=staging_dir,
        archive_path=archive_path,
        is_windows=is_windows,
    )
    created_artifacts.append(archive_path)

    # 2. Standalone raw binary
    raw_bin_name = f"{pkg_basename}.exe" if is_windows else pkg_basename
    raw_bin_path = dist_dir / raw_bin_name
    shutil.copy2(source_bin, raw_bin_path)
    if not is_windows:
        raw_bin_path.chmod(0o755)
    created_artifacts.append(raw_bin_path)

    # 3. Optional canonical alias (e.g. linux-x86_64 for musl)
    if canonical_alias:
        alias_basename = f"git-explain-{valid_tag}-{canonical_alias}"
        alias_archive_path = dist_dir / f"{alias_basename}{archive_ext}"
        alias_stage_path = staging_dir / f"stage_{alias_basename}"

        _stage_and_build_archive(
            source_bin=source_bin,
            repo_root=repo_root,
            stage_dir=alias_stage_path,
            staging_dir=staging_dir,
            archive_path=alias_archive_path,
            is_windows=is_windows,
        )
        created_artifacts.append(alias_archive_path)

        alias_raw_bin_name = f"{alias_basename}.exe" if is_windows else alias_basename
        alias_raw_bin_path = dist_dir / alias_raw_bin_name
        shutil.copy2(source_bin, alias_raw_bin_path)
        if not is_windows:
            alias_raw_bin_path.chmod(0o755)
        created_artifacts.append(alias_raw_bin_path)

    return created_artifacts


def smoke_test(binary_path: Path):
    """Run help on the built binary to confirm executable launch."""
    print(f"Running smoke test on {binary_path}...")
    cmd = [str(binary_path), "-h"]
    proc = subprocess.run(cmd, capture_output=True, text=True)
    if proc.returncode != 0:
        raise RuntimeError(
            f"Smoke test failed for '{binary_path}' with code {proc.returncode}:\n{proc.stderr}"
        )
    print("Smoke test passed: help displayed successfully.")


def main():
    parser = argparse.ArgumentParser(description="Package git-explain release binaries.")
    parser.add_argument("--target", required=True, help="Rust target triple")
    parser.add_argument("--name", required=True, help="Platform name identifier")
    parser.add_argument("--tag", required=True, help="Release tag (e.g. v0.1.0)")
    parser.add_argument(
        "--canonical-alias",
        default=None,
        help="Optional canonical alias (e.g. linux-x86_64)",
    )
    parser.add_argument("--repo-root", default=".", help="Path to repository root")
    parser.add_argument(
        "--target-dir", default=None, help="Path to Cargo target directory"
    )
    parser.add_argument("--dist-dir", default=None, help="Path to output dist directory")
    parser.add_argument(
        "--staging-dir", default=None, help="Path to staging temporary directory"
    )
    parser.add_argument(
        "--smoke-test",
        action="store_true",
        help="Execute a smoke test on the built binary",
    )

    args = parser.parse_args()

    repo_root = Path(args.repo_root).resolve()
    target_dir = Path(args.target_dir).resolve() if args.target_dir else repo_root / "target"
    dist_dir = Path(args.dist_dir).resolve() if args.dist_dir else repo_root / "dist"
    staging_dir = (
        Path(args.staging_dir).resolve() if args.staging_dir else repo_root / "staging"
    )

    if args.smoke_test:
        is_windows = "windows" in args.target.lower()
        bin_name = "git-explain.exe" if is_windows else "git-explain"
        bin_path = target_dir / args.target / "release" / bin_name
        smoke_test(bin_path)

    artifacts = package_target(
        repo_root=repo_root,
        target=args.target,
        name=args.name,
        tag=args.tag,
        target_dir=target_dir,
        dist_dir=dist_dir,
        staging_dir=staging_dir,
        canonical_alias=args.canonical_alias,
    )

    print("Created artifacts:")
    for art in artifacts:
        print(f"  {art.name} ({sha256_file(art)})")


if __name__ == "__main__":
    main()
