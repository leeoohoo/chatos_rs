#!/usr/bin/python3
"""Collect bounded macOS process-group resource trends without controlling the target App.

The root PID must belong to an isolated ChatOS acceptance instance. This collector never launches,
stops, or signals the target and never discovers or reads an account database.
"""

import argparse
import ctypes
import json
import math
import os
from pathlib import Path
import stat
import subprocess
import sys
import tempfile
import time

sys.dont_write_bytecode = True
from build_evidence import atomic_json, environment, file_hash, utc_now, verify


RUSAGE_INFO_V2 = 2
PROC_PIDLISTFDS = 1
PROC_PIDLISTTHREADS = 6
FD_INFO_SIZE = 8
THREAD_INFO_SIZE = 8
FORMAL_DURATION_SECONDS = 8 * 60 * 60
FORMAL_MAXIMUM_INTERVAL_SECONDS = 5.0
FORMAL_MINIMUM_SAMPLES = 5_000


class RUsageInfoV2(ctypes.Structure):
    _fields_ = [
        ("uuid", ctypes.c_uint8 * 16),
        *[(name, ctypes.c_uint64) for name in (
            "user_time", "system_time", "package_idle_wakeups", "interrupt_wakeups",
            "pageins", "wired_size", "resident_size", "physical_footprint",
            "process_start_absolute_time", "process_exit_absolute_time", "child_user_time",
            "child_system_time", "child_package_idle_wakeups", "child_interrupt_wakeups",
            "child_pageins", "child_elapsed_absolute_time", "disk_bytes_read",
            "disk_bytes_written",
        )],
    ]


class DarwinProcessMetrics:
    def __init__(self):
        self.library = ctypes.CDLL("/usr/lib/libproc.dylib", use_errno=True)
        self.library.proc_pid_rusage.argtypes = [
            ctypes.c_int, ctypes.c_int, ctypes.POINTER(RUsageInfoV2),
        ]
        self.library.proc_pid_rusage.restype = ctypes.c_int
        self.library.proc_pidinfo.argtypes = [
            ctypes.c_int, ctypes.c_int, ctypes.c_uint64, ctypes.c_void_p, ctypes.c_int,
        ]
        self.library.proc_pidinfo.restype = ctypes.c_int

    def usage(self, pid):
        value = RUsageInfoV2()
        if self.library.proc_pid_rusage(pid, RUSAGE_INFO_V2, ctypes.byref(value)) != 0:
            return None
        return {
            "cpu_time_nanoseconds": value.user_time + value.system_time,
            "physical_footprint_bytes": value.physical_footprint,
            "resident_size_bytes": value.resident_size,
            "disk_bytes_read": value.disk_bytes_read,
            "disk_bytes_written": value.disk_bytes_written,
            "process_start_absolute_time": value.process_start_absolute_time,
        }

    def _list_count(self, pid, flavor, item_size):
        size = item_size * 64
        while size <= item_size * 65_536:
            buffer = ctypes.create_string_buffer(size)
            result = self.library.proc_pidinfo(pid, flavor, 0, buffer, size)
            if result <= 0:
                return None
            if result < size:
                return result // item_size
            size *= 2
        return None

    def fd_count(self, pid):
        return self._list_count(pid, PROC_PIDLISTFDS, FD_INFO_SIZE)

    def thread_count(self, pid):
        return self._list_count(pid, PROC_PIDLISTTHREADS, THREAD_INFO_SIZE)


def process_table():
    output = subprocess.run(
        ["/bin/ps", "-axo", "pid=,ppid=,comm="],
        check=True,
        capture_output=True,
        text=True,
        timeout=10,
    ).stdout
    table = {}
    for line in output.splitlines():
        fields = line.strip().split(None, 2)
        if len(fields) != 3:
            continue
        try:
            pid, parent = int(fields[0]), int(fields[1])
        except ValueError:
            continue
        table[pid] = {"pid": pid, "parent_pid": parent, "command": fields[2]}
    return table


