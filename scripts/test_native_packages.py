#!/usr/bin/env python3
"""Tests for native packaging metadata and scripts/package-native.py.

Verifies:
- Cargo.toml metadata for deb and generate-rpm (license, maintainer, assets, dependencies, no scriptlets)
- Package naming helper for plain stable tags vs prereleases/build variants
- Rejection of invalid tags
- Missing exact musl binary error without fallback
- Subprocess invocation using exact commands without rebuild (--no-build, no shell)
- CLI argument handling
"""

import importlib.util
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import MagicMock, patch

try:
    import tomllib
except ImportError:
    import tomli as tomllib  # type: ignore


REPO_ROOT = Path(__file__).resolve().parent.parent
CARGO_TOML = REPO_ROOT / "Cargo.toml"

PKG_NATIVE_PATH = REPO_ROOT / "scripts" / "package-native.py"


def load_package_native_module():
    if not PKG_NATIVE_PATH.exists():
        return None
    spec = importlib.util.spec_from_file_location("package_native", PKG_NATIVE_PATH)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


class TestCargoMetadata(unittest.TestCase):
    def setUp(self):
        with open(CARGO_TOML, "rb") as f:
            self.cargo_data = tomllib.load(f)

    def test_package_general_metadata(self):
        pkg = self.cargo_data.get("package", {})
        self.assertEqual(pkg.get("license"), "Apache-2.0")
        self.assertEqual(
            pkg.get("repository"), "https://github.com/adil-adysh/git-explain"
        )
        self.assertEqual(
            pkg.get("homepage"), "https://github.com/adil-adysh/git-explain"
        )

    def test_deb_metadata(self):
        deb = self.cargo_data.get("package", {}).get("metadata", {}).get("deb", {})
        self.assertEqual(
            deb.get("maintainer"), "adil-adysh <adil-adysh@users.noreply.github.com>"
        )
        self.assertEqual(deb.get("depends"), "git")

        # Must not contain daemon or scriptlet hooks
        for forbidden in [
            "systemd",
            "postinst",
            "preinst",
            "prerm",
            "postrm",
            "maintainer-scripts",
        ]:
            self.assertNotIn(
                forbidden, deb, f"Forbidden scriptlet or service key '{forbidden}' found in deb metadata"
            )

        assets = deb.get("assets", [])
        self.assertTrue(len(assets) >= 4, f"Expected at least 4 assets in deb metadata, got {assets}")

        # Check binary asset
        bin_asset = [
            a for a in assets if len(a) == 3 and a[0] == "target/release/git-explain"
        ]
        self.assertEqual(len(bin_asset), 1, "deb assets must specify target/release/git-explain")
        self.assertEqual(bin_asset[0][1].lstrip("/"), "usr/bin/git-explain")
        self.assertEqual(bin_asset[0][2], "755")

        # Check documentation and license assets
        dest_map = {a[0]: a[1].lstrip("/") for a in assets if len(a) == 3}
        self.assertEqual(
            dest_map.get("README.md"), "usr/share/doc/git-explain/README.md"
        )
        self.assertEqual(
            dest_map.get("LICENSE"), "usr/share/doc/git-explain/LICENSE"
        )
        self.assertEqual(
            dest_map.get("docs/releases.md"), "usr/share/doc/git-explain/releases.md"
        )

    def test_generate_rpm_metadata(self):
        rpm = (
            self.cargo_data.get("package", {})
            .get("metadata", {})
            .get("generate-rpm", {})
        )
        self.assertEqual(str(rpm.get("release")), "1")
        self.assertEqual(rpm.get("auto-req"), "no")
        self.assertEqual(rpm.get("require-sh"), False)

        requires = rpm.get("requires", {})
        self.assertEqual(requires.get("git"), "*")

        # Must not contain scriptlets
        for forbidden in [
            "pre_install_script",
            "post_install_script",
            "pre_uninstall_script",
            "post_uninstall_script",
        ]:
            self.assertNotIn(
                forbidden, rpm, f"Forbidden scriptlet key '{forbidden}' found in RPM metadata"
            )

        assets = rpm.get("assets", [])
        self.assertTrue(len(assets) >= 4, f"Expected at least 4 assets in RPM metadata, got {assets}")

        dest_map = {a.get("source"): a.get("dest") for a in assets if isinstance(a, dict)}
        mode_map = {a.get("source"): str(a.get("mode", "")).lstrip("0") for a in assets if isinstance(a, dict)}

        self.assertEqual(dest_map.get("target/release/git-explain"), "/usr/bin/git-explain")
        self.assertEqual(mode_map.get("target/release/git-explain"), "755")

        self.assertEqual(dest_map.get("README.md"), "/usr/share/doc/git-explain/README.md")
        self.assertEqual(dest_map.get("docs/releases.md"), "/usr/share/doc/git-explain/releases.md")
        self.assertEqual(dest_map.get("LICENSE"), "/usr/share/licenses/git-explain/LICENSE")


