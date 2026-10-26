import json
import os
from pathlib import Path
import plistlib
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import build_evidence as evidence
import run_performance_baseline as baseline


class RepositoryTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.repository = self.root / "repository"
        self.repository.mkdir()
        self.git("init", "-q")
        self.git("config", "user.name", "Evidence Test")
        self.git("config", "user.email", "evidence@example.invalid")
        (self.repository / ".gitignore").write_text("ignored/\n")
        (self.repository / "input.swift").write_text("original\n")
        self.git("add", ".")
        self.git("commit", "-qm", "fixture")

    def git(self, *arguments):
        return evidence.run(["git", *arguments], self.repository)

    def test_clean_snapshot_is_repeatable(self):
        first = evidence.source_snapshot(self.repository)
        self.assertFalse(first["dirty"])
        self.assertEqual(first, evidence.source_snapshot(self.repository))

    def test_dirty_input_changes_hash_even_when_git_status_is_unchanged(self):
        source = self.repository / "input.swift"
        source.write_text("first dirty version\n")
        first = evidence.source_snapshot(self.repository)
        source.write_text("other dirty version\n")
        second = evidence.source_snapshot(self.repository)
        self.assertTrue(first["dirty"])
        self.assertEqual(first["status_sha256"], second["status_sha256"])
        self.assertNotEqual(first["input_sha256"], second["input_sha256"])
        with self.assertRaisesRegex(ValueError, "changed during build"):
            evidence.assert_unchanged(first, second)

    def test_untracked_input_is_included_but_ignored_output_is_not(self):
        initial = evidence.source_snapshot(self.repository)
        ignored = self.repository / "ignored"
        ignored.mkdir()
        (ignored / "run.json").write_text("build output")
        self.assertEqual(initial, evidence.source_snapshot(self.repository))
        (self.repository / "new.swift").write_text("untracked source")
        changed = evidence.source_snapshot(self.repository)
        self.assertNotEqual(initial["input_sha256"], changed["input_sha256"])
        self.assertTrue(changed["dirty"])

    def test_deletion_and_executable_bit_change_are_recorded(self):
        initial = evidence.source_snapshot(self.repository)
        source = self.repository / "input.swift"
        source.chmod(0o755)
        self.assertNotEqual(initial["input_sha256"], evidence.source_snapshot(self.repository)["input_sha256"])
        source.unlink()
        changed = evidence.source_snapshot(self.repository)
        self.assertIn({"path": "input.swift", "kind": "missing"}, changed["files"])

    def test_symlink_is_not_followed(self):
        (self.repository / "link").symlink_to(self.root / "absent-external-file")
        result = evidence.source_snapshot(self.repository)
        self.assertTrue(result["has_symlinks"])
        self.assertEqual(next(item for item in result["files"] if item["path"] == "link")["kind"], "symlink")

    def test_index_change_without_worktree_change_is_rejected(self):
        source = self.repository / "input.swift"
        source.write_text("staged one\n")
        self.git("add", "input.swift")
        source.write_text("final working copy\n")
        first = evidence.source_snapshot(self.repository)
        source.write_text("staged two\n")
        self.git("add", "input.swift")
        source.write_text("final working copy\n")
        second = evidence.source_snapshot(self.repository)
        self.assertEqual(first["input_sha256"], second["input_sha256"])
        self.assertEqual(first["status_sha256"], second["status_sha256"])
        with self.assertRaises(ValueError):
            evidence.assert_unchanged(first, second)

    def make_app(self, configuration="release", commit=None):
        app = self.root / "ChatOS.app"
        for binary in ("MacOS/ChatOSSwift", "MacOS/chatos_local_agent_host", "MacOS/ChatOSMediaWorker",
                       "MacOS/ChatOSCodeReadWorker",
                       "MacOS/ChatOSAgentWorker",
                       "Resources/Tools/darwin-arm64/ps", "Resources/Tools/darwin-arm64/rg"):
            path = app / "Contents" / binary
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(b"fixture executable")
            path.chmod(0o755)
        with (app / "Contents/Info.plist").open("wb") as stream:
            plistlib.dump({"ChatOSBuildConfiguration": configuration,
                          "ChatOSBuildGitCommit": commit or self.git("rev-parse", "HEAD").decode().strip(),
                          "CFBundleShortVersionString": "1.0", "CFBundleVersion": "1"}, stream)
        return app

    def finish(self, app, configuration="release", symbols=()):
        captured = {"source": evidence.source_snapshot(self.repository),
                    "profiles": {"swift": configuration, "rust": configuration}}
        with patch.object(evidence, "environment", return_value={}), \
             patch.object(evidence, "probe", return_value={"available": True, "value": "UUID: AAA (arm64)"}), \
             patch.object(evidence, "signature", return_value="Signature=adhoc"):
            return evidence.finish(self.repository, app, captured, [], symbols)

    def test_clean_release_is_not_claimed_as_performance_or_distribution_acceptance(self):
        result = self.finish(self.make_app())
        self.assertEqual(result["classification"], "clean-release-build")
        self.assertIn("Contents/MacOS/ChatOSMediaWorker", result["app"]["binaries"])
        self.assertIn("Contents/MacOS/ChatOSCodeReadWorker", result["app"]["binaries"])
        self.assertIn("Contents/MacOS/ChatOSAgentWorker", result["app"]["binaries"])
        self.assertEqual(result["performance_acceptance"], "not-run")
        self.assertEqual(result["distribution_acceptance"], "not-run")
        self.assertEqual(result["symbolication_status"], "not-collected")

    def test_dirty_release_and_debug_are_only_diagnostic(self):
        app = self.make_app()
        (self.repository / "input.swift").write_text("dirty")
        self.assertEqual(self.finish(app)["classification"], "diagnostic-build")
        with (app / "Contents/Info.plist").open("rb") as stream:
            info = plistlib.load(stream)
        info["ChatOSBuildConfiguration"] = "debug"
        with (app / "Contents/Info.plist").open("wb") as stream:
            plistlib.dump(info, stream)
        self.assertEqual(self.finish(app, "debug")["classification"], "diagnostic-build")

    def test_overridden_commit_and_configuration_are_rejected(self):
        app = self.make_app(commit="0" * 40)
        with self.assertRaisesRegex(ValueError, "Git commit"):
            self.finish(app)
        with (app / "Contents/Info.plist").open("rb") as stream:
            info = plistlib.load(stream)
        info["ChatOSBuildGitCommit"] = self.git("rev-parse", "HEAD").decode().strip()
        with (app / "Contents/Info.plist").open("wb") as stream:
            plistlib.dump(info, stream)
        with self.assertRaisesRegex(ValueError, "configuration"):
            self.finish(app, "debug")

    def test_manifest_detects_changed_binary_resource_and_added_file(self):
        app = self.make_app()
        result = self.finish(app)
        evidence.verify(app, result)
        for relative in ("Contents/MacOS/ChatOSSwift", "Contents/Resources/new-resource"):
            path = app / relative
            old = path.read_bytes() if path.exists() else None
            path.write_bytes(b"changed")
            with self.assertRaisesRegex(ValueError, "differ"):
                evidence.verify(app, result)
            if old is None:
                path.unlink()
            else:
                path.write_bytes(old)
        evidence.verify(app, result)

    def test_ambiguous_or_mismatched_symbols_are_rejected(self):
        app = self.make_app()
        symbols = self.root / "app.dSYM"
        symbols.mkdir()
        with self.assertRaisesRegex(ValueError, "exactly one"):
            self.finish(app, symbols=[symbols])

    def test_matching_symbols_are_recorded_and_tampering_is_rejected(self):
        app = self.make_app()
        symbols = self.root / "app.dSYM"
        symbols.mkdir()
        dwarf = symbols / "debug-data"
        dwarf.write_text("symbols")
        captured = {"source": evidence.source_snapshot(self.repository),
                    "profiles": {"swift": "release", "rust": "release"}}

        def uuid(command):
            return {"available": True, "value": "UUID: " + (
                "AAA" if Path(command[-1]).name in ("ChatOSSwift", "app.dSYM") else Path(command[-1]).name
            ) + " (arm64)"}

        with patch.object(evidence, "environment", return_value={}), \
             patch.object(evidence, "probe", side_effect=uuid), \
             patch.object(evidence, "signature", return_value="Signature=adhoc"):
            result = evidence.finish(self.repository, app, captured, [], [symbols])
        self.assertEqual(result["symbolication_status"], "uuid-matched")
        evidence.verify(app, result)
        dwarf.write_text("wrong symbols")
        with self.assertRaisesRegex(ValueError, "Symbol files differ"):
            evidence.verify(app, result)


