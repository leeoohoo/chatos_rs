# Local Connector Client

This directory contains the local-side Connector implementation.

Current status:

1. `core` is a Rust local daemon.
2. `frontend` is the local React client UI for login, workspace grants, Skill enablement, terminal testing, sandbox toggling, and image creation.
3. The daemon registers a device against `local_connector_service`.
4. It stores the local-only mapping from cloud `workspace_id` to the real local root.
5. It opens an outbound WebSocket to the cloud service.
6. It handles MCP, Skill prepare/execute/cancel, terminal PTY, terminal exec, and sandbox relay messages from the cloud service.
7. One owner can hold only one active Local Connector session lease; a second client is rejected with `409 connector_already_active`.
8. The installer embeds all 28 internal Skill Bundles. Fourteen currently have implemented adapters; Browser includes its pinned native `agent-browser` and Chrome for Testing runtime, while Chrome includes a user-authorized macOS/Windows extension and user-level Native Messaging Host path with approved snapshots, same-origin navigation, short-lived target click/type/select actions, bounded scrolling/history/tab activation, workspace upload, hash-verified create-new download handoff and transient viewport capture; the other fourteen fail closed as unsupported.

## Run the Local Client

Development mode runs the Rust core and the Vite UI separately:

```bash
cargo run -p local_connector_client_core
```

The core listens on `http://127.0.0.1:39232` by default.

In another terminal:

```bash
cd local_connector_client/frontend
npm install
npm run dev
```

Open the Vite URL, usually `http://127.0.0.1:39233`.

Packaged/client mode lets the Rust core serve the built React UI itself:

```bash
cd local_connector_client/frontend
npm install
npm run build

cd ../..
cargo run -p local_connector_client_core -- --open
```

The core listens on `http://127.0.0.1:39232`, serves `frontend/dist`, and opens the client UI when `--open` or `LOCAL_CONNECTOR_OPEN_UI=1` is set.

## Package the Desktop Client

### macOS

Run the reusable packaging script on macOS:

```bash
./local_connector_client/package-electron-macos-client.sh
```

The macOS package includes a ChatOS-owned document runtime for Word/PDF tooling. The script looks for the runtime source in this order:

1. `CHATOS_DOCUMENT_RUNTIME_SOURCE`, for CI or release builds that provide a verified runtime source explicitly.
2. `local_connector_client/runtime_assets/document-runtime-source/<platform>`, for a repo- or artifact-provided ChatOS runtime source.
3. `~/Library/Caches/chatos-local-connector/document-runtime-source/<platform>`, for the local ChatOS runtime cache.

For temporary local verification only, `CHATOS_USE_CODEX_DOCUMENT_RUNTIME_SOURCE=1` imports Codex's bundled LibreOffice/Poppler source into the ChatOS cache and then packages from that ChatOS-owned cache. Official builds should provide the ChatOS runtime source directly instead of using the Codex import option.

The script validates the 28-entry Skill catalog, stages the 12 signed-control-plane Plugin Bundles with exact manifest/artifact hashes, checksums, and SPDX SBOMs, filters platform assets, verifies the staged Plugin index, detects Apple Silicon versus Intel, builds the desktop resources, and writes a DMG under:

```text
local_connector_client/dist/electron-macos/
```

Before accepting the DMG, packaging runs `verify-installed-package.mjs` against the final `.app/Contents/Resources` directory. The verifier does not launch Electron, Chrome, LibreOffice, or any service. It checks executable architecture, critical non-symlink paths, the fixed least-privilege Chrome extension, Browser and Document runtimes, document hashes, the 28-Skill/12-Plugin catalogs, the packaged Electron resource bindings, and internal resource-tree symlink/case-collision safety. A redacted JSON report is written beside the DMG as `*.dmg.verification.json`. When `CHATOS_MAC_SIGN=1`, the same pass also requires strict app/Core/helper signatures and one shared Team Identifier.

Packaging is unsigned by default so an invalid or revoked local certificate is not selected accidentally. After installing a valid `Developer ID Application` certificate, enable signing explicitly:

```bash
CHATOS_MAC_SIGN=1 ./local_connector_client/package-electron-macos-client.sh
```

Set `CSC_NAME` as well if the keychain contains more than one signing identity. Set `CHATOS_SKIP_NPM_CI=1` to reuse an already installed `node_modules` directory.

### Windows

Windows desktop packaging must run in PowerShell on Windows:

```powershell
powershell -ExecutionPolicy Bypass -File .\local_connector_client\package-electron-windows-client.ps1
```

Before creating the archive, the script stages and re-verifies the same 12 Plugin Bundles for the detected Windows architecture, then runs the same final-resources verifier without launching the app or opening a port. Core, Native Host, and Sandbox binaries must match the package architecture; Windows ARM64 explicitly permits the pinned x64 `agent-browser` and Chrome for Testing runtimes through Windows x64 emulation. It writes:

