import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import run_process_group_soak as soak


class ProcessGroupSoakTests(unittest.TestCase):
    def test_descendants_are_scoped_to_root(self):
        table = {
            1: {"pid": 1, "parent_pid": 0, "command": "launchd"},
            10: {"pid": 10, "parent_pid": 1, "command": "ChatOS"},
            11: {"pid": 11, "parent_pid": 10, "command": "Host"},
            12: {"pid": 12, "parent_pid": 11, "command": "Worker"},
            20: {"pid": 20, "parent_pid": 1, "command": "unrelated"},
        }
        self.assertEqual([item["pid"] for item in soak.descendant_processes(10, table)], [10, 11, 12])

    def test_summary_uses_raw_nearest_rank_and_detects_monotonic_growth(self):
        samples = []
        for index, footprint in enumerate((100, 300, 200)):
            totals = {key: index + 1 for key in (
                "process_count", "fd_count", "kernel_thread_count", "cpu_percent",
                "disk_read_delta_bytes", "disk_write_delta_bytes", "temporary_file_count",
                "temporary_directory_count", "temporary_bytes",
            )}
            totals.update({"physical_footprint_bytes": footprint, "resident_size_bytes": footprint // 2})
            samples.append({"totals": totals})
        result = soak.summarize(samples)
        self.assertEqual(result["physical_footprint_bytes"]["p95"], 300)
        self.assertTrue(result["fd_count"]["monotonic_growth"])
        self.assertFalse(result["physical_footprint_bytes"]["monotonic_growth"])

    def test_temporary_scan_is_bounded_and_never_follows_symlinks(self):
        with tempfile.TemporaryDirectory() as directory, tempfile.TemporaryDirectory() as outside:
            root = Path(directory)
            (root / "nested").mkdir()
            (root / "nested/file").write_bytes(b"1234")
            (Path(outside) / "large").write_bytes(b"x" * 10_000)
            (root / "escape").symlink_to(outside)
            result = soak.scan_temporary_roots([root])
            self.assertEqual(result, {"temporary_file_count": 2,
                                      "temporary_directory_count": 1, "temporary_bytes": 4})
            with self.assertRaisesRegex(ValueError, "bounded"):
                soak.scan_temporary_roots([root], maximum_entries=1)

    def test_formal_acceptance_rejects_short_or_unbounded_runs(self):
        class Arguments:
            evaluate_acceptance = True
            duration_seconds = 60
            sample_interval_seconds = 5
            minimum_samples = 5_000
            require_process = ["ChatOSAgentWorker"]
            temporary_root = [Path("/tmp/isolated")]
            max_group_physical_footprint_bytes = 1
            max_cpu_percent_p95 = 1
            max_disk_write_bytes_per_interval = 1

        with self.assertRaisesRegex(ValueError, "8 hours"):
            soak.validate_formal_acceptance(Arguments())

    def test_formal_acceptance_requires_manifest_bound_app(self):
        class Arguments:
            evaluate_acceptance = True
            duration_seconds = soak.FORMAL_DURATION_SECONDS
            sample_interval_seconds = soak.FORMAL_MAXIMUM_INTERVAL_SECONDS
            minimum_samples = soak.FORMAL_MINIMUM_SAMPLES
            require_process = ["ChatOSSwift"]
            temporary_root = [Path("/tmp/isolated")]
            app = None
            build_manifest = None
            max_group_physical_footprint_bytes = 1
            max_cpu_percent_p95 = 1
            max_disk_write_bytes_per_interval = 1

        with self.assertRaisesRegex(ValueError, "build manifest"):
            soak.validate_formal_acceptance(Arguments())

    def test_repository_output_must_be_ignored(self):
        with tempfile.TemporaryDirectory() as directory:
            repository = Path(directory)
            output = repository / "tracked-results"
            with patch.object(soak.subprocess, "run") as command:
                command.return_value.returncode = 1
                with self.assertRaisesRegex(ValueError, "must be ignored"):
                    soak.validate_output_location(repository, output)

    def test_output_must_not_be_inside_scanned_temporary_root(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "isolated-runtime"
            with self.assertRaisesRegex(ValueError, "collector's own files"):
                soak.validate_output_is_not_scanned(root / "soak-output", [root])
            soak.validate_output_is_not_scanned(
                Path(directory) / "evidence" / "soak-output",
                [root],
            )

    def test_acceptance_reports_missing_process_limits_and_growth(self):
        class Arguments:
            minimum_samples = 3
            require_process = ["Worker"]
            required_process_minimum_samples = 2
            max_group_physical_footprint_bytes = 100
            max_cpu_percent_p95 = 50
            max_disk_write_bytes_per_interval = 100

        summary = {
            "physical_footprint_bytes": {"p95": 101},
            "cpu_percent": {"p95": 51},
            "disk_write_delta_bytes": {"maximum": 101},
            "fd_count": {"monotonic_growth": True},
            "kernel_thread_count": {"monotonic_growth": False},
            "temporary_file_count": {"monotonic_growth": False},
            "temporary_bytes": {"monotonic_growth": False},
        }
        failures = soak.acceptance_failures(Arguments(), summary, {"Worker": 1}, 2)
        self.assertEqual(len(failures), 6)


if __name__ == "__main__":
    unittest.main()
