#!/usr/bin/env python3

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PATTERN = re.compile(r"\b(?:unwrap|expect)\s*\(")

TARGET_ROOTS = [
    ROOT / "chat_app_server_rs" / "src",
    ROOT / "memory_server" / "backend" / "src",
]

ALLOWLIST = {
    ROOT
    / "chat_app_server_rs"
    / "src"
    / "services"
    / "v3"
    / "ai_client"
    / "test_support.rs",
}


def should_skip(path: Path) -> bool:
    if path in ALLOWLIST:
        return True
    if "tests" in path.name:
        return True
    if any(part == "tests" for part in path.parts):
        return True
    return False


def scan_file(path: Path) -> list[tuple[int, str]]:
    text = path.read_text()
    head = text.split("#[cfg(test)]", 1)[0]
    hits: list[tuple[int, str]] = []
    for lineno, line in enumerate(head.splitlines(), 1):
        if PATTERN.search(line):
            hits.append((lineno, line.strip()))
    return hits


def main() -> int:
    failures: list[tuple[Path, list[tuple[int, str]]]] = []
    for root in TARGET_ROOTS:
        for path in sorted(root.rglob("*.rs")):
            if should_skip(path):
                continue
            hits = scan_file(path)
            if hits:
                failures.append((path, hits))

    if not failures:
        print("No unwrap/expect found in non-test Rust code.")
        return 0

    print("Found unwrap/expect in non-test Rust code:")
    for path, hits in failures:
        print(path.relative_to(ROOT))
        for lineno, line in hits:
            print(f"  {lineno}: {line}")
    return 1


if __name__ == "__main__":
    sys.exit(main())