def descendant_processes(root_pid, table):
    children = {}
    for item in table.values():
        children.setdefault(item["parent_pid"], []).append(item["pid"])
    result, pending, seen = [], [root_pid], set()
    while pending:
        pid = pending.pop()
        if pid in seen:
            continue
        seen.add(pid)
        item = table.get(pid)
        if item is None:
            continue
        result.append(item)
        pending.extend(children.get(pid, ()))
    return sorted(result, key=lambda item: item["pid"])


def scan_temporary_roots(roots, maximum_entries=100_000):
    files = directories = bytes_used = entries = 0
    for root in roots:
        root = Path(root)
        root_status = root.lstat()
        if stat.S_ISLNK(root_status.st_mode) or not stat.S_ISDIR(root_status.st_mode):
            raise ValueError(f"Temporary root must be a real directory: {root}")
        pending = [root]
        while pending:
            directory = pending.pop()
            with os.scandir(directory) as stream:
                for item in stream:
                    entries += 1
                    if entries > maximum_entries:
                        raise ValueError("Temporary-root scan exceeded its bounded entry limit")
                    status = item.stat(follow_symlinks=False)
                    if stat.S_ISLNK(status.st_mode):
                        files += 1
                    elif stat.S_ISDIR(status.st_mode):
                        directories += 1
                        pending.append(Path(item.path))
                    else:
                        files += 1
                        bytes_used += status.st_size
    return {"temporary_file_count": files, "temporary_directory_count": directories,
            "temporary_bytes": bytes_used}


def nearest_rank(values, percentile):
    values = sorted(values)
    return values[math.ceil(len(values) * percentile / 100) - 1]


def trend(values):
    increases = sum(current > prior for prior, current in zip(values, values[1:]))
    decreases = sum(current < prior for prior, current in zip(values, values[1:]))
    return {
        "first": values[0], "final": values[-1], "minimum": min(values), "maximum": max(values),
        "net_change": values[-1] - values[0], "increase_steps": increases,
        "decrease_steps": decreases,
        "monotonic_growth": len(values) >= 3 and values[-1] > values[0] and decreases == 0,
    }


def summarize(samples):
    scalar_keys = (
        "process_count", "physical_footprint_bytes", "resident_size_bytes", "fd_count",
        "kernel_thread_count", "cpu_percent", "disk_read_delta_bytes", "disk_write_delta_bytes",
        "temporary_file_count", "temporary_directory_count", "temporary_bytes",
    )
    result = {}
    for key in scalar_keys:
        values = [sample["totals"][key] for sample in samples]
        result[key] = {
            "samples": len(values), "p50": nearest_rank(values, 50),
            "p95": nearest_rank(values, 95), "p99": nearest_rank(values, 99), **trend(values),
        }
    return result


def acceptance_failures(args, summary, seen_counts, sample_count):
    failures = []
    if sample_count < args.minimum_samples:
        failures.append(f"sample_count {sample_count} < {args.minimum_samples}")
    for name in args.require_process:
        count = seen_counts.get(name, 0)
        if count < args.required_process_minimum_samples:
            failures.append(
                f"required process {name!r} seen in {count} samples < "
                f"{args.required_process_minimum_samples}"
            )
    if summary["physical_footprint_bytes"]["p95"] > args.max_group_physical_footprint_bytes:
        failures.append("group physical-footprint p95 exceeded the configured limit")
    if summary["cpu_percent"]["p95"] > args.max_cpu_percent_p95:
        failures.append("group CPU p95 exceeded the configured limit")
    if summary["disk_write_delta_bytes"]["maximum"] > args.max_disk_write_bytes_per_interval:
        failures.append("group interval disk writes exceeded the configured limit")
    for key in ("fd_count", "kernel_thread_count", "temporary_file_count", "temporary_bytes"):
        if summary[key]["monotonic_growth"]:
            failures.append(f"{key} grew monotonically")
    return failures


