import os
import pathlib
import stat
import subprocess
import tempfile
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts" / "postgres-verify-user-import.sh"


class PostgresVerifyUserImportTests(unittest.TestCase):
    def run_verifier(self, users: int) -> subprocess.CompletedProcess[str]:
        with tempfile.TemporaryDirectory() as temporary:
            fake_psql = pathlib.Path(temporary) / "psql"
            fake_psql.write_text(
                f"#!/bin/sh\nprintf '%s\\n' '{users}|0|1|0'\n",
                encoding="utf-8",
            )
            fake_psql.chmod(fake_psql.stat().st_mode | stat.S_IXUSR)
            environment = os.environ.copy()
            environment.update(
                {
                    "PATH": f"{temporary}:{environment['PATH']}",
                    "CHATOS_BOOTSTRAP_FILE": str(pathlib.Path(temporary) / "missing.conf"),
                    "EXPECTED_MIGRATED_USER_COUNT": "4",
                    "EXPECTED_MIGRATED_WECHAT_IDENTITY_COUNT": "1",
                    "POSTGRES_ADMIN_PASSWORD": "test-only",
                }
            )
            return subprocess.run(
                ["bash", str(SCRIPT)],
                cwd=ROOT,
                env=environment,
                text=True,
                capture_output=True,
                check=False,
            )

    def test_allows_users_created_after_the_migration_baseline(self) -> None:
        result = self.run_verifier(5)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("User migration verified: 5 user(s)", result.stdout)

    def test_rejects_loss_below_the_migration_baseline(self) -> None:
        result = self.run_verifier(3)
        self.assertEqual(result.returncode, 1)
        self.assertIn("expected at least 4", result.stderr)


if __name__ == "__main__":
    unittest.main()
