# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

from __future__ import annotations

import sys
import unittest
from datetime import date
from pathlib import Path


SCRIPTS_DIR = Path(__file__).resolve().parents[1]
if str(SCRIPTS_DIR) not in sys.path:
    sys.path.insert(0, str(SCRIPTS_DIR))

from check_new_code_clones import build_source_sequence, find_clone_violations
from check_source_size_policy import (
    AllowlistEntry,
    collect_oversized_sources,
    evaluate_source_sizes,
)
from code_quality_common import (
    is_owned_source,
    is_production_source,
    parse_unified_diff_added_lines,
)


class CodeQualityCommonTests(unittest.TestCase):
    def test_production_scope_excludes_test_and_generated_files(self) -> None:
        self.assertTrue(is_production_source("service/src/api.rs"))
        self.assertTrue(is_production_source("frontend/src/App.tsx"))
        self.assertTrue(is_production_source("clients/windows/src/App.xaml"))
        self.assertTrue(is_production_source("clients/windows/src/ViewModel.cs"))
        self.assertTrue(is_production_source("clients/macos/Sources/AppView.swift"))
        self.assertFalse(is_production_source("service/src/api/tests.rs"))
        self.assertFalse(is_production_source("frontend/src/App.test.tsx"))
        self.assertFalse(is_production_source("clients/windows/tests/ViewModelTests.cs"))
        self.assertFalse(is_production_source("clients/windows/obj/App.g.cs"))
        self.assertFalse(is_production_source("clients/windows/bin/App.xaml"))
        self.assertFalse(is_production_source("clients/macos/.build/AppView.swift"))
        self.assertFalse(is_production_source("service/tests/integration.rs"))
        self.assertFalse(is_production_source("frontend/dist/index.js"))
        self.assertFalse(is_production_source("frontend/src/icons.generated.ts"))
        self.assertFalse(is_production_source("frontend/src/schema.GENERATED.tsx"))

    def test_owned_scope_includes_tests_but_excludes_generated_and_external_files(self) -> None:
        self.assertTrue(is_owned_source("service/src/api/tests.rs"))
        self.assertTrue(is_owned_source("clients/macos/Tests/AppTests.swift"))
        self.assertTrue(is_owned_source("plugins/studio/test/schema.test.mjs"))
        self.assertFalse(is_owned_source("frontend/src/schema.generated.ts"))
        self.assertFalse(is_owned_source("frontend/node_modules/library/index.js"))
        self.assertFalse(is_owned_source("service/fixtures/large_fixture.rs"))

    def test_unified_diff_parser_tracks_only_added_head_lines(self) -> None:
        diff = """diff --git a/src/app.rs b/src/app.rs
--- a/src/app.rs
+++ b/src/app.rs
@@ -2,0 +3,2 @@
+first
+second
@@ -8,1 +10,0 @@
-removed
"""
        self.assertEqual(parse_unified_diff_added_lines(diff), {"src/app.rs": {3, 4}})


class SourceSizePolicyTests(unittest.TestCase):
    def test_owned_baseline_blocks_new_oversized_files_and_existing_growth(self) -> None:
        allowlist = {
            "legacy.rs": AllowlistEntry(
                path="legacy.rs",
                max_lines=800,
                expires_on=date(2026, 12, 31),
                reason="scheduled split",
            )
        }
        _, errors = evaluate_source_sizes(
            {"legacy.rs": 801, "new.swift": 800},
            allowlist,
            warn_lines=500,
            hard_lines=800,
            today=date(2026, 9, 26),
            warning_paths={"new.swift"},
            hard_limit_inclusive=True,
        )
        self.assertEqual(
            errors,
            [
                "legacy.rs: 801 lines exceeds allowlist budget 800",
                "new.swift: 800 lines reaches hard limit 800 without an allowlist entry",
            ],
        )

    def test_owned_scope_is_stably_sorted_and_treats_800_lines_as_oversized(self) -> None:
        line_counts = {"z_test.rs": 800, "largest.swift": 1200, "small.ts": 799}
        self.assertEqual(
            collect_oversized_sources(line_counts, hard_lines=800, inclusive=True),
            [("largest.swift", 1200), ("z_test.rs", 800)],
        )
        _, errors = evaluate_source_sizes(
            line_counts,
            {},
            warn_lines=500,
            hard_lines=800,
            today=date(2026, 9, 26),
            warning_paths=set(),
            hard_limit_inclusive=True,
        )
        self.assertEqual(
            errors,
            [
                "largest.swift: 1200 lines reaches hard limit 800 without an allowlist entry",
                "z_test.rs: 800 lines reaches hard limit 800 without an allowlist entry",
            ],
        )

    def test_only_new_files_warn_and_valid_allowlist_covers_hard_limit(self) -> None:
        allowlist = {
            "legacy.rs": AllowlistEntry(
                path="legacy.rs",
                max_lines=820,
                expires_on=date(2026, 12, 31),
                reason="scheduled split",
            )
        }
        warnings, errors = evaluate_source_sizes(
            {"legacy.rs": 810, "new.ts": 510},
            allowlist,
            warn_lines=500,
            hard_lines=800,
            today=date(2026, 7, 17),
            warning_paths={"new.ts"},
        )
        self.assertEqual(warnings, ["new.ts: 510 lines (warning threshold 500)"])
        self.assertEqual(errors, [])

    def test_expired_and_stale_allowlist_entries_fail(self) -> None:
        allowlist = {
            "expired.rs": AllowlistEntry(
                path="expired.rs",
                max_lines=900,
                expires_on=date(2026, 7, 16),
                reason="expired split",
            ),
            "stale.rs": AllowlistEntry(
                path="stale.rs",
                max_lines=900,
                expires_on=date(2026, 12, 31),
                reason="already split",
            ),
        }
        _, errors = evaluate_source_sizes(
            {"expired.rs": 850, "stale.rs": 700},
            allowlist,
            warn_lines=500,
            hard_lines=800,
            today=date(2026, 7, 17),
            warning_paths=set(),
        )
        self.assertIn("expired.rs: allowlist expired on 2026-07-16", errors)
        self.assertTrue(any(error.startswith("stale.rs: stale allowlist entry") for error in errors))


class NewClonePolicyTests(unittest.TestCase):
    def test_finds_clone_only_when_duplicate_touches_added_lines(self) -> None:
        block = [f"let value_{index} = source_{index};" for index in range(30)]
        sequences = {
            "existing.rs": build_source_sequence("existing.rs", block),
            "new.rs": build_source_sequence("new.rs", block),
        }
        violations = find_clone_violations(
            sequences,
            {"new.rs": set(range(1, 31))},
            minimum_lines=25,
        )
        self.assertEqual(len(violations), 1)
        self.assertEqual(violations[0].significant_lines, 30)
        self.assertEqual(
            find_clone_violations(sequences, {}, minimum_lines=25),
            [],
        )


if __name__ == "__main__":
    unittest.main()