class BaselineTests(unittest.TestCase):
    def valid_result(self):
        return {"buildConfiguration": "release", "preparedStatementCountingEnabled": False,
                **{f"fixture{name.title()}": baseline.FIXTURE[name] for name in
                   ("agents", "rooms", "messages", "runs", "todos")},
                "repetitions": 3, **{key: [1.0, 2.0, 3.0] for key in baseline.TIMINGS},
                "idleHeartbeatPollDatabaseChanges": [0, 0, 0], "idleAccountDrainDatabaseChanges": [0, 0, 0]}

    def read(self, records):
        with tempfile.TemporaryDirectory() as directory:
            log = Path(directory) / "test.log"
            log.write_text("Build log\n" + "".join(baseline.MARKER + json.dumps(item) + "\n" for item in records))
            return baseline.measurements(log)

    def test_valid_release_record(self):
        result = self.valid_result()
        self.assertEqual(self.read([result]), result)

    def test_skipped_or_duplicate_benchmark_is_not_a_success(self):
        for records in ([], [self.valid_result(), self.valid_result()]):
            with self.assertRaisesRegex(ValueError, "Expected one"):
                self.read(records)

    def test_debug_wrong_fixture_writes_and_invalid_samples_are_rejected(self):
        for key, value in (("buildConfiguration", "debug"), ("fixtureMessages", 1),
                           ("preparedStatementCountingEnabled", True), ("repetitions", 1),
                           ("idleAccountDrainDatabaseChanges", [0, 1, 0]),
                           ("openStoreMilliseconds", [1.0, float("nan"), 2.0]),
                           ("openStoreMilliseconds", [True, 1, 2]),
                           ("openStoreMilliseconds", [-1, 1, 2]),
                           ("openStoreMilliseconds", None),
                           ("idleAccountDrainDatabaseChanges", [False, 0, 0]),
                           ("openStoreMilliseconds", [1, 2])):
            with self.subTest(key=key, value=value):
                result = self.valid_result()
                result[key] = value
                with self.assertRaises(ValueError):
                    self.read([result])

    def test_missing_or_non_object_record_is_rejected(self):
        for result in ({}, None, [], "not measurements"):
            with self.assertRaisesRegex(ValueError, "Malformed/incomplete"):
                self.read([result])

    def test_apple_silicon_commands_are_arm64_only(self):
        with patch.object(evidence.sys, "platform", "darwin"), \
             patch.object(evidence, "run", return_value=b"1\n"):
            self.assertEqual(evidence.native_command(["swift", "test"]),
                             ["/usr/bin/arch", "-arm64", "swift", "test"])

    def test_intel_mac_and_non_macos_are_rejected(self):
        with patch.object(evidence.sys, "platform", "darwin"), \
             patch.object(evidence, "run", return_value=b"0\n"):
            with self.assertRaisesRegex(ValueError, "Intel macOS is unsupported"):
                evidence.native_command(["swift", "test"])
        with patch.object(evidence.sys, "platform", "linux"):
            with self.assertRaisesRegex(ValueError, "requires Apple Silicon"):
                evidence.native_command(["swift", "test"])

    def test_unknown_hardware_never_falls_back_to_intel(self):
        with patch.object(evidence.sys, "platform", "darwin"), \
             patch.object(evidence, "run", side_effect=OSError("sysctl unavailable")):
            with self.assertRaisesRegex(ValueError, "Cannot verify Apple Silicon"):
                evidence.native_command(["swift", "test"])

    def test_shell_entry_points_reject_intel_before_building_or_writing(self):
        scripts = Path(__file__).resolve().parents[1]
        with tempfile.TemporaryDirectory() as directory:
            uname = Path(directory) / "uname"
            uname.write_text("#!/bin/sh\nprintf '%s\\n' x86_64\n")
            uname.chmod(0o755)
            environment = dict(os.environ, PATH=directory + ":/usr/bin:/bin")
            for script in ("package-app.sh", "verify-app-build.sh", "run-performance-baseline.sh",
                           "run-process-group-soak.sh"):
                with self.subTest(script=script):
                    result = subprocess.run(["/bin/zsh", str(scripts / script)],
                                            env=environment, capture_output=True, timeout=10)
                    self.assertEqual(result.returncode, 2)
                    self.assertIn(b"arm64", result.stderr)

    def test_percentiles_use_all_raw_samples(self):
        rounds = [self.valid_result(), self.valid_result(), self.valid_result()]
        rounds[2]["openStoreMilliseconds"] = [10, 20, 100]
        result = baseline.summarize(rounds)["openStoreMilliseconds"]
        self.assertEqual(result, {"samples": 9, "minimum": 1, "maximum": 100,
                                  "p50": 3, "p95": 100, "p99": 100})

    def test_atomic_manifest_has_no_leftover_temporary_file(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "manifest.json"
            evidence.atomic_json(path, {"status": "running"})
            evidence.atomic_json(path, {"status": "failed"})
            self.assertEqual(json.loads(path.read_text()), {"status": "failed"})
            self.assertEqual(list(Path(directory).iterdir()), [path])

    def test_timeout_reaps_own_child_process(self):
        with tempfile.TemporaryFile() as stream:
            with self.assertRaises(subprocess.TimeoutExpired):
                baseline.execute([sys.executable, "-c", "import time; time.sleep(10)"],
                                 Path.cwd(), os.environ.copy(), stream, 0.05)

    def run_fixture(self, root, exit_code=0, timed_out=False):
        repository = root / "repository"
        fixture = repository / "clients/macos/Tests/ChatOSConnectorTests/AgentGroupChatStorePerformanceBaselineTests.swift"
        fixture.parent.mkdir(parents=True)
        fixture.write_text("fixture source")
        source = {"git_commit": "abc", "input_sha256": "inputs", "status_sha256": "dirty", "index_sha256": "index"}

        def execute(command, repository, environment_values, stream, timeout):
            stream.write((baseline.MARKER + json.dumps(self.valid_result()) + "\n").encode())
            if timed_out:
                raise subprocess.TimeoutExpired(command, timeout)
            return exit_code

        with patch.object(baseline, "capture", return_value={"source": source, "started_utc": "start", "profiles": {"swift": "release", "rust": "release"}}), \
             patch.object(baseline, "source_snapshot", return_value=source), \
             patch.object(baseline, "environment", return_value={}), \
             patch.object(baseline, "test_product", return_value={"sha256": "test-binary"}), \
             patch.object(baseline, "execute", side_effect=execute):
            return baseline.run_baseline(repository, root / "results", 3, 1)

    def test_three_round_manifest_records_artifact_and_raw_samples(self):
        with tempfile.TemporaryDirectory() as directory:
            path = self.run_fixture(Path(directory))
            result = json.loads(path.read_text())
            self.assertEqual(result["status"], "completed")
            self.assertEqual(len(result["rounds"]), 3)
            self.assertEqual(result["rounds"][0]["test_product"]["sha256"], "test-binary")
            self.assertEqual(result["summary_milliseconds"]["openStoreMilliseconds"]["samples"], 9)
            self.assertIsNone(result["dataset_byte_sha256"])
            self.assertEqual(result["performance_acceptance"], "not-evaluated")

    def test_failed_and_timed_out_runs_preserve_logs_and_failure_manifest(self):
        for timed_out in (False, True):
            with self.subTest(timed_out=timed_out), tempfile.TemporaryDirectory() as directory:
                with self.assertRaises((ValueError, subprocess.TimeoutExpired)):
                    self.run_fixture(Path(directory), exit_code=1, timed_out=timed_out)
                result = json.loads((Path(directory) / "results/run.json").read_text())
                self.assertEqual(result["status"], "failed")
                self.assertIn("finished_utc", result)
                self.assertIn("log_sha256", result["rounds"][0])
                self.assertNotIn("summary_milliseconds", result)


if __name__ == "__main__":
    unittest.main()
