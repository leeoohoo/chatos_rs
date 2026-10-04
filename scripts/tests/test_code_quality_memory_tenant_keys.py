# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

from pathlib import Path
import unittest


ROOT = Path(__file__).resolve().parents[2]
MEMORY = ROOT / "memory_engine/backend"


class MemoryTenantKeyContractTests(unittest.TestCase):
    def test_migration_scopes_tenant_resource_primary_and_foreign_keys(self) -> None:
        migration = (
            MEMORY
            / "migrations/postgres/0006_tenant_scoped_resource_keys.sql"
        ).read_text()
        for table in (
            "engine_subjects",
            "engine_subject_memory_scopes",
            "engine_subject_memories",
            "engine_threads",
            "engine_records",
            "engine_compact_turns",
            "engine_summaries",
            "engine_thread_snapshots",
        ):
            with self.subTest(table=table):
                self.assertIn(
                    f"ALTER TABLE {table}\n    ADD PRIMARY KEY (tenant_id, source_id, id);",
                    migration,
                )
        self.assertEqual(
            migration.count("FOREIGN KEY (tenant_id, source_id, thread_id)"),
            4,
        )
        self.assertEqual(
            migration.count(
                "REFERENCES engine_threads (tenant_id, source_id, id) ON DELETE CASCADE"
            ),
            4,
        )

    def test_repository_writes_and_worker_claims_use_composite_identity(self) -> None:
        threads = (MEMORY / "src/repositories/threads/writes.rs").read_text()
        records = (MEMORY / "src/repositories/records/writes.rs").read_text()
        summaries = (MEMORY / "src/repositories/summaries/writes.rs").read_text()
        self.assertIn("ON CONFLICT(tenant_id,source_id,id)", threads)
        self.assertIn("ON CONFLICT(tenant_id,source_id,id)", records)
        self.assertIn("ON CONFLICT(tenant_id,source_id,id)", summaries)

        for relative in (
            "src/repositories/subject_memory_scopes.rs",
            "src/repositories/summaries/dispatch.rs",
            "src/repositories/summaries/subject_dispatch.rs",
        ):
            source = (MEMORY / relative).read_text()
            with self.subTest(file=relative):
                self.assertIn("SELECT tenant_id,source_id,id", source)
                self.assertIn("s.tenant_id=c.tenant_id", source)
                self.assertIn("s.source_id=c.source_id", source)
                self.assertIn("s.id=c.id", source)


if __name__ == "__main__":
    unittest.main()
