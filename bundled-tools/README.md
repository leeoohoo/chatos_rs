# Bundled Tools

Runtime shells prepend the current platform directory under this folder to
`PATH` when a bundled `rg` binary is present.

Desktop packaging also stages a pinned `agent-browser` native executable and
Chrome for Testing under the selected platform directory. These large runtime
artifacts are prepared by the platform packaging script and are not checked in.

Directory names:

- `macos-arm64`
- `macos-x64`
- `linux-arm64`
- `linux-x64`
- `windows-arm64`
- `windows-x64`

Environment overrides:

- `CHATOS_BUNDLED_TOOLS_DIR`: root directory that contains platform folders, or
  a platform directory that directly contains `rg`.
- `CHATOS_BUNDLED_TOOLS_PATH`: path-list of directories to prepend directly.

Refresh the current platform binary from a local `rg` with:

```sh
scripts/sync-bundled-ripgrep.sh
```

Set `RG_SOURCE=/path/to/rg` to copy a specific binary.

Download and refresh every supported platform from the official ripgrep release:

```sh
scripts/sync-bundled-ripgrep.sh --all
```

Download one specific platform:

```sh
scripts/sync-bundled-ripgrep.sh --platform linux-x64
```

`scripts/sync-bundled-ripgrep.sh` updates `bundled-tools/SHA256SUMS`.
CI verifies the manifest with:

```sh
scripts/check-bundled-tools-integrity.sh
```

Third-party note: `rg` is ripgrep, licensed upstream under MIT or Unlicense.
`agent-browser` is licensed under Apache-2.0. Chrome for Testing is distributed
under Google's Chrome/Chromium terms included with its archive.
