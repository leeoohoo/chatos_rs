#!/usr/bin/python3
"""Run the opt-in Release storage benchmark on synthetic temporary data only.

Not an App navigation, physical-footprint, or production database benchmark.
Build time is recorded separately; reported operation timings originate in XCTest.
"""

import argparse
import json
import math
import os
import signal
from pathlib import Path
import subprocess
import sys
import tempfile
import time

sys.dont_write_bytecode = True
from build_evidence import (assert_unchanged, atomic_json, capture, digest, environment,
                            file_hash, native_command, probe, run, source_snapshot, utc_now)


MARKER = "CHATOS_AGENT_GROUP_CHAT_BASELINE "
TEST = "AgentGroupChatStorePerformanceBaselineTests.testRepeatableWorkspaceDataVolumeBaseline"
FIXTURE = {"agents": 20, "rooms": 10, "messages": 500, "runs": 500, "todos": 100,
           "attachment_bytes": 128 * 1024, "attachment_fill": "0x5A", "repetitions": 3}
TIMINGS = (
    "openStoreMilliseconds", "workspaceSnapshotMilliseconds", "recentMessagesMilliseconds",
    "imageAttachmentReadMilliseconds", "publish500RunChangesMilliseconds",
    "idleHeartbeatPoll500Milliseconds", "idleAccountDrainMilliseconds",
)


def measurements(log):
    records = []
    with Path(log).open(errors="replace") as stream:
        for line in stream:
            if MARKER in line:
                records.append(json.loads(line.split(MARKER, 1)[1]))
    if len(records) != 1:
        raise ValueError(f"Expected one benchmark record, got {len(records)}; skipped/missing/duplicate test")
    result = records[0]
    required = {"buildConfiguration", "preparedStatementCountingEnabled", "repetitions",
                "idleHeartbeatPollDatabaseChanges", "idleAccountDrainDatabaseChanges", *TIMINGS,
                *(f"fixture{name}" for name in ("Agents", "Rooms", "Messages", "Runs", "Todos"))}
    if not isinstance(result, dict) or not required.issubset(result):
        raise ValueError("Malformed/incomplete benchmark record")
    if result["buildConfiguration"] != "release" or result["preparedStatementCountingEnabled"] is not False:
        raise ValueError("Expected actual Release measurement without Debug SQL counting")
    for name in ("Agents", "Rooms", "Messages", "Runs", "Todos"):
        if result[f"fixture{name}"] != FIXTURE[name.lower()]:
            raise ValueError(f"Unexpected fixture size: {name}")
    if result["repetitions"] != FIXTURE["repetitions"]:
        raise ValueError("Unexpected number of benchmark repetitions")
    for key in TIMINGS:
        values = result[key]
        if not isinstance(values, list) or len(values) != FIXTURE["repetitions"] or any(
                isinstance(value, bool) or not isinstance(value, (int, float)) or
                not math.isfinite(value) or value < 0 for value in values):
            raise ValueError(f"Missing/invalid timing samples: {key}")
    for key in ("idleHeartbeatPollDatabaseChanges", "idleAccountDrainDatabaseChanges"):
        if result[key] != [0] * FIXTURE["repetitions"] or any(type(value) is not int for value in result[key]):
            raise ValueError(f"Idle operation unexpectedly wrote database changes: {key}")
    return result


def summarize(rounds):
    # Nearest-rank over raw samples, not the mean of per-round percentiles.
    summary = {}
    for key in TIMINGS:
        values = sorted(value for result in rounds for value in result[key])
        summary[key] = {"samples": len(values), "minimum": values[0], "maximum": values[-1],
                        **{f"p{p}": values[math.ceil(len(values) * p / 100) - 1] for p in (50, 95, 99)}}
    return summary


def execute(command, repository, environment_values, stream, timeout):
    # Own a fresh process group: a timeout must not leave XCTest/compiler children
    # running, and must never terminate an unrelated installed App/Host process.
    with subprocess.Popen(command, cwd=repository, env=environment_values, stdout=stream,
                          stderr=subprocess.STDOUT, start_new_session=True) as process:
        try:
            return process.wait(timeout=timeout)
        except (subprocess.TimeoutExpired, KeyboardInterrupt):
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            process.wait()
            raise


def test_product(repository):
    package = Path(repository) / "clients/macos"
    command = native_command(["swift", "build", "--arch", "arm64", "--package-path", str(package), "--build-system", "native",
               "--scratch-path", str(package / ".build-native"), "--force-resolved-versions",
               "-c", "release", "--show-bin-path"])
    products = Path(run(command, repository).decode().strip())
    for relative in ("ChatOSSwiftPackageTests.xctest/Contents/MacOS/ChatOSSwiftPackageTests",
                     "ChatOSSwiftPackageTests.xctest", "ChatOSSwiftPackageTests"):
        binary = products / relative
        if binary.is_file():
            return {"path": str(binary), "sha256": file_hash(binary),
                    "uuid": probe(["dwarfdump", "--uuid", str(binary)]),
                    "query_command": command}
    raise ValueError(f"Cannot locate executed XCTest product under {products}")


