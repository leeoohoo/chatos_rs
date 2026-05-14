#!/usr/bin/env python3
from __future__ import annotations

import sys
import tempfile
import unittest
from pathlib import Path


GATEWAY_ROOT = Path(__file__).resolve().parents[2]
if str(GATEWAY_ROOT) not in sys.path:
    sys.path.insert(0, str(GATEWAY_ROOT))

from gateway_core.state_store import ResponseThreadStore  # noqa: E402


class GatewayStateStoreTest(unittest.TestCase):
    def test_put_and_get_thread_binding(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            db_path = str(Path(tmpdir) / "gateway.sqlite3")
            store = ResponseThreadStore(db_path)
            try:
                store.put("resp_1", "thread_1", "fp_123")

                self.assertEqual(store.get_thread("resp_1"), "thread_1")
                self.assertEqual(
                    store.get_thread_binding("resp_1"),
                    {
                        "thread_id": "thread_1",
                        "instructions_fingerprint": "fp_123",
                        "resume_fingerprint": "",
                    },
                )
            finally:
                store.close()


if __name__ == "__main__":
    unittest.main()
