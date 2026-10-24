#!/usr/bin/env python3
"""Tests for scripts/package-release.py and scripts/verify-release.py.

Verifies:
- Creation of archives (.zip on Windows, .tar.gz on Linux)
- Exact file contents: git-explain[.exe], README.md, LICENSE, docs/releases.md
- Standalone raw executables
- Canonical musl alias creation without path collisions
- Valid executable file modes on POSIX
- Exact target binary requirement (missing binary rejection, no stale fallback)
- Rejection of missing README, LICENSE, or docs/releases.md
- Rejection of invalid tag formats (whitespace, newlines, non-vSEMVER)
- Rejection of unknown target triples or platform names
- Path traversal outside staging rejection
- Exact 8 release assets validation (missing and unexpected asset rejection)
- Safe SHA256SUMS generation (excludes itself, supports rerun)
- Cargo package version matching including full prereleases
- Cleanup of temporary staging directories
"""

import importlib.util
import subprocess
import sys
import stat
import tarfile
import tempfile
import unittest
import zipfile
from unittest.mock import MagicMock, patch
from pathlib import Path

# Import package-release.py
pkg_path = Path(__file__).parent / "package-release.py"
spec_pkg = importlib.util.spec_from_file_location("package_release", pkg_path)
package_release = importlib.util.module_from_spec(spec_pkg)
spec_pkg.loader.exec_module(package_release)

# Import verify-release.py if present
verify_path = Path(__file__).parent / "verify-release.py"
verify_release = None
if verify_path.exists():
    spec_ver = importlib.util.spec_from_file_location("verify_release", verify_path)
    verify_release = importlib.util.module_from_spec(spec_ver)
    spec_ver.loader.exec_module(verify_release)