def validate_formal_acceptance(args):
    if not args.evaluate_acceptance:
        return
    if args.duration_seconds < FORMAL_DURATION_SECONDS:
        raise ValueError("Formal acceptance requires at least 8 hours")
    if args.sample_interval_seconds > FORMAL_MAXIMUM_INTERVAL_SECONDS:
        raise ValueError("Formal acceptance requires a sample interval no greater than 5 seconds")
    if args.minimum_samples < FORMAL_MINIMUM_SAMPLES:
        raise ValueError("Formal acceptance requires at least 5,000 samples")
    if not args.require_process or not args.temporary_root:
        raise ValueError("Formal acceptance requires explicit process and temporary-root coverage")
    if args.app is None or args.build_manifest is None:
        raise ValueError("Formal acceptance requires an App and its build manifest")
    for value, name in (
        (args.max_group_physical_footprint_bytes, "physical-footprint"),
        (args.max_cpu_percent_p95, "CPU"),
        (args.max_disk_write_bytes_per_interval, "disk-write"),
    ):
        if value is None or value <= 0:
            raise ValueError(f"Formal acceptance requires a positive {name} limit")


def default_output(repository):
    root = Path(repository) / "clients/macos/.build/performance"
    root.mkdir(parents=True, exist_ok=True)
    output = Path(tempfile.mkdtemp(prefix="process-soak-", dir=root))
    output.rmdir()
    return output


def validate_output_location(repository, output):
    repository, output = Path(repository).resolve(), Path(output).resolve()
    try:
        relative = output.relative_to(repository)
    except ValueError:
        return
    ignored = subprocess.run(
        ["git", "check-ignore", "-q", "--", str(relative)], cwd=repository, timeout=10,
    )
    if ignored.returncode != 0:
        raise ValueError("Output inside the repository must be ignored")


def validate_output_is_not_scanned(output, temporary_roots):
    output = Path(output).resolve()
    for temporary_root in temporary_roots:
        root = Path(temporary_root).resolve()
        if output == root or output.is_relative_to(root):
            raise ValueError(
                "Soak output must be outside every scanned temporary root "
                "to avoid measuring the collector's own files"
            )


def build_identity(args):
    if args.app is None and args.build_manifest is None:
        return None
    if args.app is None or args.build_manifest is None:
        raise ValueError("App and build manifest must be supplied together")
    app, manifest_path = Path(args.app).resolve(), Path(args.build_manifest).resolve()
    manifest = json.loads(manifest_path.read_text())
    verify(app, manifest)
    if args.evaluate_acceptance and manifest.get("classification") != "clean-release-build":
        raise ValueError("Formal acceptance requires a clean Release build manifest")
    return {"app": str(app), "manifest": str(manifest_path),
            "manifest_sha256": file_hash(manifest_path),
            "classification": manifest.get("classification"),
            "git_commit": manifest.get("source", {}).get("git_commit")}


