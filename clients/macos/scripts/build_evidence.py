#!/usr/bin/python3
"""Local build provenance. Does not launch the App or inspect user databases.

This records inputs, not a claim of hermetic/reproducible compilation. In particular,
ignored files and external toolchain/configuration files are not source snapshots.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import plistlib
import re
import stat
import subprocess
import sys
import tempfile
from datetime import datetime, timezone


def utc_now():
    return datetime.now(timezone.utc).isoformat(timespec="seconds")


def digest(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(",", ":"),
                                     ensure_ascii=True).encode()).hexdigest()


def file_hash(path):
    result = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            result.update(chunk)
    return result.hexdigest()


def run(command, cwd=None):
    return subprocess.check_output(command, cwd=cwd, stderr=subprocess.PIPE, timeout=60)


def probe(command):
    try:
        return {"available": True, "value": run(command).decode(errors="replace").strip()}
    except (OSError, subprocess.SubprocessError) as error:
        # Do not encode unavailable metrics as zero or a healthy thermal state.
        return {"available": False, "reason": type(error).__name__}


def native_command(command):
    """macOS tooling supports Apple Silicon only; never select an Intel fallback."""
    if sys.platform != "darwin":
        raise ValueError("This macOS build tool requires Apple Silicon (arm64)")
    try:
        silicon = run(["sysctl", "-n", "hw.optional.arm64"]).strip() == b"1"
    except (OSError, subprocess.SubprocessError) as error:
        raise ValueError("Cannot verify Apple Silicon hardware; refusing build") from error
    if not silicon:
        raise ValueError("Intel macOS is unsupported; this project requires arm64")
    return ["/usr/bin/arch", "-arm64", *command]


def signature(path):
    result = subprocess.run(["codesign", "-d", "--verbose=4", str(path)],
                            capture_output=True, timeout=60, check=True)
    return result.stderr.decode(errors="replace").strip()


def contract_versions(repository):
    versions = {}
    for name, relative, pattern in (
        ("host_protocol", "clients/local_agent_host/layers/interface/src/lib.rs",
         r"pub const LOCAL_AGENT_PROTOCOL_VERSION: u32 = (\d+);"),
        ("storage_schema", "clients/local_agent_host/layers/infrastructure/database/src/migration.rs",
         r"pub\(super\) const SCHEMA_VERSION: i64 = (\d+);"),
    ):
        path = Path(repository) / relative
        matches = re.findall(pattern, path.read_text())
        if len(matches) != 1:
            raise ValueError(f"Cannot identify {name} version; update evidence extractor")
        versions[name] = {"value": int(matches[0]), "source": relative, "source_sha256": file_hash(path)}
    return versions


def atomic_json(path, value):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    descriptor, temporary = tempfile.mkstemp(prefix=path.name + ".", dir=path.parent)
    try:
        with os.fdopen(descriptor, "w") as stream:
            json.dump(value, stream, indent=2, sort_keys=True, ensure_ascii=True)
            stream.write("\n")
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary, path)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)


def entry(root, name):
    path = root / name
    try:
        mode = path.lstat().st_mode
    except FileNotFoundError:
        return {"path": name, "kind": "missing"}
    if stat.S_ISLNK(mode):
        return {"path": name, "kind": "symlink", "target": os.readlink(path)}
    if stat.S_ISREG(mode):
        return {"path": name, "kind": "file", "executable": bool(mode & 0o111),
                "bytes": path.stat().st_size, "sha256": file_hash(path)}
    raise ValueError(f"Unsupported input (including submodules): {name}")


def source_snapshot(repository):
    repository = Path(repository).resolve()
    head = run(["git", "rev-parse", "HEAD"], repository).decode().strip()
    names = run(["git", "ls-files", "-z", "--cached", "--others", "--exclude-standard"],
                repository).split(b"\0")
    files = [entry(repository, os.fsdecode(name)) for name in sorted(set(names)) if name]
    status = run(["git", "status", "--porcelain=v1", "-z", "--untracked-files=all"], repository)
    # A symlink may point to untracked/ignored/out-of-tree content that a build follows.
    return {"git_commit": head, "dirty": bool(status),
            "status_sha256": hashlib.sha256(status).hexdigest(),
            "index_sha256": hashlib.sha256(run(["git", "ls-files", "--stage", "-z"], repository)).hexdigest(),
            "input_sha256": digest(files), "files": files,
            "has_symlinks": any(item["kind"] == "symlink" for item in files)}


def environment():
    return {
        "os": probe(["sw_vers"]),
        "hardware": probe(["sysctl", "-n", "hw.model", "hw.memsize", "hw.logicalcpu"]),
        "native_architecture": probe(native_command(["uname", "-m"])),
        "collector_architecture": os.uname().machine,
        "power": probe(["pmset", "-g", "batt"]),
        "thermal_raw": probe(["pmset", "-g", "therm"]),
    }


def capture(repository, configuration, swift_build_system):
    source = source_snapshot(repository)
    return {
        "format_version": 1, "started_utc": utc_now(), "source": source,
        "profiles": {"swift": configuration, "rust": configuration},
        "swift_build_system": swift_build_system,
        "contracts_from_source": contract_versions(repository),
        "toolchain": {name: probe(command) for name, command in {
            "swift": native_command(["swift", "--version"]), "rust": ["rustc", "-vV"],
            "cargo": ["cargo", "--version"], "xcode": ["xcodebuild", "-version"],
            "sdk": ["xcrun", "--sdk", "macosx", "--show-sdk-version"],
            "developer_directory": ["xcode-select", "-p"],
        }.items()},
        # Explicit allowlist: never dump environment variables containing credentials.
        "build_environment": {key: os.environ[key] for key in (
            "DEVELOPER_DIR", "SDKROOT", "TOOLCHAINS", "MACOSX_DEPLOYMENT_TARGET",
            "RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "CARGO_BUILD_TARGET",
            "CARGO_TARGET_DIR", "CHATOS_SWIFT_SCRATCH_PATH",
            "CARGO_PROFILE_RELEASE_DEBUG", "CARGO_PROFILE_RELEASE_STRIP",
            "CARGO_PROFILE_RELEASE_OPT_LEVEL", "CARGO_PROFILE_RELEASE_LTO",
            "CARGO_PROFILE_RELEASE_CODEGEN_UNITS",
        ) if key in os.environ},
        "environment_start": environment(),
        "limitations": ["Not a hermetic build attestation; ignored inputs and external config are not archived.",
                        "Build profiles describe recorded commands, not an optimizer proof from machine code.",
                        "Performance and distribution acceptance require separate run/signing evidence."],
    }


def inventory(root):
    root = Path(root)
    if root.is_symlink() or not root.is_dir():
        raise ValueError(f"Expected a real directory: {root}")
    # Do not traverse symlink directories, including a symlink into a user directory.
    result = []
    for directory, folders, files in os.walk(root, followlinks=False):
        for name in folders + files:
            path = Path(directory) / name
            if path.is_symlink() or path.is_file():
                result.append(entry(root, path.relative_to(root).as_posix()))
    return sorted(result, key=lambda item: item["path"])


def assert_unchanged(before, after):
    if any(before[key] != after[key] for key in
           ("git_commit", "input_sha256", "status_sha256", "index_sha256")):
        raise ValueError("Source/index changed during build; evidence rejected. Rebuild from stable inputs.")


def finish(repository, app, captured, commands, symbols):
    after = source_snapshot(repository)
    assert_unchanged(captured["source"], after)
    app = Path(app).resolve()
    with (app / "Contents/Info.plist").open("rb") as stream:
        info = plistlib.load(stream)
    if info["ChatOSBuildGitCommit"] != after["git_commit"]:
        raise ValueError("App Git commit differs from captured source")
    if info["ChatOSBuildConfiguration"] != captured["profiles"]["swift"]:
        raise ValueError("App configuration differs from captured commands")
    artifacts = inventory(app)
    binaries = {}
    for relative in ("Contents/MacOS/ChatOSSwift", "Contents/MacOS/chatos_local_agent_host",
                     "Contents/MacOS/ChatOSMediaWorker",
                     "Contents/MacOS/ChatOSCodeReadWorker",
                     "Contents/MacOS/ChatOSAgentWorker",
                     "Contents/Resources/Tools/darwin-arm64/ps", "Contents/Resources/Tools/darwin-arm64/rg"):
        path = app / relative
        if not path.is_file() or path.is_symlink():
            raise ValueError(f"Missing/invalid executable: {relative}")
        binaries[relative] = {"sha256": file_hash(path),
                              "uuid": probe(["dwarfdump", "--uuid", str(path)])}
        # codesign writes its description to stderr.
        binaries[relative]["signature"] = signature(path)
    symbol_records = {}
    for path in symbols:
        uuids = probe(["dwarfdump", "--uuid", str(path)])
        identifiers = set(re.findall(r"UUID: ([\w-]+) \(([^)]+)\)", uuids.get("value", "")))
        matches = [name for name, binary in binaries.items() if identifiers and identifiers == set(
            re.findall(r"UUID: ([\w-]+) \(([^)]+)\)", binary["uuid"].get("value", "")))]
        if len(matches) != 1:
            raise ValueError(f"dSYM UUID does not match exactly one packaged binary: {path}")
        symbol_records[str(Path(path).resolve())] = {
            "inventory": inventory(path), "uuid": uuids, "binary": matches[0]}
    return {**captured, "finished_utc": utc_now(), "source_unchanged": True,
            "environment_end": environment(), "commands": commands,
            "classification": "clean-release-build" if (
                not after["dirty"] and not after["has_symlinks"] and
                captured["profiles"] == {"swift": "release", "rust": "release"}
            ) else "diagnostic-build",
            "app": {"name": app.name, "inventory": artifacts, "inventory_sha256": digest(artifacts),
                    "version": info["CFBundleShortVersionString"], "build": info["CFBundleVersion"],
                    "binaries": binaries},
            "symbols": symbol_records,
            "symbolication_status": "uuid-matched" if symbols else "not-collected",
            "performance_acceptance": "not-run", "distribution_acceptance": "not-run"}


def verify(app, manifest, require_symbols=False):
    artifacts = inventory(app)
    if artifacts != manifest["app"]["inventory"] or digest(artifacts) != manifest["app"]["inventory_sha256"]:
        raise ValueError("App contents differ from build manifest (stale/replaced/modified bundle)")
    for path, expected in manifest["symbols"].items():
        if not Path(path).exists():
            if require_symbols:
                raise ValueError(f"Symbol files are unavailable: {path}")
            continue
        if inventory(path) != expected["inventory"]:
            raise ValueError(f"Symbol files differ from build manifest: {path}")


def signature_summary(path):
    description = signature(path)
    values = {}
    authorities = []
    for line in description.splitlines():
        if line.startswith("Authority="):
            authorities.append(line.removeprefix("Authority="))
        elif line.startswith("CodeDirectory "):
            values["CodeDirectory"] = line.removeprefix("CodeDirectory ")
        elif "=" in line:
            key, value = line.split("=", 1)
            if key in {"Identifier", "TeamIdentifier", "Timestamp", "Format", "CodeDirectory"}:
                values[key] = value
    return {
        "identifier": values.get("Identifier"),
        "team_identifier": values.get("TeamIdentifier"),
        "timestamp": values.get("Timestamp"),
        "format": values.get("Format"),
        "code_directory": values.get("CodeDirectory"),
        "authorities": authorities,
        "hardened_runtime": bool(re.search(r"^CodeDirectory .*flags=.*\bruntime\b", description,
                                            flags=re.MULTILINE)),
    }


def notarization_record(path):
    path = Path(path)
    payload = json.loads(path.read_text())
    if payload.get("status") != "Accepted":
        raise ValueError(f"Notarization evidence is not Accepted: {path}")
    submission_id = payload.get("id")
    if not isinstance(submission_id, str) or not submission_id.strip():
        raise ValueError(f"Notarization evidence has no submission id: {path}")
    return {"status": "Accepted", "submission_id": submission_id,
            "evidence_file": path.name, "evidence_sha256": file_hash(path)}


def distribution_evidence(app, build_manifest_path, dmg, app_notary, dmg_notary, signing_identity):
    app = Path(app).resolve()
    build_manifest_path = Path(build_manifest_path).resolve()
    dmg = Path(dmg).resolve()
    build_manifest = json.loads(build_manifest_path.read_text())
    if build_manifest.get("classification") != "clean-release-build":
        raise ValueError("Distribution requires a clean-release-build manifest")
    if build_manifest.get("source_unchanged") is not True:
        raise ValueError("Distribution build source was not proven unchanged")
    if build_manifest.get("profiles") != {"swift": "release", "rust": "release"}:
        raise ValueError("Distribution build profiles are not both release")
    if build_manifest.get("symbolication_status") != "uuid-matched":
        raise ValueError("Distribution requires UUID-matched symbols")

    with (app / "Contents/Info.plist").open("rb") as stream:
        info = plistlib.load(stream)
    source_commit = build_manifest["source"]["git_commit"]
    if info.get("ChatOSBuildConfiguration") != "release":
        raise ValueError("Stapled App is not a Release build")
    if not info.get("CFBundleIdentifier"):
        raise ValueError("Stapled App has no bundle identifier")
    if info.get("ChatOSBuildGitCommit") != source_commit:
        raise ValueError("Stapled App Git commit differs from build evidence")
    if (build_manifest["app"].get("version"), build_manifest["app"].get("build")) != (
            info.get("CFBundleShortVersionString"), info.get("CFBundleVersion")):
        raise ValueError("Stapled App version differs from build evidence")

    contracts = build_manifest.get("contracts_from_source")
    if not isinstance(contracts, dict) or set(contracts) != {"host_protocol", "storage_schema"}:
        raise ValueError("Build evidence is missing protocol/schema contracts")
    for name, contract in contracts.items():
        if not isinstance(contract.get("value"), int) or not contract.get("source_sha256"):
            raise ValueError(f"Malformed build contract: {name}")

    app_signature = signature_summary(app)
    dmg_signature = signature_summary(dmg)
    if signing_identity not in app_signature["authorities"]:
        raise ValueError("Stapled App signature identity differs from requested Developer ID")
    if signing_identity not in dmg_signature["authorities"]:
        raise ValueError("Stapled DMG signature identity differs from requested Developer ID")
    if not app_signature["hardened_runtime"]:
        raise ValueError("Stapled App is missing hardened runtime")
    if not app_signature["team_identifier"] or not dmg_signature["team_identifier"]:
        raise ValueError("Distribution signatures are missing TeamIdentifier")
    if app_signature["team_identifier"] != dmg_signature["team_identifier"]:
        raise ValueError("App and DMG signatures use different TeamIdentifier values")
    if not app_signature["timestamp"] or not dmg_signature["timestamp"]:
        raise ValueError("Distribution signatures are missing trusted timestamps")

    return {
        "format_version": 1,
        "created_utc": utc_now(),
        "distribution_stage": "signed-notarized-stapled",
        "clean_machine_install_acceptance": "not-run",
        "source": {"git_commit": source_commit},
        "app": {
            "filename": app.name,
            "bundle_identifier": info.get("CFBundleIdentifier"),
            "version": info["CFBundleShortVersionString"],
            "build": info["CFBundleVersion"],
            "configuration": info["ChatOSBuildConfiguration"],
            "architecture": "arm64",
            "inventory_sha256": digest(inventory(app)),
            "signature": app_signature,
        },
        "dmg": {"filename": dmg.name, "bytes": dmg.stat().st_size,
                "sha256": file_hash(dmg), "signature": dmg_signature},
        "build_evidence": {
            "filename": build_manifest_path.name,
            "sha256": file_hash(build_manifest_path),
            "classification": build_manifest["classification"],
            "symbolication_status": build_manifest["symbolication_status"],
        },
        "contracts": contracts,
        "notarization": {"app": notarization_record(app_notary),
                         "dmg": notarization_record(dmg_notary)},
        "rollback": {
            "complete_previous_app_required": True,
            "automatic_schema_downgrade_supported": False,
            "policy": ("Rollback uses a complete previous App only after verifying its maximum "
                       "supported storage schema; never clear schema markers, receipts, or user data."),
        },
    }


def main():
    if sys.platform != "darwin" or os.uname().machine != "arm64":
        raise ValueError("Use /usr/bin/arch -arm64 /usr/bin/python3; macOS arm64 only")
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)
    start = subparsers.add_parser("capture")
    start.add_argument("--repository", type=Path, required=True)
    start.add_argument("--configuration", choices=("debug", "release"), required=True)
    start.add_argument("--swift-build-system", required=True)
    start.add_argument("--output", type=Path, required=True)
    end = subparsers.add_parser("finish")
    end.add_argument("--repository", type=Path, required=True)
    end.add_argument("--app", type=Path, required=True)
    end.add_argument("--capture", type=Path, required=True)
    end.add_argument("--commands", type=Path, required=True)
    end.add_argument("--symbols", type=Path, action="append", default=[])
    end.add_argument("--output", type=Path, required=True)
    check = subparsers.add_parser("verify")
    check.add_argument("--app", type=Path, required=True)
    check.add_argument("--manifest", type=Path, required=True)
    check.add_argument("--require-symbols", action="store_true")
    record = subparsers.add_parser("record-command")
    record.add_argument("--output", type=Path, required=True)
    record.add_argument("arguments", nargs=argparse.REMAINDER)
    distribute = subparsers.add_parser("distribution")
    distribute.add_argument("--app", type=Path, required=True)
    distribute.add_argument("--build-manifest", type=Path, required=True)
    distribute.add_argument("--dmg", type=Path, required=True)
    distribute.add_argument("--app-notary", type=Path, required=True)
    distribute.add_argument("--dmg-notary", type=Path, required=True)
    distribute.add_argument("--signing-identity", required=True)
    distribute.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.command == "capture":
        atomic_json(args.output, capture(args.repository, args.configuration, args.swift_build_system))
    elif args.command == "finish":
        atomic_json(args.output, finish(args.repository, args.app, json.loads(args.capture.read_text()),
                                       json.loads(args.commands.read_text()), args.symbols))
    elif args.command == "verify":
        verify(args.app, json.loads(args.manifest.read_text()), args.require_symbols)
    elif args.command == "distribution":
        atomic_json(args.output, distribution_evidence(
            args.app, args.build_manifest, args.dmg, args.app_notary, args.dmg_notary,
            args.signing_identity))
    else:
        arguments = args.arguments
        if arguments[:1] == ["--"]:
            arguments = arguments[1:]
        records = json.loads(args.output.read_text()) if args.output.exists() else []
        records.append({"cwd": os.getcwd(), "argv": arguments, "started_utc": utc_now()})
        atomic_json(args.output, records)


if __name__ == "__main__":
    try:
        main()
    except (ValueError, KeyError, OSError, subprocess.SubprocessError) as error:
        sys.exit(f"Build evidence failed: {error}")
