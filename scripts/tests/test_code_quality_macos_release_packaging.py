# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

from __future__ import annotations

import unittest
from pathlib import Path


REPOSITORY_ROOT = Path(__file__).resolve().parents[2]
MACOS_SCRIPTS = REPOSITORY_ROOT / "clients" / "macos" / "scripts"


class MacOSReleasePackagingPolicyTests(unittest.TestCase):
    def read(self, path: Path) -> str:
        return path.read_text(encoding="utf-8")

    def test_deployment_and_dmg_use_the_release_entry_point(self) -> None:
        deploy = self.read(REPOSITORY_ROOT / "scripts" / "deploy-online.sh")
        dmg = self.read(MACOS_SCRIPTS / "package-release-dmg.sh")

        self.assertIn('clients/macos/scripts/package-release-app.sh', deploy)
        self.assertNotIn('clients/macos/scripts/package-debug-app.sh', deploy)
        self.assertIn('"$SCRIPT_DIR/package-release-app.sh"', dmg)

    def test_debug_and_release_entry_points_are_explicit(self) -> None:
        debug = self.read(MACOS_SCRIPTS / "package-debug-app.sh")
        release = self.read(MACOS_SCRIPTS / "package-release-app.sh")

        self.assertIn('CHATOS_BUILD_CONFIGURATION=debug', debug)
        self.assertIn('CHATOS_BUILD_CONFIGURATION=release', release)
        self.assertIn('"$SCRIPT_DIR/package-app.sh"', debug)
        self.assertIn('"$SCRIPT_DIR/package-app.sh"', release)

    def test_generic_packager_embeds_and_verifies_build_identity(self) -> None:
        package = self.read(MACOS_SCRIPTS / "package-app.sh")
        verify = self.read(MACOS_SCRIPTS / "verify-app-build.sh")

        self.assertIn('BUILD_CONFIGURATION=${CHATOS_BUILD_CONFIGURATION:-}', package)
        self.assertIn('Refusing to package a Release app from a dirty worktree.', package)
        self.assertIn('ChatOSBuildConfiguration', package)
        self.assertIn('ChatOSBuildGitCommit', package)
        self.assertIn('ChatOSBuildDateUTC', package)
        self.assertIn('verify-app-build.sh', package)
        self.assertIn("Release ChatOS binary contains a Debug build path", verify)
        self.assertIn("codesign --verify --deep --strict", verify)


if __name__ == "__main__":
    unittest.main()