1. `local_connector_client/dist/electron-windows/Chat OS Local Connector/Chat OS Local Connector.exe`
2. `local_connector_client/dist/electron-windows/Chat-OS-Local-Connector-windows-<architecture>.zip`
3. `local_connector_client/dist/electron-windows/Chat-OS-Local-Connector-windows-<architecture>.zip.verification.json`

An existing unpacked app can be checked again without rebuilding it. Supply its final resources directory to the standalone verifier:

```bash
node local_connector_client/verify-installed-package.mjs \
  --platform macos-arm64 \
  --resources "/Applications/Chat OS Local Connector.app/Contents/Resources" \
  --plugin-catalog local_connector_client/plugin_bundles/catalog/bundled-plugin-catalog.json \
  --skill-catalog local_connector_client/skill_bundles/catalog/internal-skill-catalog.json \
  --report /tmp/chatos-installed-package-verification.json \
  --require-signed
```

Use `windows-x64` or `windows-arm64` and the unpacked app's `resources` directory on Windows; omit `--require-signed`, which is the macOS Developer ID contract.

The Windows package includes `chatos_chrome_native_host.exe` and the fixed-identity MV3 extension. Enabling Chrome integration in Local Connector requires an explicit risk acknowledgement, writes the owned manifest under the current user's ChatOS state directory, and registers only `HKCU\Software\Google\Chrome\NativeMessagingHosts\com.chatos.chrome`. Existing registry or manifest ownership conflicts fail closed; disabling removes only the exact ChatOS-owned user registration.

### Linux

Run the Linux packager on the target Linux architecture:

```bash
./local_connector_client/package-electron-linux-client.sh
```

The script detects `linux-arm64` versus `linux-x64`, builds the two React frontends and native Rust
executables on Linux, stages and verifies all 12 Plugin Bundles and 28 Skill Bundles, and creates a
DEB under:

```text
local_connector_client/dist/electron-linux/
```

Linux currently ships the explicit `linux-browser` runtime profile. It contains the desktop app,
Local Connector Core, sandbox MCP server, bundled `rg`, Plugin/Skill catalogs, SQLite migrations,
the fixed-identity Chrome extension, and the Linux native messaging host. The desktop shell loads
the hosted ChatOS web application at runtime instead of packaging a local ChatOS frontend bundle. The
client registers user-scoped manifests for both Google Chrome and Chromium when the user explicitly
enables Chrome integration. When Ubuntu's Snap Chromium is installed, the client also writes the
Snap profile manifest, copies the Native Host into the Snap-accessible user directory, and publishes
the private rendezvous file into the active Snap revision's user home. The final unpacked resources are checked by
`verify-installed-package.mjs`, and a redacted `*.deb.verification.json` report is written beside the
DEB.

The `linux-browser` profile still excludes Computer Use, `agent-browser`/Chrome for Testing, and the
bundled LibreOffice/Poppler document runtime. These features fail closed until Linux-native runtime
assets and adapters are added; the package must not be represented as equivalent to the full macOS
or Windows release profile. The verifier retains `linux-core` for validating older core-only Linux
packages.

To publish the ZIP to the official website's MinIO release bucket:

```powershell
$env:OFFICIAL_WEBSITE_API_BASE = "https://www.example.com"
$env:OFFICIAL_WEBSITE_RELEASE_UPLOAD_TOKEN = "replace-with-your-token"

powershell -ExecutionPolicy Bypass `
  -File .\local_connector_client\publish-release-to-minio.ps1 `
  -Version "2.0.4"
```

The publishing script computes SHA-256, uploads the ZIP through a short-lived presigned URL, and publishes the website download manifest only after the artifact upload succeeds.

The Electron desktop app starts `local_connector_client_core` as a bundled local process and keeps
two UI surfaces separate:

1. A minimal local React shell for pairing status, settings, approvals, permissions, and other
   device-owned controls.
2. A dedicated `WebContentsView` that loads the hosted ChatOS web application at runtime instead of
   packaging a local ChatOS frontend bundle.

The hosted ChatOS page is treated as a remote application inside a managed desktop web container,
not as the owner of desktop state. Its Electron bridge is intentionally narrow: desktop ticket
authentication, opening the local settings surface, and restricted `/api/local/runtime/*` calls
that are already scoped for Local Connector runtime use. Local approvals, permission policy, and
device administration remain in the local shell and core.

The UI supports:

