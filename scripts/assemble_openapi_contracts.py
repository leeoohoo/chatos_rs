#!/usr/bin/env python3
from __future__ import annotations

import argparse
import sys
from pathlib import Path
from typing import Dict, List, Tuple

import yaml


ROOT = Path(__file__).resolve().parent.parent
CONTRACT_DIR = ROOT / ".github" / "api-contract"
FRAGMENTS_DIR = CONTRACT_DIR / "fragments"

SERVICES: Dict[str, Dict[str, Path]] = {
    "chat_app_server_rs": {
        "fragment_dir": FRAGMENTS_DIR / "chat_app_server_rs",
        "output_file": CONTRACT_DIR / "chat_app_server_rs.openapi.yaml",
    },
    "memory_server": {
        "fragment_dir": FRAGMENTS_DIR / "memory_server",
        "output_file": CONTRACT_DIR / "memory_server.openapi.yaml",
    },
}


def load_yaml(path: Path) -> dict:
    with path.open("r", encoding="utf-8") as f:
        data = yaml.safe_load(f)
    if data is None:
        return {}
    if not isinstance(data, dict):
        raise ValueError(f"YAML root must be mapping: {path}")
    return data


def dump_yaml(data: dict) -> str:
    return yaml.safe_dump(data, sort_keys=False, allow_unicode=False)


def collect_fragment_files(fragment_dir: Path) -> Tuple[Path, List[Path]]:
    meta_file = fragment_dir / "_meta.yaml"
    if not meta_file.exists():
        raise FileNotFoundError(f"Missing fragment meta file: {meta_file}")

    files = [
        p
        for p in sorted(fragment_dir.glob("*.yaml"))
        if p.name != "_meta.yaml"
    ]
    if not files:
        raise FileNotFoundError(f"No fragment files found in: {fragment_dir}")

    return meta_file, files


def merge_service(fragment_dir: Path) -> dict:
    meta_file, fragment_files = collect_fragment_files(fragment_dir)
    merged = load_yaml(meta_file)

    merged_paths = merged.get("paths")
    if merged_paths is None:
        merged_paths = {}
    if not isinstance(merged_paths, dict):
        raise ValueError(f"`paths` in meta must be mapping: {meta_file}")
    merged["paths"] = merged_paths

    seen_paths: Dict[str, Path] = {}

    for file in fragment_files:
        fragment = load_yaml(file)
        fragment_paths = fragment.get("paths")
        if fragment_paths is None:
            continue
        if not isinstance(fragment_paths, dict):
            raise ValueError(f"`paths` in fragment must be mapping: {file}")

        for path_key, path_item in fragment_paths.items():
            if path_key in merged_paths:
                prev = seen_paths.get(path_key, meta_file)
                raise ValueError(
                    f"Duplicate path `{path_key}` in fragments: {prev} and {file}"
                )
            merged_paths[path_key] = path_item
            seen_paths[path_key] = file

    return merged


def assemble_all() -> Dict[str, dict]:
    assembled: Dict[str, dict] = {}
    for service, cfg in SERVICES.items():
        assembled[service] = merge_service(cfg["fragment_dir"])
    return assembled


def check_mode(assembled: Dict[str, dict]) -> int:
    mismatches: List[str] = []

    for service, spec in assembled.items():
        output_file = SERVICES[service]["output_file"]
        expected = dump_yaml(spec)
        if not output_file.exists():
            mismatches.append(f"missing output file: {output_file}")
            continue
        actual = output_file.read_text(encoding="utf-8")
        if actual != expected:
            mismatches.append(f"drift detected: {output_file}")

    if mismatches:
        print("[ERROR] OpenAPI assembly check failed:")
        for m in mismatches:
            print(f"  - {m}")
        print("[INFO] Run: python3 scripts/assemble_openapi_contracts.py --write")
        return 1

    print("[OK] OpenAPI assembly check passed.")
    return 0


def write_mode(assembled: Dict[str, dict]) -> int:
    for service, spec in assembled.items():
        output_file = SERVICES[service]["output_file"]
        output_file.write_text(dump_yaml(spec), encoding="utf-8")
        print(f"[OK] Wrote assembled OpenAPI: {output_file}")
    return 0


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Assemble OpenAPI specs from fragments.")
    parser.add_argument(
        "--check",
        action="store_true",
        help="Fail when assembled output differs from tracked OpenAPI files.",
    )
    parser.add_argument(
        "--write",
        action="store_true",
        help="Write assembled output to tracked OpenAPI files.",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if args.check and args.write:
        print("[ERROR] Choose only one mode: --check or --write")
        return 2

    mode_check = args.check or (not args.write)
    mode_write = args.write

    assembled = assemble_all()
    if mode_write:
        return write_mode(assembled)
    if mode_check:
        return check_mode(assembled)
    return 0


if __name__ == "__main__":
    sys.exit(main())
