#!/usr/bin/python3
"""Run the opt-in Release App CPU scenario on synthetic in-memory fixtures."""

import argparse
import json
import math
import os
from pathlib import Path
import tempfile
import time

from build_evidence import (assert_unchanged, atomic_json, capture, digest, environment,
                            file_hash, native_command, source_snapshot, utc_now)
from run_performance_baseline import execute, test_product


MARKER = "CHATOS_APP_SCENARIO_BASELINE "
TEST = "AppPerformanceScenarioBaselineTests.testFixedCompositeScenario"
FIXTURE = {"turns": 1_000, "todos": 100, "minimum_markdown_bytes": 1_000_000,
           "minimum_code_utf16": 600_000, "timeline_items": 2_000}
TIMINGS = (
    "timeline1000BuildMilliseconds", "todo100ProjectionMilliseconds",
    "megabyteMarkdownParseMilliseconds", "largeCodeFallbackMilliseconds",
    "mainActorMaximumGapMilliseconds", "concurrentWallMilliseconds",
)
CEILINGS = {
    "timeline1000BuildMilliseconds": 100,
    "todo100ProjectionMilliseconds": 100,
    "megabyteMarkdownParseMilliseconds": 2_000,
    "largeCodeFallbackMilliseconds": 100,
    "mainActorMaximumGapMilliseconds": 100,
    "concurrentWallMilliseconds": 2_500,
}


def measurements(log):
    records = []
    with Path(log).open(errors="replace") as stream:
        for line in stream:
            if MARKER in line:
                records.append(json.loads(line.split(MARKER, 1)[1]))
    if len(records) != 1:
        raise ValueError(f"Expected one App scenario record, got {len(records)}")
    result = records[0]
    required = {
        "buildConfiguration", "fixtureTurns", "fixtureTodos", "markdownBytes",
        "codeUTF16Count", "timelineItemCount", "megabyteMarkdownBlockCount",
        "largeCodeSpanCount", *TIMINGS,
    }
    if not isinstance(result, dict) or not required.issubset(result):
        raise ValueError("Malformed/incomplete App scenario record")
    if result["buildConfiguration"] != "release":
        raise ValueError("Expected an optimized Release App scenario")
    expected = {"fixtureTurns": FIXTURE["turns"], "fixtureTodos": FIXTURE["todos"],
                "timelineItemCount": FIXTURE["timeline_items"], "largeCodeSpanCount": 0}
    for key, value in expected.items():
        if result[key] != value or type(result[key]) is not int:
            raise ValueError(f"Unexpected App scenario fixture/result: {key}")
    if type(result["markdownBytes"]) is not int \
            or result["markdownBytes"] <= FIXTURE["minimum_markdown_bytes"]:
        raise ValueError("Markdown fixture is too small")
    if type(result["codeUTF16Count"]) is not int \
            or result["codeUTF16Count"] <= FIXTURE["minimum_code_utf16"]:
        raise ValueError("Code fixture is too small")
    if type(result["megabyteMarkdownBlockCount"]) is not int \
            or result["megabyteMarkdownBlockCount"] < 1:
        raise ValueError("Markdown scenario produced no blocks")
    for key in TIMINGS:
        value = result[key]
        if isinstance(value, bool) or not isinstance(value, (int, float)) \
                or not math.isfinite(value) or value < 0:
            raise ValueError(f"Invalid App scenario timing: {key}")
        if value >= CEILINGS[key]:
            raise ValueError(f"App scenario exceeded acceptance ceiling: {key}")
    return result


def summarize(rounds):
    summary = {}
    for key in TIMINGS:
        values = sorted(float(result[key]) for result in rounds)
        summary[key] = {
            "samples": len(values), "minimum": values[0], "maximum": values[-1],
            "p50": values[math.ceil(len(values) * 0.50) - 1],
            "p95": values[math.ceil(len(values) * 0.95) - 1],
            "p99": values[math.ceil(len(values) * 0.99) - 1], "ceiling": CEILINGS[key],
        }
    return summary