class TestPackageNative(unittest.TestCase):
    def setUp(self):
        self.mod = load_package_native_module()
        self.assertIsNotNone(
            self.mod, "scripts/package-native.py must exist and be loadable"
        )
        self.temp_dir = tempfile.TemporaryDirectory()
        self.root = Path(self.temp_dir.name).resolve()
        self.repo_root = (self.root / "repo").resolve()
        self.repo_root.mkdir()
        self.dist_dir = (self.root / "dist").resolve()
        self.dist_dir.mkdir()

        # Minimal Cargo.toml
        (self.repo_root / "Cargo.toml").write_text(
            '[package]\nname = "git-explain"\nversion = "0.2.1"\n'
        )
        (self.repo_root / "README.md").write_text("# git-explain\n")
        (self.repo_root / "LICENSE").write_text("Apache-2.0\n")
        docs_dir = self.repo_root / "docs"
        docs_dir.mkdir()
        (docs_dir / "releases.md").write_text("# Releases\n")

    def tearDown(self):
        self.temp_dir.cleanup()

    def test_native_package_names_plain_stable(self):
        names = self.mod.native_package_names("v0.2.1")
        self.assertEqual(
            names,
            [
                "git-explain_0.2.1-1_amd64.deb",
                "git-explain-0.2.1-1.x86_64.rpm",
            ],
        )

        names2 = self.mod.native_package_names("v1.0.0")
        self.assertEqual(
            names2,
            [
                "git-explain_1.0.0-1_amd64.deb",
                "git-explain-1.0.0-1.x86_64.rpm",
            ],
        )

    def test_native_package_names_prerelease_and_build_skipped(self):
        # Plain stable releases only; prereleases and build tags return empty list
        prerelease_tags = [
            "v0.2.1-rc.1",
            "v0.2.1-alpha.2",
            "v1.0.0-beta",
            "v0.2.1+build.123",
            "v0.2.1-rc.1+build.456",
        ]
        for tag in prerelease_tags:
            with self.subTest(tag=tag):
                self.assertEqual(self.mod.native_package_names(tag), [])

    def test_native_package_names_invalid_tags(self):
        invalid_tags = [
            "0.2.1",
            "v0.2",
            "v",
            "",
            "v0.2.1\n",
            " v0.2.1",
            "v0.2.1 ",
            "v0.2.1.0",
        ]
        for tag in invalid_tags:
            with self.subTest(tag=tag):
                with self.assertRaises(ValueError):
                    self.mod.native_package_names(tag)

    def test_missing_exact_musl_binary_rejection_no_fallback(self):
        # No binary exists at target/x86_64-unknown-linux-musl/release/git-explain
        with self.assertRaises(FileNotFoundError) as cm:
            self.mod.package_native(
                repo_root=self.repo_root,
                tag="v0.2.1",
                dist_dir=self.dist_dir,
            )
        err = str(cm.exception)
        self.assertIn("target/x86_64-unknown-linux-musl/release/git-explain", err.replace("\\", "/"))

    def test_prerelease_skips_without_binary_or_commands(self):
        # For non-stable tags, package_native should return [] and do nothing
        with patch("subprocess.run") as mock_run:
            results = self.mod.package_native(
                repo_root=self.repo_root,
                tag="v0.2.1-rc.1",
                dist_dir=self.dist_dir,
            )
            self.assertEqual(results, [])
            mock_run.assert_not_called()

    def test_subprocess_invocation_never_rebuilds_source(self):
        # Create the exact musl binary
        musl_bin_dir = self.repo_root / "target" / "x86_64-unknown-linux-musl" / "release"
        musl_bin_dir.mkdir(parents=True, exist_ok=True)
        musl_bin = musl_bin_dir / "git-explain"
        musl_bin.write_bytes(b"STATIC_MUSL_BINARY_CONTENT")

        expected_deb = (self.dist_dir / "git-explain_0.2.1-1_amd64.deb").resolve()
        expected_rpm = (self.dist_dir / "git-explain-0.2.1-1.x86_64.rpm").resolve()

        def mock_subprocess_run(cmd, *args, **kwargs):
            self.assertEqual(Path(kwargs.get("cwd")).resolve(), self.repo_root.resolve())
            self.assertTrue(kwargs.get("check"), "check=True is required")
            self.assertFalse(kwargs.get("shell", False), "shell=True is forbidden")

            # Simulate tools creating the output files
            if cmd[0] == "cargo" and cmd[1] == "deb":
                # Must have --no-build and --no-strip to never rebuild or mutate
                self.assertIn("--no-build", cmd)
                self.assertIn("--no-strip", cmd)
                self.assertIn("--target", cmd)
                self.assertEqual(cmd[cmd.index("--target") + 1], "x86_64-unknown-linux-musl")
                self.assertIn("--deb-version", cmd)
                self.assertEqual(cmd[cmd.index("--deb-version") + 1], "0.2.1-1")
                self.assertIn("--output", cmd)
                self.assertEqual(Path(cmd[cmd.index("--output") + 1]).resolve(), expected_deb)
                expected_deb.write_bytes(b"MOCK_DEB")
                return MagicMock(returncode=0)
            elif cmd[0] == "cargo" and cmd[1] == "generate-rpm":
                self.assertIn("--target", cmd)
                self.assertEqual(cmd[cmd.index("--target") + 1], "x86_64-unknown-linux-musl")
                self.assertIn("--output", cmd)
                self.assertEqual(Path(cmd[cmd.index("--output") + 1]).resolve(), expected_rpm)
                self.assertIn("--set-metadata", cmd)
                self.assertEqual(cmd[cmd.index("--set-metadata") + 1], 'version = "0.2.1"')
                expected_rpm.write_bytes(b"MOCK_RPM")
                return MagicMock(returncode=0)
            else:
                self.fail(f"Unexpected subprocess command: {cmd}")

        with patch("subprocess.run", side_effect=mock_subprocess_run) as mock_run:
            results = self.mod.package_native(
                repo_root=self.repo_root,
                tag="v0.2.1",
                dist_dir=self.dist_dir,
            )
            self.assertEqual(mock_run.call_count, 2)
            self.assertEqual([p.resolve() for p in results], [expected_deb, expected_rpm])
            self.assertTrue(expected_deb.exists())
            self.assertTrue(expected_rpm.exists())

    def test_cli_execution(self):
        # Create the exact musl binary
        musl_bin_dir = self.repo_root / "target" / "x86_64-unknown-linux-musl" / "release"
        musl_bin_dir.mkdir(parents=True, exist_ok=True)
        (musl_bin_dir / "git-explain").write_bytes(b"STATIC_MUSL_BINARY_CONTENT")

        expected_deb = (self.dist_dir / "git-explain_0.2.1-1_amd64.deb").resolve()
        expected_rpm = (self.dist_dir / "git-explain-0.2.1-1.x86_64.rpm").resolve()

        def mock_subprocess_run(cmd, *args, **kwargs):
            if "deb" in cmd:
                expected_deb.write_bytes(b"DEB")
            elif "generate-rpm" in cmd:
                expected_rpm.write_bytes(b"RPM")
            return MagicMock(returncode=0)

        with patch("subprocess.run", side_effect=mock_subprocess_run):
            with patch.object(
                sys,
                "argv",
                [
                    "package-native.py",
                    "--repo-root",
                    str(self.repo_root),
                    "--tag",
                    "v0.2.1",
                    "--dist-dir",
                    str(self.dist_dir),
                ],
            ):
                self.mod.main()

        self.assertTrue(expected_deb.exists())
        self.assertTrue(expected_rpm.exists())


if __name__ == "__main__":
    unittest.main()
