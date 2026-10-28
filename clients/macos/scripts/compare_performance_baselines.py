#!/usr/bin/python3
"""Compare two completed ChatOS Release storage baselines and gate regressions."""

import argparse
import json
import math
from pathlib import Path
import sys

sys.dont_write_bytecode = True
from build_evidence import atomic_json, file_hash
from run_performance_baseline import TIMINGS


PERCENTILES = ("p50", "p95", "p99")


def load_manifest(path):
    path = Path(path).resolve()
    with path.open() as stream:
        value = json.load(stream)
    if not isinstance(value, dict):
        raise ValueError(f"Baseline manifest is not an object: {path}")
    return path, value


def environment_identity(manifest):
    rounds = manifest.get("rounds")
    if not isinstance(rounds, list) or not rounds:
        raise ValueError("Baseline has no recorded rounds")
    environment = rounds[0].get("environment_start")
    if not isinstance(environment, dict):
        raise ValueError("Baseline has no starting environment")

    def probe_value(name):
        probe = environment.get(name)
        if not isinstance(probe, dict) or probe.get("available") is not True:
            raise ValueError(f"Baseline environment probe is unavailable: {name}")
        value = probe.get("value")
        if not isinstance(value, str) or not value:
            raise ValueError(f"Baseline environment probe is invalid: {name}")
        return value

    power = probe_value("power").splitlines()[0]
    return {
        "collector_architecture": environment.get("collector_architecture"),
        "hardware": probe_value("hardware"),
        "os": probe_value("os"),
        "power_source": power,
    }


def validate_manifest(manifest, minimum_samples):
    if manifest.get("status") != "completed" or manifest.get("source_unchanged") is not True:
        raise ValueError("Baseline must be completed with unchanged source inputs")
    if manifest.get("performance_acceptance") != "not-evaluated":
        raise ValueError("Expected an unevaluated engineering storage baseline")
    if not isinstance(manifest.get("fixture_recipe_sha256"), str):
        raise ValueError("Baseline fixture recipe identity is missing")
    if not isinstance(manifest.get("fixture_source_sha256"), str):
        raise ValueError("Baseline fixture source identity is missing")
    summary = manifest.get("summary_milliseconds")
    if not isinstance(summary, dict):
        raise ValueError("Baseline timing summary is missing")
    for metric in TIMINGS:
        measurement = summary.get(metric)
        if not isinstance(measurement, dict):
            raise ValueError(f"Missing timing summary: {metric}")
        samples = measurement.get("samples")
        if type(samples) is not int or samples < minimum_samples:
            raise ValueError(f"Insufficient timing samples: {metric}")
        for percentile in PERCENTILES:
            value = measurement.get(percentile)
            if isinstance(value, bool) or not isinstance(value, (int, float)) \
                    or not math.isfinite(value) or value < 0:
                raise ValueError(f"Invalid {percentile} timing: {metric}")
    return environment_identity(manifest)


def compare(baseline, candidate, relative_tolerance=0.15,
            absolute_tolerance_milliseconds=0.25, minimum_samples=9):
    if not 0 <= relative_tolerance <= 1:
        raise ValueError("Relative tolerance must be between 0 and 1")
    if absolute_tolerance_milliseconds < 0 or not math.isfinite(absolute_tolerance_milliseconds):
        raise ValueError("Absolute tolerance must be a finite non-negative value")
    if minimum_samples < 1:
        raise ValueError("Minimum samples must be positive")

    baseline_environment = validate_manifest(baseline, minimum_samples)
    candidate_environment = validate_manifest(candidate, minimum_samples)
    if baseline.get("fixture_recipe_sha256") != candidate.get("fixture_recipe_sha256"):
        raise ValueError("Fixture recipes differ; results are not comparable")
    if baseline.get("fixture_source_sha256") != candidate.get("fixture_source_sha256"):
        raise ValueError("Fixture source changed; establish a new baseline instead of comparing")
    if baseline_environment != candidate_environment:
        raise ValueError("Hardware, OS, architecture, or power source differs")

    comparisons = []
    failures = []
    for metric in TIMINGS:
        baseline_metric = baseline["summary_milliseconds"][metric]
        candidate_metric = candidate["summary_milliseconds"][metric]
        for percentile in PERCENTILES:
            baseline_value = float(baseline_metric[percentile])
            candidate_value = float(candidate_metric[percentile])
            limit = max(
                baseline_value * (1 + relative_tolerance),
                baseline_value + absolute_tolerance_milliseconds,
            )
            passed = candidate_value <= limit
            record = {
                "metric": metric,
                "percentile": percentile,
                "baseline_milliseconds": baseline_value,
                "candidate_milliseconds": candidate_value,
                "limit_milliseconds": limit,
                "delta_milliseconds": candidate_value - baseline_value,
                "delta_ratio": None if baseline_value == 0 else candidate_value / baseline_value - 1,
                "passed": passed,
            }
            comparisons.append(record)
            if not passed:
                failures.append({key: record[key] for key in (
                    "metric", "percentile", "baseline_milliseconds",
                    "candidate_milliseconds", "limit_milliseconds",
                )})
    return {
        "format_version": 1,
        "kind": "engineering-storage-baseline-comparison",
        "status": "passed" if not failures else "failed",
        "policy": {
            "relative_tolerance": relative_tolerance,
            "absolute_tolerance_milliseconds": absolute_tolerance_milliseconds,
            "minimum_samples_per_metric": minimum_samples,
            "percentiles": list(PERCENTILES),
            "limit_formula": "max(baseline * (1 + relative), baseline + absolute_ms)",
        },
        "environment": baseline_environment,
        "comparisons": comparisons,
        "failures": failures,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", required=True, type=Path)
    parser.add_argument("--candidate", required=True, type=Path)
    parser.add_argument("--output", type=Path, help="New JSON report path; stdout when omitted")
    parser.add_argument("--relative-tolerance", type=float, default=0.15)
    parser.add_argument("--absolute-tolerance-ms", type=float, default=0.25)
    parser.add_argument("--minimum-samples", type=int, default=9)
    args = parser.parse_args()

    baseline_path, baseline = load_manifest(args.baseline)
    candidate_path, candidate = load_manifest(args.candidate)
    report = compare(
        baseline,
        candidate,
        relative_tolerance=args.relative_tolerance,
        absolute_tolerance_milliseconds=args.absolute_tolerance_ms,
        minimum_samples=args.minimum_samples,
    )
    report["baseline"] = {"path": str(baseline_path), "sha256": file_hash(baseline_path)}
    report["candidate"] = {"path": str(candidate_path), "sha256": file_hash(candidate_path)}
    if args.output:
        output = args.output.resolve()
        if output.exists():
            raise ValueError(f"Output already exists: {output}")
        output.parent.mkdir(parents=True, exist_ok=True)
        atomic_json(output, report)
        print(output)
    else:
        print(json.dumps(report, ensure_ascii=False, indent=2, sort_keys=True))
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (ValueError, OSError, json.JSONDecodeError) as error:
        sys.exit(f"Comparison failed: {error}")