def collect(args):
    validate_formal_acceptance(args)
    output = Path(args.output).resolve()
    validate_output_location(args.repository, output)
    validate_output_is_not_scanned(output, args.temporary_root)
    output.mkdir(parents=True, exist_ok=False)
    recorded_build = build_identity(args)
    metrics = DarwinProcessMetrics()
    table = process_table()
    root_item = table.get(args.root_pid)
    root_usage = metrics.usage(args.root_pid)
    if root_item is None or root_usage is None:
        raise ValueError("Root PID is unavailable")
    if recorded_build is not None:
        expected_executable = Path(recorded_build["app"]) / "Contents/MacOS/ChatOSSwift"
        if Path(root_item["command"]).resolve() != expected_executable.resolve():
            raise ValueError("Root PID does not execute the manifest-bound ChatOS App")
    root_identity = root_usage["process_start_absolute_time"]
    manifest = {
        "format_version": 1,
        "kind": "macos-process-group-soak",
        "status": "running",
        "performance_acceptance": "running" if args.evaluate_acceptance else "not-evaluated",
        "started_utc": utc_now(),
        "environment_start": environment(),
        "root": {**root_item, "process_start_absolute_time": root_identity},
        "build": recorded_build,
        "configuration": {
            "duration_seconds": args.duration_seconds,
            "sample_interval_seconds": args.sample_interval_seconds,
            "minimum_samples": args.minimum_samples,
            "required_processes": args.require_process,
            "required_process_minimum_samples": args.required_process_minimum_samples,
            "temporary_roots": [str(Path(value).resolve()) for value in args.temporary_root],
            "evaluate_acceptance": args.evaluate_acceptance,
            "limits": {
                "max_group_physical_footprint_bytes": args.max_group_physical_footprint_bytes,
                "max_cpu_percent_p95": args.max_cpu_percent_p95,
                "max_disk_write_bytes_per_interval": args.max_disk_write_bytes_per_interval,
            },
        },
        "limitations": [
            "Kernel thread count is not an internal Swift Task or Notification observer count.",
            "The collector observes an externally orchestrated load; it does not create UI navigation or jobs.",
            "Only explicitly supplied temporary roots are scanned; active account databases are never discovered.",
        ],
    }
    atomic_json(output / "run.json", manifest)
    samples, seen_counts, prior_by_identity = [], {}, {}
    deadline = time.monotonic() + args.duration_seconds
    next_sample = time.monotonic()
    try:
        with (output / "samples.jsonl").open("w", encoding="utf-8") as stream:
            while time.monotonic() < deadline:
                now = time.monotonic()
                if now < next_sample:
                    time.sleep(next_sample - now)
                sample_started = time.monotonic()
                table = process_table()
                processes, process_samples = descendant_processes(args.root_pid, table), []
                current_by_identity = {}
                seen_this_sample = set()
                total_cpu_delta = total_read_delta = total_write_delta = 0
                totals = {"process_count": 0, "physical_footprint_bytes": 0,
                          "resident_size_bytes": 0, "fd_count": 0, "kernel_thread_count": 0}
                for item in processes:
                    usage = metrics.usage(item["pid"])
                    if usage is None:
                        continue
                    identity = (item["pid"], usage["process_start_absolute_time"])
                    if item["pid"] == args.root_pid and identity[1] != root_identity:
                        raise ValueError("Root PID exited or was reused")
                    fd_count = metrics.fd_count(item["pid"])
                    thread_count = metrics.thread_count(item["pid"])
                    if fd_count is None or thread_count is None:
                        continue
                    name = Path(item["command"]).name
                    for required in args.require_process:
                        if name == required:
                            seen_this_sample.add(required)
                    prior = prior_by_identity.get(identity)
                    if prior is not None:
                        total_cpu_delta += max(0, usage["cpu_time_nanoseconds"] - prior["cpu_time_nanoseconds"])
                        total_read_delta += max(0, usage["disk_bytes_read"] - prior["disk_bytes_read"])
                        total_write_delta += max(0, usage["disk_bytes_written"] - prior["disk_bytes_written"])
                    current_by_identity[identity] = usage
                    process_sample = {**item, "name": name, **usage,
                                      "fd_count": fd_count, "kernel_thread_count": thread_count}
                    process_samples.append(process_sample)
                    totals["process_count"] += 1
                    for key in ("physical_footprint_bytes", "resident_size_bytes", "fd_count",
                                "kernel_thread_count"):
                        totals[key] += process_sample[key]
                if not any(item["pid"] == args.root_pid for item in process_samples):
                    raise ValueError("Root PID exited during collection")
                for required in seen_this_sample:
                    seen_counts[required] = seen_counts.get(required, 0) + 1
                temporary = scan_temporary_roots(args.temporary_root) if args.temporary_root else {
                    "temporary_file_count": 0, "temporary_directory_count": 0, "temporary_bytes": 0,
                }
                elapsed = max(sample_started - (samples[-1]["monotonic_seconds"] if samples else sample_started), 0.001)
                totals.update(temporary)
                totals.update({"cpu_percent": total_cpu_delta / (elapsed * 1_000_000_000) * 100,
                               "disk_read_delta_bytes": total_read_delta,
                               "disk_write_delta_bytes": total_write_delta})
                sample = {"index": len(samples), "utc": utc_now(), "monotonic_seconds": sample_started,
                          "collection_seconds": time.monotonic() - sample_started,
                          "totals": totals, "processes": process_samples}
                stream.write(json.dumps(sample, sort_keys=True, separators=(",", ":")) + "\n")
                stream.flush()
                samples.append(sample)
                prior_by_identity = current_by_identity
                next_sample += args.sample_interval_seconds
        if recorded_build is not None and build_identity(args) != recorded_build:
            raise ValueError("App or build manifest changed during collection")
        summary = summarize(samples)
        failures = acceptance_failures(args, summary, seen_counts, len(samples)) \
            if args.evaluate_acceptance else []
        manifest.update({"status": "completed", "finished_utc": utc_now(),
                         "environment_end": environment(), "sample_count": len(samples),
                         "required_process_seen_samples": seen_counts, "summary": summary,
                         "acceptance_failures": failures,
                         "performance_acceptance": (
                             "failed" if failures else "passed"
                         ) if args.evaluate_acceptance else "not-evaluated"})
    except (Exception, KeyboardInterrupt) as error:
        manifest.update({"status": "failed", "finished_utc": utc_now(),
                         "sample_count": len(samples), "error": str(error) or type(error).__name__,
                         "performance_acceptance": "failed" if args.evaluate_acceptance else "not-evaluated"})
        raise
    finally:
        atomic_json(output / "run.json", manifest)
    return output / "run.json"