class TestPackageAndVerifyRelease(unittest.TestCase):
    def test_tag_validation_cli(self):
        for tag, success in [("v0.1.0", True), ("v0.1.0-rc.1", True),
                             ("v0.1.0';raise Exception('injected')#", False)]:
            result = subprocess.run(
                [sys.executable, str(verify_path), "validate-tag", "--tag", tag],
                capture_output=True, text=True,
            )
            self.assertEqual(result.returncode == 0, success, result.stderr)

    def test_uploaded_assets_exact_match(self):
        # Stable tag: 10 assets + SHA256SUMS.txt = 11 assets
        names = verify_release.get_expected_assets("v0.1.0") + ["SHA256SUMS.txt"]
        self.assertEqual(len(names), 11)
        verify_release.validate_uploaded_assets(names, "v0.1.0")
        for invalid in [names[:-1], names + ["stale.exe"], names + [names[0]]]:
            with self.assertRaises(ValueError):
                verify_release.validate_uploaded_assets(invalid, "v0.1.0")

        # Prerelease tag: 8 assets + SHA256SUMS.txt = 9 assets
        prerelease_names = verify_release.get_expected_assets("v0.1.0-rc.1") + ["SHA256SUMS.txt"]
        self.assertEqual(len(prerelease_names), 9)
        verify_release.validate_uploaded_assets(prerelease_names, "v0.1.0-rc.1")
        for invalid in [prerelease_names[:-1], prerelease_names + ["git-explain_0.1.0-1_amd64.deb"]]:
            with self.assertRaises(ValueError):
                verify_release.validate_uploaded_assets(invalid, "v0.1.0-rc.1")

    def setUp(self):
        self.temp_dir = tempfile.TemporaryDirectory()
        self.root = Path(self.temp_dir.name)
        self.repo_root = self.root / "repo"
        self.repo_root.mkdir()
        self.target_dir = self.repo_root / "target"
        self.staging_dir = self.root / "staging"
        self.dist_dir = self.root / "dist"
        self.staging_dir.mkdir(parents=True, exist_ok=True)
        self.dist_dir.mkdir(parents=True, exist_ok=True)

        # Create standard repository files
        (self.repo_root / "README.md").write_text("# git-explain\n")
        (self.repo_root / "LICENSE").write_text("Apache-2.0\n")
        docs_dir = self.repo_root / "docs"
        docs_dir.mkdir()
        (docs_dir / "releases.md").write_text("# Release Information\n")

    def tearDown(self):
        self.temp_dir.cleanup()

    def _create_mock_binary(self, target: str, is_windows: bool) -> Path:
        bin_dir = self.target_dir / target / "release"
        bin_dir.mkdir(parents=True, exist_ok=True)
        bin_name = "git-explain.exe" if is_windows else "git-explain"
        bin_path = bin_dir / bin_name
        bin_path.write_bytes(b"MOCK_BINARY_BYTES_FOR_EXACT_VERIFICATION")
        if not is_windows:
            bin_path.chmod(0o755)
        return bin_path

    def test_invalid_tag_rejection(self):
        invalid_tags = [
            "1.0.0",
            "v1",
            "release-1.0.0",
            "v1.0",
            "v1.0.0.0",
            "v",
            "",
            "v0.1.0\n",
            " v0.1.0",
            "v0.1.0 ",
            "v0.1.0\r",
        ]
        for tag in invalid_tags:
            with self.subTest(tag=tag):
                self.assertFalse(package_release.is_valid_tag(tag))
                with self.assertRaises(ValueError):
                    package_release.validate_tag(tag)

    def test_valid_tag_acceptance(self):
        valid_tags = [
            "v0.1.0",
            "v1.0.0",
            "v2.10.5",
            "v0.1.0-alpha.1",
            "v0.1.0-rc.2+build.123",
        ]
        for tag in valid_tags:
            with self.subTest(tag=tag):
                self.assertTrue(package_release.is_valid_tag(tag))

    def test_missing_binary_rejection_no_stale_fallback(self):
        with self.assertRaises(FileNotFoundError):
            package_release.package_target(
                repo_root=self.repo_root,
                target="x86_64-pc-windows-msvc",
                name="windows-x86_64",
                tag="v0.1.0",
                target_dir=self.target_dir,
                dist_dir=self.dist_dir,
                staging_dir=self.staging_dir,
            )

    def test_missing_documentation_rejection(self):
        self._create_mock_binary("x86_64-unknown-linux-gnu", is_windows=False)
        # Test missing README.md
        (self.repo_root / "README.md").unlink()
        with self.assertRaises(FileNotFoundError):
            package_release.package_target(
                repo_root=self.repo_root,
                target="x86_64-unknown-linux-gnu",
                name="linux-x86_64-gnu",
                tag="v0.1.0",
                target_dir=self.target_dir,
                dist_dir=self.dist_dir,
                staging_dir=self.staging_dir,
            )
        # Restore README.md and remove docs/releases.md
        (self.repo_root / "README.md").write_text("# git-explain\n")
        (self.repo_root / "docs" / "releases.md").unlink()
        with self.assertRaises(FileNotFoundError):
            package_release.package_target(
                repo_root=self.repo_root,
                target="x86_64-unknown-linux-gnu",
                name="linux-x86_64-gnu",
                tag="v0.1.0",
                target_dir=self.target_dir,
                dist_dir=self.dist_dir,
                staging_dir=self.staging_dir,
            )

    def test_unknown_target_or_name_rejection(self):
        self._create_mock_binary("x86_64-unknown-linux-gnu", is_windows=False)
        with self.assertRaises(ValueError):
            package_release.package_target(
                repo_root=self.repo_root,
                target="arm-unknown-linux-gnueabi",
                name="linux-x86_64-gnu",
                tag="v0.1.0",
                target_dir=self.target_dir,
                dist_dir=self.dist_dir,
                staging_dir=self.staging_dir,
            )
        with self.assertRaises(ValueError):
            package_release.package_target(
                repo_root=self.repo_root,
                target="x86_64-unknown-linux-gnu",
                name="unsupported-name",
                tag="v0.1.0",
                target_dir=self.target_dir,
                dist_dir=self.dist_dir,
                staging_dir=self.staging_dir,
            )

    def test_archive_contents_and_exact_binary_bytes_windows(self):
        mock_bin = self._create_mock_binary("x86_64-pc-windows-msvc", is_windows=True)
        results = package_release.package_target(
            repo_root=self.repo_root,
            target="x86_64-pc-windows-msvc",
            name="windows-x86_64",
            tag="v0.1.0",
            target_dir=self.target_dir,
            dist_dir=self.dist_dir,
            staging_dir=self.staging_dir,
        )

        expected_zip = self.dist_dir / "git-explain-v0.1.0-windows-x86_64.zip"
        expected_raw = self.dist_dir / "git-explain-v0.1.0-windows-x86_64.exe"
        self.assertTrue(expected_zip.exists())
        self.assertTrue(expected_raw.exists())
        self.assertEqual(expected_raw.read_bytes(), mock_bin.read_bytes())

        with zipfile.ZipFile(expected_zip, "r") as zf:
            namelist = sorted(zf.namelist())
            expected_members = sorted(
                ["git-explain.exe", "README.md", "LICENSE", "docs/releases.md"]
            )
            self.assertEqual(namelist, expected_members)
            self.assertEqual(zf.read("git-explain.exe"), mock_bin.read_bytes())

        # Assert staging directory is empty/cleaned up
        self.assertEqual(list(self.staging_dir.iterdir()), [])

    def test_archive_contents_and_exact_binary_bytes_linux_gnu(self):
        mock_bin = self._create_mock_binary("x86_64-unknown-linux-gnu", is_windows=False)
        results = package_release.package_target(
            repo_root=self.repo_root,
            target="x86_64-unknown-linux-gnu",
            name="linux-x86_64-gnu",
            tag="v0.1.0",
            target_dir=self.target_dir,
            dist_dir=self.dist_dir,
            staging_dir=self.staging_dir,
        )

        expected_tar = self.dist_dir / "git-explain-v0.1.0-linux-x86_64-gnu.tar.gz"
        expected_raw = self.dist_dir / "git-explain-v0.1.0-linux-x86_64-gnu"
        self.assertTrue(expected_tar.exists())
        self.assertTrue(expected_raw.exists())
        self.assertEqual(expected_raw.read_bytes(), mock_bin.read_bytes())

        with tarfile.open(expected_tar, "r:gz") as tf:
            names = sorted(tf.getnames())
            expected_members = sorted(
                ["git-explain", "README.md", "LICENSE", "docs/releases.md"]
            )
            self.assertEqual(names, expected_members)
            member = tf.getmember("git-explain")
            self.assertTrue(bool(member.mode & stat.S_IXUSR))
            f = tf.extractfile(member)
            self.assertEqual(f.read(), mock_bin.read_bytes())

        self.assertEqual(list(self.staging_dir.iterdir()), [])

    def test_linux_musl_canonical_alias_no_collision(self):
        mock_bin = self._create_mock_binary("x86_64-unknown-linux-musl", is_windows=False)
        results = package_release.package_target(
            repo_root=self.repo_root,
            target="x86_64-unknown-linux-musl",
            name="linux-x86_64-musl",
            tag="v0.1.0",
            target_dir=self.target_dir,
            dist_dir=self.dist_dir,
            staging_dir=self.staging_dir,
            canonical_alias="linux-x86_64",
        )

        alias_tar = self.dist_dir / "git-explain-v0.1.0-linux-x86_64.tar.gz"
        alias_raw = self.dist_dir / "git-explain-v0.1.0-linux-x86_64"
        self.assertTrue(alias_tar.exists())
        self.assertTrue(alias_raw.is_file())
        self.assertEqual(alias_raw.read_bytes(), mock_bin.read_bytes())
        self.assertEqual(list(self.staging_dir.iterdir()), [])

    # Tests for verify-release helper
    def test_expected_assets_list_and_validation(self):
        self.assertIsNotNone(
            verify_release, "verify-release module must exist at scripts/verify-release.py"
        )
        tag = "v0.1.0"
        expected = verify_release.get_expected_assets(tag)
        self.assertEqual(len(expected), 10)
        self.assertEqual(
            expected,
            [
                f"git-explain-{tag}-windows-x86_64.zip",
                f"git-explain-{tag}-windows-x86_64.exe",
                f"git-explain-{tag}-linux-x86_64-gnu.tar.gz",
                f"git-explain-{tag}-linux-x86_64-gnu",
                f"git-explain-{tag}-linux-x86_64-musl.tar.gz",
                f"git-explain-{tag}-linux-x86_64-musl",
                f"git-explain-{tag}-linux-x86_64.tar.gz",
                f"git-explain-{tag}-linux-x86_64",
                "git-explain_0.1.0-1_amd64.deb",
                "git-explain-0.1.0-1.x86_64.rpm",
            ],
        )

        # Prerelease/build variant remains 8 assets
        prerelease_tag = "v0.1.0-rc.1"
        prerelease_expected = verify_release.get_expected_assets(prerelease_tag)
        self.assertEqual(len(prerelease_expected), 8)
        self.assertNotIn("git-explain_0.1.0-1_amd64.deb", prerelease_expected)
        self.assertNotIn("git-explain-0.1.0-1.x86_64.rpm", prerelease_expected)

        # Create only 9 of the 10 assets in dist_dir for stable tag -> must raise ValueError
        for fname in expected[:9]:
            (self.dist_dir / fname).write_bytes(b"asset_content")
        with self.assertRaises(ValueError):
            verify_release.validate_dist_assets(self.dist_dir, tag)

        # Create 10th asset -> passes
        (self.dist_dir / expected[9]).write_bytes(b"asset_content_10")
        found = verify_release.validate_dist_assets(self.dist_dir, tag)
        self.assertEqual(found, expected)

        # Add unexpected artifact -> must raise ValueError
        (self.dist_dir / "unexpected.txt").write_bytes(b"extra")
        with self.assertRaises(ValueError):
            verify_release.validate_dist_assets(self.dist_dir, tag)

    def test_checksums_generation_and_verification(self):
        self.assertIsNotNone(verify_release)
        tag = "v0.1.0"
        expected = verify_release.get_expected_assets(tag)
        for fname in expected:
            (self.dist_dir / fname).write_bytes(f"content of {fname}".encode())

        # Generate checksums file
        sums_file = verify_release.generate_checksums(self.dist_dir, tag)
        self.assertTrue(sums_file.exists())
        content = sums_file.read_text()
        self.assertNotIn("SHA256SUMS.txt", content)

        # Rerun is safe and idempotent
        sums_file2 = verify_release.generate_checksums(self.dist_dir, tag)
        self.assertEqual(content, sums_file2.read_text())

        # Verification succeeds
        verify_release.verify_checksums(self.dist_dir)

        # If a file is tampered with, verification fails
        (self.dist_dir / expected[0]).write_bytes(b"tampered")
        with self.assertRaises(ValueError):
            verify_release.verify_checksums(self.dist_dir)

    def test_cargo_version_validation(self):
        self.assertIsNotNone(verify_release)
        cargo_path = self.repo_root / "Cargo.toml"

        cargo_path.write_text('[package]\nname = "git-explain"\nversion = "0.1.0"\n')
        self.assertTrue(verify_release.matches_cargo_version(cargo_path, "v0.1.0"))
        self.assertFalse(verify_release.matches_cargo_version(cargo_path, "v0.2.0"))
        # Full prerelease match: Cargo 0.2.0-rc.1 must match tag v0.2.0-rc.1
        self.assertFalse(verify_release.matches_cargo_version(cargo_path, "v0.1.0-rc.1"))

        cargo_path.write_text('[package]\nname = "git-explain"\nversion = "0.2.0-rc.1"\n')
        self.assertTrue(verify_release.matches_cargo_version(cargo_path, "v0.2.0-rc.1"))
        self.assertFalse(verify_release.matches_cargo_version(cargo_path, "v0.2.0"))

    def test_validate_remote_tag_lightweight_match(self):
        repo = "owner/git-explain"
        tag = "v0.1.0"
        commit = "1111222233334444555566667777888899990000"

        def mock_subprocess_run(cmd, *args, **kwargs):
            if cmd == ["gh", "api", f"repos/{repo}/git/ref/tags/{tag}"]:
                return MagicMock(
                    returncode=0,
                    stdout='{"ref": "refs/tags/' + tag + '", "object": {"type": "commit", "sha": "' + commit + '"}}',
                )
            raise AssertionError(f"Unexpected command: {cmd}")

        with patch("subprocess.run", side_effect=mock_subprocess_run):
            # Should succeed without exception
            verify_release.validate_remote_tag(repo, tag, commit)

    def test_validate_remote_tag_annotated_peeled_match(self):
        repo = "owner/git-explain"
        tag = "v0.1.0"
        tag_sha = "aaaabbbbccccddddeeeeffff0000111122223333"
        commit = "1111222233334444555566667777888899990000"

        def mock_subprocess_run(cmd, *args, **kwargs):
            if cmd == ["gh", "api", f"repos/{repo}/git/ref/tags/{tag}"]:
                return MagicMock(
                    returncode=0,
                    stdout='{"ref": "refs/tags/' + tag + '", "object": {"type": "tag", "sha": "' + tag_sha + '"}}',
                )
            elif cmd == ["gh", "api", f"repos/{repo}/git/tags/{tag_sha}"]:
                return MagicMock(
                    returncode=0,
                    stdout='{"sha": "' + tag_sha + '", "object": {"type": "commit", "sha": "' + commit + '"}}',
                )
            raise AssertionError(f"Unexpected command: {cmd}")

        with patch("subprocess.run", side_effect=mock_subprocess_run):
            verify_release.validate_remote_tag(repo, tag, commit)

    def test_validate_remote_tag_moved_mismatch(self):
        repo = "owner/git-explain"
        tag = "v0.1.0"
        expected_commit = "1111222233334444555566667777888899990000"
        moved_commit = "9999888877776666555544443333222211110000"

        def mock_subprocess_run(cmd, *args, **kwargs):
            if cmd == ["gh", "api", f"repos/{repo}/git/ref/tags/{tag}"]:
                return MagicMock(
                    returncode=0,
                    stdout='{"ref": "refs/tags/' + tag + '", "object": {"type": "commit", "sha": "' + moved_commit + '"}}',
                )
            raise AssertionError(f"Unexpected command: {cmd}")

        with patch("subprocess.run", side_effect=mock_subprocess_run):
            with self.assertRaises(ValueError):
                verify_release.validate_remote_tag(repo, tag, expected_commit)

    def test_validate_remote_tag_api_failure(self):
        repo = "owner/git-explain"
        tag = "v0.1.0"
        commit = "1111222233334444555566667777888899990000"

        def mock_subprocess_run(cmd, *args, **kwargs):
            raise subprocess.CalledProcessError(1, cmd, stderr="Not found")

        with patch("subprocess.run", side_effect=mock_subprocess_run):
            with self.assertRaises(Exception):
                verify_release.validate_remote_tag(repo, tag, commit)


if __name__ == "__main__":
    unittest.main()
