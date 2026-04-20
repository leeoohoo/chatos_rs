from __future__ import annotations

import sqlite3
import threading
import time
from pathlib import Path

from gateway_base.logging import state_log


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
                updated_at INTEGER NOT NULL
            )
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

    def put(self, response_id: str, thread_id: str) -> None:
        with self._lock:
            self._conn.execute(
                """
                INSERT INTO response_threads (response_id, thread_id, updated_at)
                VALUES (?, ?, ?)
                ON CONFLICT(response_id) DO UPDATE SET
                    thread_id = excluded.thread_id,
                    updated_at = excluded.updated_at
                """,
                (response_id, thread_id, int(time.time())),
            )
            self._conn.commit()
        state_log(
            "map.put",
            f"response_id={response_id}",
            f"thread_id={thread_id}",
        )

    def get_thread(self, response_id: str) -> str | None:
        with self._lock:
            row = self._conn.execute(
                "SELECT thread_id FROM response_threads WHERE response_id = ?",
                (response_id,),
            ).fetchone()
        thread_id = row[0] if row else None
        state_log(
            "map.lookup",
            f"response_id={response_id}",
            f"hit={'yes' if thread_id else 'no'}",
        )
        return thread_id

    def close(self) -> None:
        with self._lock:
            self._conn.close()
