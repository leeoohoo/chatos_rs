#!/usr/bin/env python3
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

"""Make Compose-mounted secrets readable by the non-root Rust runtime group."""

import json
import os
from pathlib import Path
import subprocess
import sys


def main() -> None:
    release = Path(sys.argv[1]).resolve(strict=True)
    config = json.loads(subprocess.check_output([
        "docker", "compose",
        "-f", str(release / "docker/compose.yml"),
        "-f", str(release / "docker/compose.platform.yml"),
        "--env-file", str(release / "docker/bootstrap.conf"),
        "config", "--format", "json",
    ]))
    mounted_files = {
        Path(secret["file"]).resolve(strict=True)
        for secret in config.get("secrets", {}).values()
        if "file" in secret
    }
    for path in mounted_files:
        if not path.is_file():
            raise RuntimeError(f"Secret is not a regular file: {path}")
        os.chown(path, 0, 10001)
        path.chmod(0o640)
    print(f"[OK] Prepared {len(mounted_files)} mounted secrets for runtime group 10001")
    for volume_key in ("plugin-management-data", "local-connector-data"):
        volume_name = config["volumes"][volume_key]["name"]
        subprocess.check_output(["docker", "volume", "create", volume_name])
        volume = json.loads(subprocess.check_output(["docker", "volume", "inspect", volume_name]))[0]
        root = Path(volume["Mountpoint"]).resolve(strict=True)
        os.chown(root, 10001, 10001)
        for directory, directories, files in os.walk(root, followlinks=False):
            for name in directories + files:
                os.chown(Path(directory) / name, 10001, 10001, follow_symlinks=False)
        print(f"[OK] Prepared runtime ownership for volume {volume_name}")


if __name__ == "__main__":
    main()