def run_scenario(repository, output, rounds, timeout):
    repository, output = Path(repository).resolve(), Path(output).resolve()
    try:
        relative = output.relative_to(repository)
    except ValueError:
        pass
    else:
        import subprocess
        if subprocess.run(["git", "check-ignore", "-q", "--", str(relative)],
                          cwd=repository).returncode != 0:
            raise ValueError("Output inside repository must be ignored")
    output.mkdir(parents=True, exist_ok=False)
    started = capture(repository, "release", "native")
    started["profiles"]["rust"] = None
    atomic_json(output / "inputs.json", started)
    package = repository / "clients/macos"
    command = native_command([
        "swift", "test", "--arch", "arm64", "--package-path", str(package),
        "--build-system", "native", "--scratch-path", str(package / ".build-native"),
        "--force-resolved-versions", "-c", "release", "--filter", TEST,
    ])
    environment_values = dict(os.environ, CHATOS_RUN_APP_SCENARIO_BASELINE="1")
    results, run_records = [], []
    manifest = {
        "format_version": 1, "kind": "engineering-app-cpu-scenario", "status": "running",
        "started_utc": started["started_utc"],
        "inputs_manifest_sha256": file_hash(output / "inputs.json"),
        "source_input_sha256": started["source"]["input_sha256"], "command": command,
        "environment_overrides": {"CHATOS_RUN_APP_SCENARIO_BASELINE": "1"},
        "fixture_recipe": FIXTURE, "fixture_recipe_sha256": digest(FIXTURE),
        "fixture_source_sha256": file_hash(
            package / "Tests/ChatOSAppTests/AppPerformanceScenarioBaselineTests.swift"
        ),
        "ceilings_milliseconds": CEILINGS,
        "limitations": [
            "Synthetic in-memory CPU scenario; no account or production database access.",
            "Does not measure real window navigation, frame pacing, WindowServer, or footprint.",
            "Background Markdown wall time is descriptive; MainActor gap is the responsiveness gate.",
        ],
        "rounds": run_records, "scenario_acceptance": "running",
    }
    atomic_json(output / "run.json", manifest)
    try:
        for index in range(rounds):
            assert_unchanged(started["source"], source_snapshot(repository))
            log = output / f"round-{index + 1}.log"
            print(f"Round {index + 1}/{rounds}; log: {log}", flush=True)
            record = {"started_utc": utc_now(), "environment_start": environment()}
            run_records.append(record)
            monotonic_start = time.monotonic()
            try:
                with log.open("wb") as stream:
                    record["exit_code"] = execute(
                        command, repository, environment_values, stream, timeout
                    )
            finally:
                record.update({"finished_utc": utc_now(),
                               "command_wall_seconds": time.monotonic() - monotonic_start,
                               "log_sha256": file_hash(log), "environment_end": environment()})
            if record["exit_code"]:
                raise ValueError(f"App scenario failed with exit {record['exit_code']}; see {log}")
            record["measurements"] = measurements(log)
            record["test_product"] = test_product(repository)
            if len(run_records) > 1 and record["test_product"] != run_records[0]["test_product"]:
                raise ValueError("Executed test binary changed between rounds")
            results.append(record["measurements"])
            atomic_json(output / "run.json", manifest)
        assert_unchanged(started["source"], source_snapshot(repository))
        manifest.update({"status": "completed", "source_unchanged": True,
                         "summary_milliseconds": summarize(results),
                         "scenario_acceptance": "passed"})
    except (Exception, KeyboardInterrupt) as error:
        manifest.update({"status": "failed", "scenario_acceptance": "failed",
                         "error": str(error) or type(error).__name__})
        raise
    finally:
        manifest["finished_utc"] = utc_now()
        atomic_json(output / "run.json", manifest)
    return output / "run.json"


def main():
    if os.uname().sysname != "Darwin" or os.uname().machine != "arm64":
        raise ValueError("Use run-app-performance-scenario.sh on native Apple Silicon macOS")
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repository", type=Path, default=Path(__file__).resolve().parents[3])
    parser.add_argument("--output", type=Path)
    parser.add_argument("--rounds", type=int, default=3)
    parser.add_argument("--timeout-seconds", type=int, default=1_800)
    args = parser.parse_args()
    if args.rounds < 3 or args.timeout_seconds < 1:
        parser.error("Use at least 3 rounds and a positive per-round timeout")
    output = args.output
    if output is None:
        root = args.repository / "clients/macos/.build/performance"
        root.mkdir(parents=True, exist_ok=True)
        output = Path(tempfile.mkdtemp(prefix="app-scenario-release-", dir=root))
        output.rmdir()
    print(run_scenario(args.repository, output, args.rounds, args.timeout_seconds))


if __name__ == "__main__":
    try:
        main()
    except (ValueError, OSError, json.JSONDecodeError, KeyboardInterrupt) as error:
        raise SystemExit(f"App scenario failed: {error}")
