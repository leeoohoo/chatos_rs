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
        release = self.read(MACOS_SCRIPTS / "package-release-app.sh")
        verify = self.read(MACOS_SCRIPTS / "verify-app-build.sh")

        self.assertIn('BUILD_CONFIGURATION=${CHATOS_BUILD_CONFIGURATION:-}', package)
        self.assertIn('Refusing to package a Release app from a dirty worktree.', package)
        self.assertIn('ChatOSBuildConfiguration', package)
        self.assertIn('ChatOSBuildGitCommit', package)
        self.assertIn('ChatOSBuildDateUTC', package)
        self.assertIn('verify-app-build.sh', package)
        self.assertIn('CHATOS_VERIFY_LAUNCH_SMOKE=1', release)
        self.assertIn('CHATOS_BUILD_SMOKE_TEST=1', verify)
        self.assertIn('CHATOS_BUILD_SMOKE_TEST_EXIT_AFTER_SECONDS=6', verify)
        self.assertIn('did not terminate cleanly after launch smoke test', verify)
        self.assertIn('CFFIXED_USER_HOME=', verify)
        self.assertIn('"$LOCAL_AGENT_HOST" --help', verify)
        self.assertIn("Release ChatOS binary contains a Debug build path", verify)
        self.assertIn("codesign --verify --deep --strict", verify)

        delegate = self.read(
            REPOSITORY_ROOT
            / "clients"
            / "macos"
            / "Sources"
            / "ChatOSApp"
            / "ChatOSApplicationDelegate.swift"
        )
        self.assertIn("BundledAgentSkillLoader.validateCatalog()", delegate)

    def test_debug_build_cannot_overwrite_release_artifact(self) -> None:
        package = self.read(MACOS_SCRIPTS / "package-app.sh")

        self.assertIn('.build/$BUILD_CONFIGURATION/ChatOS.app', package)
        self.assertNotIn('APP_DIR="$PROJECT_DIR/.build/ChatOS.app"', package)

    def test_dmg_checksum_is_portable_between_machines(self) -> None:
        dmg = self.read(MACOS_SCRIPTS / "package-release-dmg.sh")

        self.assertIn('DMG_FILENAME="ChatOS-$VERSION-macOS-$ARCH.dmg"', dmg)
        self.assertIn('cd "$WORK_DIR"', dmg)
        self.assertIn('shasum -a 256 "$DMG_FILENAME"', dmg)
        self.assertNotIn('shasum -a 256 "$DMG_PATH"', dmg)
        self.assertLess(dmg.index('mv -f "$CHECKSUM_WORK_PATH" "$CHECKSUM_PATH"'),
                        dmg.index('mv -f "$DISTRIBUTION_MANIFEST_WORK" "$DISTRIBUTION_MANIFEST"'))

    def test_release_verifier_checks_trust_mount_copy_and_isolated_launch(self) -> None:
        package = self.read(MACOS_SCRIPTS / "package-release-dmg.sh")
        verify = self.read(MACOS_SCRIPTS / "verify-release-dmg.sh")

        self.assertIn("EXPECTED_TEAM_ID=AHVN42RQS3", package)
        self.assertIn("EXPECTED_TEAM_ID=AHVN42RQS3", verify)
        self.assertIn("xcrun stapler validate", verify)
        self.assertIn("spctl --assess --type open", verify)
        self.assertIn("hdiutil attach -readonly -nobrowse", verify)
        self.assertIn('ditto "$MOUNTED_APP" "$INSTALLED_APP"', verify)
        self.assertIn("verify-distribution", verify)
        self.assertIn("CHATOS_BUILD_SMOKE_TEST=1", verify)
        self.assertNotIn("/Applications/ChatOS.app", verify)

    def test_project_creation_explains_directory_semantics(self) -> None:
        sheet = self.read(
            REPOSITORY_ROOT
            / "clients"
            / "macos"
            / "Sources"
            / "ChatOSApp"
            / "Features"
            / "Resources"
            / "CreateProjectSheet.swift"
        )

        self.assertIn("项目根目录：", sheet)
        self.assertIn("当前目录本身会成为项目根目录", sheet)
        self.assertIn("不会自动新建同名文件夹", sheet)


if __name__ == "__main__":
    unittest.main()
