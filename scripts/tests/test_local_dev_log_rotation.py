#!/usr/bin/env python3

from pathlib import Path
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]


class LocalDevLogRotationTests(unittest.TestCase):
    def test_service_startup_rotates_instead_of_truncating_logs(self) -> None:
        services = (ROOT / "scripts/local-dev-stack/services.sh").read_text()
        support = (ROOT / "scripts/local-dev-stack/support.sh").read_text()

        self.assertNotIn(': >"$log_file"', services)
        self.assertEqual(services.count('rotate_service_log "$log_file"'), 2)
        self.assertIn('mv -- "$log_file" "$archive"', support)
        self.assertIn('-mtime +7 -delete', support)

    def test_rotation_preserves_the_previous_log(self) -> None:
        support = ROOT / "scripts/local-dev-stack/support.sh"
        with tempfile.TemporaryDirectory() as directory:
            log = Path(directory) / "task-runner.log"
            log.write_text("failure evidence\n")

            subprocess.run(
                [
                    "bash",
                    "-c",
                    'source "$1"; rotate_service_log "$2"',
                    "_",
                    str(support),
                    str(log),
                ],
                check=True,
            )

            self.assertEqual(log.read_text(), "")
            archives = list(log.parent.glob(f"{log.name}.20*T*Z"))
            self.assertEqual(len(archives), 1)
            self.assertEqual(archives[0].read_text(), "failure evidence\n")


if __name__ == "__main__":
    unittest.main()
