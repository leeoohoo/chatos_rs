from __future__ import annotations

import sqlite3
import threading
import time
from pathlib import Path
from typing import TypedDict

from gateway_base.logging import state_log


class StoredThreadBinding(TypedDict):
    thread_id: str
    instructions_fingerprint: str
    resume_fingerprint: str


class ResponseThreadStore:
    def __init__(self, db_path: str) -> None:
        self._lock = threading.Lock()
        self._db_path = Path(db_path).expanduser()
        self._db_path.parent.mkdir(parents=True, exist_ok=True)
        self._conn = sqlite3.connect(self._db_path, check_same_thread=False)
        self._conn.execute("PRAGMA journal_mode=WAL")
        self._conn.execute("PRAGMA synchronous=NORMAL")
        self._conn.execute(
            """
            CREATE TABLE IF NOT EXISTS response_threads (
                response_id TEXT PRIMARY KEY,
                thread_id TEXT NOT NULL,
                instructions_fingerprint TEXT NOT NULL DEFAULT '',
                resume_fingerprint TEXT NOT NULL DEFAULT '',
                updated_at INTEGER NOT NULL
            )
            """
        )
        columns = {
            row[1]
            for row in self._conn.execute("PRAGMA table_info(response_threads)").fetchall()
        }
        if "instructions_fingerprint" not in columns:
            self._conn.execute(
                """
                ALTER TABLE response_threads
                ADD COLUMN instructions_fingerprint TEXT NOT NULL DEFAULT ''
                """
            )
        if "resume_fingerprint" not in columns:
            self._conn.execute(
                """
                ALTER TABLE response_threads
                ADD COLUMN resume_fingerprint TEXT NOT NULL DEFAULT ''
                """
            )
        self._conn.commit()
        count_row = self._conn.execute(
            "SELECT COUNT(*) FROM response_threads"
        ).fetchone()
        state_log(
            "db.ready",
            f"path={self._db_path}",
            f"entries={count_row[0] if count_row else 0}",
        )

    def put(
        self,
        response_id: str,
        thread_id: str,
        instructions_fingerprint: str = "",
        resume_fingerprint: str = "",
    ) -> None:
        with self._lock:
            self._conn.execute(
                """
                INSERT INTO response_threads (
                    response_id,
                    thread_id,
                    instructions_fingerprint,
                    resume_fingerprint,
                    updated_at
                )
                VALUES (?, ?, ?, ?, ?)
                ON CONFLICT(response_id) DO UPDATE SET
                    thread_id = excluded.thread_id,
                    instructions_fingerprint = excluded.instructions_fingerprint,
                    resume_fingerprint = excluded.resume_fingerprint,
                    updated_at = excluded.updated_at
                """,
                (
                    response_id,
                    thread_id,
                    instructions_fingerprint,
                    resume_fingerprint,
                    int(time.time()),
                ),
            )
            self._conn.commit()
        state_log(
            "map.put",
            f"response_id={response_id}",
            f"thread_id={thread_id}",
            f"instructions_fp={instructions_fingerprint or 'none'}",
            f"resume_fp={resume_fingerprint or 'none'}",
        )

    def get_thread(self, response_id: str) -> str | None:
        binding = self.get_thread_binding(response_id)
        return binding["thread_id"] if binding else None

    def get_thread_binding(self, response_id: str) -> StoredThreadBinding | None:
        with self._lock:
            row = self._conn.execute(
                """
                SELECT thread_id, instructions_fingerprint, resume_fingerprint
                FROM response_threads
                WHERE response_id = ?
                """,
                (response_id,),
            ).fetchone()
        binding = (
            StoredThreadBinding(
                thread_id=row[0],
                instructions_fingerprint=row[1] or "",
                resume_fingerprint=row[2] or "",
            )
            if row
            else None
        )
        state_log(
            "map.lookup",
            f"response_id={response_id}",
            f"hit={'yes' if binding else 'no'}",
        )
        return binding

    def close(self) -> None:
        with self._lock:
            self._conn.close()