1. Login/register through `user_service`.
2. Device registration with `local_connector_service`.
3. Local directory browsing and multi-directory grants.
4. Terminal relay testing through `local_connector_service`.
5. Local sandbox toggle with Docker availability/running checks.
6. Sandbox image creation and sandbox lease handling in the Local Connector core through local Docker.
7. A dedicated Skills page where Admin-provided internal Skills are visible to every user but remain disabled until the user enables them.
8. A persistent developer-mode switch. It uses local ChatOS (`127.0.0.1:8088`), Local Connector Service (`127.0.0.1:39230`), and User Service (`127.0.0.1:39190`) with a separate Electron cookie partition; the local development stack continues to use the configured online MinIO endpoint by default.
9. Settings render inside the main Electron window instead of a second macOS Space-aware window. The hosted ChatOS surface continues to live in its own `WebContentsView`; main/settings views are restored and repainted when the app becomes active, and a failed ChatOS renderer is recreated automatically.
10. The system-permissions panel derives workspace, process, browser, network, Accessibility, Screen Recording, and Office Automation mappings from the signed Skill catalog. A Skill cannot be enabled while one of its mapped capabilities is not ready; on macOS, Office Automation remains enableable because its consent prompt is issued on first use.
11. The signed macOS app declares Apple Events automation and user-selected Desktop/Documents/Downloads/network-volume/removable-volume usage descriptions. Accessibility is requested through Electron, while Screen Recording and other privacy categories link to the matching macOS settings pages.

Legacy env-driven mode is still supported:

```text
LOCAL_CONNECTOR_CLOUD_BASE_URL
LOCAL_CONNECTOR_ACCESS_TOKEN
LOCAL_CONNECTOR_WORKSPACE_PATH
LOCAL_CONNECTOR_DEVICE_NAME
LOCAL_CONNECTOR_PUBLIC_KEY
LOCAL_CONNECTOR_WORKSPACE_ALIAS
LOCAL_CONNECTOR_STATE_PATH
LOCAL_CONNECTOR_CORE_API_PORT
LOCAL_CONNECTOR_SANDBOX_DOCKER_IMAGE
LOCAL_CONNECTOR_SANDBOX_IMAGE_BUILD_CONTEXT
LOCAL_CONNECTOR_SANDBOX_IMAGE_DOCKERFILE
LOCAL_CONNECTOR_DOCKER_MAINTENANCE_ENABLED
LOCAL_CONNECTOR_DOCKER_BUILD_CACHE_MAX_USED_SPACE
LOCAL_CONNECTOR_DOCKER_BUILD_CACHE_RESERVED_SPACE
LOCAL_CONNECTOR_DOCKER_BUILD_CACHE_TIMEOUT_SECS
```

Signed remote-control trust is managed by configuration center and delivered to the desktop client
through `GET /api/local-connectors/config/runtime`. The client rejects MCP/terminal/plugin/sandbox
commands unless they carry a trusted Ed25519 platform signature from that managed control-plane
bundle.

Local Docker maintenance is program-managed and enabled by default. Managed Compose services are
tracked even when they rely on Compose's implicit `<project>-<service>:latest` image name. After a
task terminal is released, the Connector removes unused dangling Compose images only for Compose
projects whose working/config paths belong to that authorized workspace. BuildKit garbage
collection is serialized and keeps cache usage at or below `32gb` by default while reserving
`8gb`; the limits can be overridden with the variables above. No Prompt instruction or Agent-side
cleanup command is required.

Command Approval Agent 完整保留在客户端本地：模型循环只使用本地只读项目工具和本地 `approval_decision`，风险判断、人工确认、白名单、Session Approval 与审批历史也都在设备侧完成。它不会创建云端 MCP Runtime Session，也不会调用云端 MCP 工具。Agent Prompt、能力策略和 Memory 可继续通过普通受认证 REST 获取，但这些控制面请求不会成为工具调用链。

The local state file stores `device_id` and the local-only mapping from cloud `workspace_id` to an absolute local root. The cloud service only stores the alias and fingerprint.

Terminal support:

1. Chat OS creates local connector terminals with `cwd=local://connector/{device_id}/{workspace_id}`.
2. Chat OS proxies `/api/terminals/{id}/ws` to `local_connector_service`.
3. The service sends `terminal_session_create_request`, `terminal_input`, `terminal_resize`, `terminal_snapshot_request`, and `terminal_close` over the Connector outbound WebSocket.
4. The local core starts a PTY shell inside the authorized workspace and streams `terminal_output`, `terminal_snapshot`, `terminal_state`, and `terminal_exit` events back through the same connection.

Terminal exec remains available for MCP tools and relay diagnostics:

1. Cloud calls `POST /api/local-connectors/relay/{device_id}/terminal/exec`.
2. The service forwards a `terminal_exec_request` through the outbound WebSocket.
3. The client runs `command` plus `args` directly inside the authorized workspace. It does not use shell expansion by default.
4. Optional `cwd` must still resolve inside the authorized workspace.
5. The response includes `exit_code`, `success`, `stdout`, `stderr`, timeout state, and truncation flags.

Sandbox support is implemented locally by the Connector core. Task Runner calls the Local Connector relay facade, the facade sends `sandbox_request` messages over the outbound Connector WebSocket, and the client creates Docker-backed leases on the user's machine. The core rewrites `workspace_root` to the authorized local workspace's `.chatos/task-runner` directory, copies the authorized workspace into the local sandbox baseline/run workspace, starts a local Docker container that runs the sandbox MCP agent, proxies MCP calls to that local container, and exports the output manifest on release. The relay facade does not create cloud sandboxes, does not call cloud Sandbox Manager, and never calls a user-machine localhost address.