def run_baseline(repository, output, rounds, timeout):
    repository, output = Path(repository).resolve(), Path(output).resolve()
    try:
        relative = output.relative_to(repository)
    except ValueError:
        pass
    else:
        ignored = subprocess.run(["git", "check-ignore", "-q", "--", str(relative)], cwd=repository)
        if ignored.returncode != 0:
            raise ValueError("Output inside repository must be ignored, to avoid changing measured inputs")
    output.mkdir(parents=True, exist_ok=False)
    started = capture(repository, "release", "native")
    started["profiles"]["rust"] = None  # This Swift store benchmark does not build/execute Rust.
    atomic_json(output / "inputs.json", started)
    package = repository / "clients/macos"
    command = native_command(["swift", "test", "--arch", "arm64", "--package-path", str(package), "--build-system", "native",
               "--scratch-path", str(package / ".build-native"), "--force-resolved-versions",
               "-c", "release", "--filter", TEST])
    environment_values = dict(os.environ, CHATOS_RUN_AGENT_STORE_BASELINE="1")
    results, run_records = [], []
    manifest = {"format_version": 1, "kind": "engineering-storage-baseline",
                "status": "running", "started_utc": started["started_utc"],
                "inputs_manifest_sha256": file_hash(output / "inputs.json"),
                "source_input_sha256": started["source"]["input_sha256"],
                "command": command, "environment_overrides": {"CHATOS_RUN_AGENT_STORE_BASELINE": "1"},
                "fixture_recipe": FIXTURE, "fixture_recipe_sha256": digest(FIXTURE),
                "fixture_source_sha256": file_hash(package / "Tests/ChatOSConnectorTests/AgentGroupChatStorePerformanceBaselineTests.swift"),
                "dataset_byte_sha256": None,
                "limitations": ["Fixture UUIDs/timestamps vary; recipe/source hash is not a database byte hash.",
                                "Does not measure App navigation, UI hangs, process footprint, IPC, or media.",
                                "Synthetic temporary database only; no active account/database access.",
                                "Small sample percentiles are descriptive, not a release SLO proof."],
                "rounds": run_records, "performance_acceptance": "not-evaluated"}
    atomic_json(output / "run.json", manifest)
    try:
        for index in range(rounds):
            assert_unchanged(started["source"], source_snapshot(repository))
            print(f"Round {index + 1}/{rounds}; log: {output / f'round-{index + 1}.log'}", flush=True)
            record = {"started_utc": utc_now(), "environment_start": environment()}
            run_records.append(record)
            monotonic_start = time.monotonic()
            log = output / f"round-{index + 1}.log"
            try:
                with log.open("wb") as stream:
                    record["exit_code"] = execute(command, repository, environment_values, stream, timeout)
            finally:
                record.update({"finished_utc": utc_now(), "command_wall_seconds": time.monotonic() - monotonic_start,
                               "log_sha256": file_hash(log), "environment_end": environment()})
            if record["exit_code"]:
                raise ValueError(f"Benchmark failed with exit {record['exit_code']}; see {log}")
            record["measurements"] = measurements(log)
            record["test_product"] = test_product(repository)
            if len(run_records) > 1 and record["test_product"] != run_records[0]["test_product"]:
                raise ValueError("Executed test binary changed between rounds")
            results.append(record["measurements"])
            atomic_json(output / "run.json", manifest)
        assert_unchanged(started["source"], source_snapshot(repository))
        manifest.update({"status": "completed", "source_unchanged": True,
                         "summary_milliseconds": summarize(results)})
    except (Exception, KeyboardInterrupt) as error:
        manifest.update({"status": "failed", "error": str(error) or type(error).__name__})
        raise
    finally:
        manifest["finished_utc"] = utc_now()
        atomic_json(output / "run.json", manifest)
    return output / "run.json"


def main():
    if sys.platform != "darwin" or os.uname().machine != "arm64":
        raise ValueError("Use run-performance-baseline.sh; macOS arm64 and native Python are required")
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repository", type=Path, default=Path(__file__).resolve().parents[3])
    parser.add_argument("--output", type=Path, help="New directory outside repository or under an ignored directory")
    parser.add_argument("--rounds", type=int, default=3)
    parser.add_argument("--timeout-seconds", type=int, default=1800)
    args = parser.parse_args()
    if args.rounds < 3 or args.timeout_seconds < 1:
        parser.error("Use at least 3 rounds and a positive per-round timeout")
    output = args.output
    if output is None:
        root = args.repository / "clients/macos/.build/performance"
        root.mkdir(parents=True, exist_ok=True)
        # Reserve a unique name, then let run_baseline create it exclusively.
        output = Path(tempfile.mkdtemp(prefix="store-release-", dir=root))
        output.rmdir()
    print(run_baseline(args.repository, output, args.rounds, args.timeout_seconds))


if __name__ == "__main__":
    try:
        main()
    except (ValueError, OSError, subprocess.SubprocessError, KeyboardInterrupt) as error:
        sys.exit(f"Baseline failed: {error}")
