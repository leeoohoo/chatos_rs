import json
from pathlib import Path
import sys
import tempfile
import unittest

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import run_app_performance_scenario as scenario


class AppPerformanceScenarioTests(unittest.TestCase):
    def valid_result(self, value=1.0):
        return {
            "buildConfiguration": "release", "fixtureTurns": 1_000, "fixtureTodos": 100,
            "markdownBytes": 1_088_018, "codeUTF16Count": 620_000,
            "timelineItemCount": 2_000, "nativeTimelineRows": 1_000,
            "nativeTimelineInitialConstructedRows": 8,
            "nativeTimelineScrolledConstructedRows": 16,
            "megabyteMarkdownBlockCount": 16_001, "largeCodeSpanCount": 0,
            "navigationTransitions": 200, "accountCycles": 20,
            **{key: value for key in scenario.TIMINGS},
        }

    def read(self, records):
        with tempfile.TemporaryDirectory() as directory:
            log = Path(directory) / "test.log"
            log.write_text("".join(
                scenario.MARKER + json.dumps(record) + "\n" for record in records
            ))
            return scenario.measurements(log)

    def test_valid_release_record(self):
        result = self.valid_result()
        self.assertEqual(self.read([result]), result)

    def test_missing_duplicate_debug_and_wrong_fixture_are_rejected(self):
        with self.assertRaisesRegex(ValueError, "Expected one"):
            self.read([])
        with self.assertRaisesRegex(ValueError, "Expected one"):
            self.read([self.valid_result(), self.valid_result()])
        for key, value in (("buildConfiguration", "debug"), ("fixtureTurns", 1),
                           ("largeCodeSpanCount", 1), ("markdownBytes", 1),
                           ("navigationTransitions", 199), ("accountCycles", 19),
                           ("nativeTimelineScrolledConstructedRows", 100)):
            with self.subTest(key=key):
                result = self.valid_result()
                result[key] = value
                with self.assertRaises(ValueError):
                    self.read([result])

    def test_invalid_or_over_budget_timing_is_rejected(self):
        for value in (True, -1, float("nan")):
            result = self.valid_result()
            result[scenario.TIMINGS[0]] = value
            with self.assertRaisesRegex(ValueError, "Invalid"):
                self.read([result])
        result = self.valid_result()
        key = "mainActorMaximumGapMilliseconds"
        result[key] = scenario.CEILINGS[key]
        with self.assertRaisesRegex(ValueError, "exceeded"):
            self.read([result])

    def test_summary_uses_raw_nearest_rank(self):
        values = [self.valid_result(value) for value in (3.0, 1.0, 2.0)]
        summary = scenario.summarize(values)
        for metric in scenario.TIMINGS:
            self.assertEqual(summary[metric]["p50"], 2.0)
            self.assertEqual(summary[metric]["p95"], 3.0)
            self.assertEqual(summary[metric]["samples"], 3)


if __name__ == "__main__":
    unittest.main()
