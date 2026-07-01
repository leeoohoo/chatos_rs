#!/usr/bin/env python3
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

import os
import sys
import time

try:
    from pymongo import MongoClient
    from pymongo.errors import OperationFailure, ServerSelectionTimeoutError
except ImportError as exc:
    print(f"[ERROR] missing pymongo: {exc}", file=sys.stderr)
    sys.exit(1)


def require_env(name: str) -> str:
    value = os.environ.get(name, "").strip()
    if not value:
        print(f"[ERROR] missing required env: {name}", file=sys.stderr)
        sys.exit(1)
    return value


def main() -> int:
    host = require_env("LOCAL_MONGO_HOST")
    port = int(require_env("LOCAL_MONGO_PORT"))
    username = require_env("LOCAL_MONGO_ROOT_USERNAME")
    password = require_env("LOCAL_MONGO_ROOT_PASSWORD")
    timeout_ms = int(os.environ.get("LOCAL_MONGO_BOOTSTRAP_TIMEOUT_MS", "30000"))
    deadline = time.time() + (timeout_ms / 1000.0)

    client = MongoClient(host=host, port=port, serverSelectionTimeoutMS=2000)

    while True:
        try:
            client.admin.command("ping")
            break
        except ServerSelectionTimeoutError:
            if time.time() >= deadline:
                print("[ERROR] local Mongo did not become reachable in time", file=sys.stderr)
                return 1
            time.sleep(1)

    admin_db = client["admin"]

    try:
        existing_users = admin_db.command("usersInfo")
    except OperationFailure as exc:
        print(f"[ERROR] failed to inspect admin users: {exc}", file=sys.stderr)
        return 1

    for user in existing_users.get("users", []):
        if user.get("user") == username and user.get("db") == "admin":
            print(f"[INFO] admin user already exists: {username}")
            return 0

    try:
        admin_db.command(
            "createUser",
            username,
            pwd=password,
            roles=[{"role": "root", "db": "admin"}],
        )
    except OperationFailure as exc:
        if "already exists" in str(exc).lower():
            print(f"[INFO] admin user already exists: {username}")
            return 0
        print(f"[ERROR] failed to create admin user: {exc}", file=sys.stderr)
        return 1

    print(f"[INFO] created admin user: {username}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
