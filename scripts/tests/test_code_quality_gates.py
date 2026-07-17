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
from check_source_size_policy import AllowlistEntry, evaluate_source_sizes
from code_quality_common import is_production_source, parse_unified_diff_added_lines


class CodeQualityCommonTests(unittest.TestCase):
    def test_production_scope_excludes_test_and_generated_files(self) -> None:
        self.assertTrue(is_production_source("service/src/api.rs"))
        self.assertTrue(is_production_source("frontend/src/App.tsx"))
        self.assertFalse(is_production_source("service/src/api/tests.rs"))
        self.assertFalse(is_production_source("frontend/src/App.test.tsx"))
        self.assertFalse(is_production_source("service/tests/integration.rs"))
        self.assertFalse(is_production_source("frontend/dist/index.js"))

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
