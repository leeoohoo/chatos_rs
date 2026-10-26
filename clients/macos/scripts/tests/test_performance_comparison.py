import copy
from pathlib import Path
import sys
import unittest

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import compare_performance_baselines as comparison
from run_performance_baseline import TIMINGS


class PerformanceComparisonTests(unittest.TestCase):
    def manifest(self, value=10.0):
        environment = {
            "collector_architecture": "arm64",
            "hardware": {"available": True, "value": "Mac14,6\n103079215104\n12"},
            "os": {"available": True, "value": "macOS 26.6.2"},
            "power": {"available": True, "value": "Now drawing from 'AC Power'\n100%"},
        }
        measurement = {
            "samples": 9,
            "minimum": value,
            "maximum": value,
            "p50": value,
            "p95": value,
            "p99": value,
        }
        return {
            "status": "completed",
            "source_unchanged": True,
            "performance_acceptance": "not-evaluated",
            "fixture_recipe_sha256": "recipe",
            "fixture_source_sha256": "source",
            "summary_milliseconds": {metric: copy.deepcopy(measurement) for metric in TIMINGS},
            "rounds": [{"environment_start": environment}],
        }

    def test_equal_and_faster_candidate_passes(self):
        baseline = self.manifest()
        candidate = self.manifest(9.0)
        result = comparison.compare(baseline, candidate)
        self.assertEqual(result["status"], "passed")
        self.assertFalse(result["failures"])
        self.assertEqual(len(result["comparisons"]), len(TIMINGS) * 3)

    def test_regressed_percentile_fails_with_machine_readable_evidence(self):
        baseline = self.manifest()
        candidate = self.manifest()
        candidate["summary_milliseconds"][TIMINGS[0]]["p95"] = 12.0
        result = comparison.compare(baseline, candidate)
        self.assertEqual(result["status"], "failed")
        self.assertEqual(result["failures"], [{
            "metric": TIMINGS[0],
            "percentile": "p95",
            "baseline_milliseconds": 10.0,
            "candidate_milliseconds": 12.0,
            "limit_milliseconds": 11.5,
        }])

    def test_absolute_tolerance_handles_small_noisy_measurements(self):
        baseline = self.manifest(0.05)
        candidate = self.manifest(0.25)
        self.assertEqual(comparison.compare(baseline, candidate)["status"], "passed")

    def test_incompatible_or_invalid_manifests_are_rejected(self):
        baseline = self.manifest()
        mutations = [
            ("status", "failed", "completed"),
            ("source_unchanged", False, "unchanged"),
            ("fixture_recipe_sha256", "different", "recipes differ"),
            ("fixture_source_sha256", "different", "Fixture source changed"),
        ]
        for key, value, message in mutations:
            with self.subTest(key=key):
                candidate = self.manifest()
                candidate[key] = value
                with self.assertRaisesRegex(ValueError, message):
                    comparison.compare(baseline, candidate)

        candidate = self.manifest()
        candidate["rounds"][0]["environment_start"]["hardware"]["value"] = "different"
        with self.assertRaisesRegex(ValueError, "Hardware"):
            comparison.compare(baseline, candidate)

        candidate = self.manifest()
        candidate["summary_milliseconds"][TIMINGS[0]]["samples"] = 8
        with self.assertRaisesRegex(ValueError, "Insufficient"):
            comparison.compare(baseline, candidate)


if __name__ == "__main__":
    unittest.main()
