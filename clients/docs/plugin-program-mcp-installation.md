# Program MCP installation (3.0.9.8)

The dedicated installation assistant can now inspect a public GitHub program repository even when it contains no Plugin/Skill/MCP manifest. Existing Marketplace, npm MCP, Plugin, Skill and Hook installation paths remain available.

## Assistant flow

1. Query installed records to distinguish a new installation from an update.
2. `plugin_installer_discover_repository` reads the README, documentation paths and latest release assets. `document_path` and optional `reference` read a specific installation/MCP document.
3. Choose a macOS CLI archive for the current CPU or a universal archive. `plugin_installer_resolve_release` pins a tag and asset, verifies the published size/digest, calculates SHA-256, and lists archive entries without executing the program.
4. Pass `source_kind=github_release`, the repository URL, `exact_version=tag`, and structured `program` launch facts to `plugin_installer_inspect_source`. The installer generates a managed wrapper, starts the actual executable, initializes MCP and validates its tool list.
5. Pass the identical source and returned fingerprint to `plugin_installer_install_source`. The installer reacquires and checks the package and tools before committing. Updates also pass the original `replacing_plugin_id`; source, launch configuration and provenance are persisted for later updates and uninstall.

`local_executable` instead copies an explicitly selected absolute executable path into managed storage. It uses the same inspection, fingerprint, transaction and runtime pipeline. Applications needing sibling libraries/resources should use a complete release archive. The UI provides release-archive and installed-program entries as well as the existing repository import.

Documents are untrusted metadata, not user instructions or installation permission. These tools do not expose an arbitrary shell. Release downloads are limited to assets belonging to the specified public GitHub repository/tag; supported formats are zip, tar.gz and tgz. Extraction rejects path escapes, links, special files and excessive sizes. Cargo/uv source builds, GUI installers and new authentication flows are outside this first version.

## Verification

`NativeProgramMCPInstallationTests` exercises document discovery, real MCP initialization, argument boundaries and literal filenames, managed local installation, source drift rejection, release origin/digest validation, archive checks, in-place updates, persistence, component discovery and uninstall.

The opt-in Craft Anything acceptance test resolves the release and executable entry from repository metadata, inspects and installs it in an isolated connector state, checks persisted integrity, discovers the component, and invokes the read-only `core_targets_list` tool. On 2026-11-12 it passed with official **v0.6.1**, **30 initially published MCP tools**, and archive SHA-256 `ddbae05f52a5526e149f656b0b44cf73a60ad9ec0a33d718f330bf231b988c2f`.

```sh
CHATOS_CRAFT_INTEGRATION=1 swift test --package-path clients/macos \
  --filter NativeProgramMCPInstallationTests
```

For an offline repeat, `CHATOS_CRAFT_ACCEPTANCE_DIR` can point at unmodified GitHub `release.json`, `readme.json`, and `craft.zip`. This substitutes transport only; release size/digest verification, real binary execution, installation and MCP calls still run. The acceptance run used this cache after downloading official metadata/archive separately because local DNS was unavailable; the archive matched GitHub's published digest. It does not write to the user's installed plugins or replace the running app.