def parse_arguments(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repository", type=Path, default=Path(__file__).resolve().parents[3])
    parser.add_argument("--output", type=Path)
    parser.add_argument("--app", type=Path)
    parser.add_argument("--build-manifest", type=Path)
    parser.add_argument("--root-pid", type=int, required=True)
    parser.add_argument("--duration-seconds", type=float, default=FORMAL_DURATION_SECONDS)
    parser.add_argument("--sample-interval-seconds", type=float, default=FORMAL_MAXIMUM_INTERVAL_SECONDS)
    parser.add_argument("--minimum-samples", type=int, default=FORMAL_MINIMUM_SAMPLES)
    parser.add_argument("--require-process", action="append", default=[])
    parser.add_argument("--required-process-minimum-samples", type=int, default=1)
    parser.add_argument("--temporary-root", action="append", type=Path, default=[])
    parser.add_argument("--evaluate-acceptance", action="store_true")
    parser.add_argument("--max-group-physical-footprint-bytes", type=int)
    parser.add_argument("--max-cpu-percent-p95", type=float)
    parser.add_argument("--max-disk-write-bytes-per-interval", type=int)
    args = parser.parse_args(argv)
    if args.root_pid <= 1 or args.duration_seconds <= 0 or args.sample_interval_seconds <= 0:
        parser.error("PID, duration, and interval must be positive")
    if args.minimum_samples < 1 or args.required_process_minimum_samples < 1:
        parser.error("Sample requirements must be positive")
    if args.output is None:
        args.output = default_output(args.repository)
    return args


def main():
    if sys.platform != "darwin" or os.uname().machine != "arm64":
        raise ValueError("Use run-process-group-soak.sh; native Apple Silicon macOS is required")
    args = parse_arguments()
    print(collect(args))


if __name__ == "__main__":
    try:
        main()
    except (ValueError, OSError, subprocess.SubprocessError, KeyboardInterrupt) as error:
        sys.exit(f"Soak collection failed: {error}")
