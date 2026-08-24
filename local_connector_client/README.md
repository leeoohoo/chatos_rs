# Local Connector Client

This directory contains the local-side Connector implementation.

Current status:

1. `core` is a Rust local daemon.
2. `frontend` is the local React client UI for login, Marketplace Plugin management, terminal testing, and permission controls.
3. The daemon registers a device against `local_connector_service`.
4. It automatically registers a device-scoped local filesystem route while preserving existing workspace mappings for project compatibility.
5. It opens an outbound WebSocket to Local Connector Service.
6. It handles Marketplace Plugin/MCP, terminal PTY, terminal exec, and permission-lease relay messages from Local Connector Service.
7. One owner can hold only one active Local Connector session lease; a second client is rejected with `409 connector_already_active`.
8. The installer contains no built-in document, PDF, Computer Use, or similar capability packages. Capabilities are installed only from approved Plugin Management releases as pinned npm MCP packages.

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

The script detects Apple Silicon versus Intel, builds the desktop resources, and writes a DMG under:

```text
local_connector_client/dist/electron-macos/
```

Before accepting the DMG, packaging runs `verify-installed-package.mjs` against the final `.app/Contents/Resources` directory. It checks executable architecture, critical non-symlink paths, the Chrome extension and browser runtime, confirms removed built-in capabilities are absent, and validates resource-tree safety. A redacted JSON report is written beside the DMG as `*.dmg.verification.json`.

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

Before creating the archive, the script verifies final resources without launching the app or opening a port. Marketplace MCP plugins are installed separately into the client's managed npm package directory. Core and Native Host binaries must match the package architecture; Windows ARM64 explicitly permits the pinned x64 browser runtime through Windows x64 emulation. It writes:

1. `local_connector_client/dist/electron-windows/Chat OS Local Connector/Chat OS Local Connector.exe`
2. `local_connector_client/dist/electron-windows/Chat-OS-Local-Connector-windows-<architecture>.zip`
3. `local_connector_client/dist/electron-windows/Chat-OS-Local-Connector-windows-<architecture>.zip.verification.json`

An existing unpacked app can be checked again without rebuilding it. Supply its final resources directory to the standalone verifier:

```bash
node local_connector_client/verify-installed-package.mjs \
  --platform macos-arm64 \
  --resources "/Applications/Chat OS Local Connector.app/Contents/Resources" \
  --report /tmp/chatos-installed-package-verification.json \
  --require-signed
```

Use `windows-x64` or `windows-arm64` and the unpacked app's `resources` directory on Windows; omit `--require-signed`, which is the macOS Developer ID contract.

### Linux

Run the Linux packager on the target Linux architecture:

```bash
./local_connector_client/package-electron-linux-client.sh
```

The script detects `linux-arm64` versus `linux-x64`, builds the React frontend and native Rust
executables on Linux, verifies that removed built-in capabilities are absent, and creates a DEB under:

```text
local_connector_client/dist/electron-linux/
```

Linux ships the `linux-core` runtime profile: the desktop app, Local Connector Core, bundled `rg`,
and SQLite migrations. Browser, Computer Use, document, PDF, and similar capabilities are not
desktop-bundled runtimes; they must be installed as approved Marketplace npm MCP plugins. The final
unpacked resources are checked by `verify-installed-package.mjs`, and a redacted
`*.deb.verification.json` report is written beside the DEB.

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
authentication, opening the local settings surface, and restricted Local Connector runtime calls
for workspace selection and generic Plugin Runtime operations. Local approvals, permission policy, and device
administration remain in the local shell and core.

The UI supports:

1. Login/register through `user_service`.
2. Device registration with `local_connector_service`.
3. Local directory browsing and multi-directory grants.
4. Terminal relay testing through `local_connector_service`.
5. Native local-process readiness, policy controls, and lease handling in the Local Connector core.
6. Marketplace npm MCP installation, activation, permission, credential, and availability status.
7. A persistent developer-mode switch. It uses local ChatOS (`127.0.0.1:8088`), Local Connector Service (`127.0.0.1:39230`), and User Service (`127.0.0.1:39190`) with a separate Electron cookie partition; the local development stack continues to use the configured online MinIO endpoint by default.
8. Settings render inside the main Electron window instead of a second macOS Space-aware window. The hosted ChatOS surface continues to live in its own `WebContentsView`; main/settings views are restored and repainted when the app becomes active, and a failed ChatOS renderer is recreated automatically.
9. The system-permissions panel reports device-owned workspace, process, network, Accessibility, Screen Recording, and Office Automation readiness used by installed MCP plugins and local runtime controls. Browser-specific permissions and Extension authorization are owned by the installed Browser MCP.
10. The signed macOS app declares Apple Events automation and user-selected Desktop/Documents/Downloads/network-volume/removable-volume usage descriptions. Accessibility is requested through Electron, while Screen Recording and other privacy categories link to the matching macOS settings pages.

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
```

Signed remote-control trust is managed by configuration center and delivered to the desktop client
through `GET /api/local-connectors/config/runtime`. The client rejects MCP/terminal/plugin/lease
commands unless they carry a trusted Ed25519 platform signature from that managed control-plane
bundle.

Command Approval Agent 完整保留在客户端本地：模型循环只使用本地只读项目工具和本地 `approval_decision`，风险判断、人工确认、白名单、Session Approval 与审批历史也都在设备侧完成。它不会创建服务端 MCP Runtime Session，也不会在服务端调用 MCP 工具。Agent Prompt、能力策略和 Memory 可继续通过普通受认证 REST 获取，但这些控制面请求不会成为工具调用链。

The local state file stores `device_id` and the local-only mapping from server-issued `workspace_id` to an absolute local root. The backend service only stores the alias and fingerprint.

Terminal support:

1. Chat OS creates local connector terminals with `cwd=local://connector/{device_id}/{workspace_id}`.
2. Chat OS proxies `/api/terminals/{id}/ws` to `local_connector_service`.
3. The service sends `terminal_session_create_request`, `terminal_input`, `terminal_resize`, `terminal_snapshot_request`, and `terminal_close` over the Connector outbound WebSocket.
4. The local core starts a PTY shell inside the authorized workspace and streams `terminal_output`, `terminal_snapshot`, `terminal_state`, and `terminal_exit` events back through the same connection.

Terminal exec remains available for MCP tools and relay diagnostics:

1. ChatOS, Task Runner, or MCP Management calls `POST /api/local-connectors/relay/{device_id}/terminal/exec`.
2. The service forwards a `terminal_exec_request` through the outbound WebSocket.
3. The client runs `command` plus `args` directly inside the authorized workspace. It does not use shell expansion by default.
4. Optional `cwd` must still resolve inside the authorized workspace.
5. The response includes `exit_code`, `success`, `stdout`, `stderr`, timeout state, and truncation flags.

The compatibility lease facade remains a Local Connector authorization and pairing boundary. Task Runner still sends `lease_request` messages for wire compatibility through the outbound Connector WebSocket, and the client binds each lease to the authorized local workspace. MCP calls then execute through the Connector's existing workspace, terminal, browser, and plugin runtimes; no standalone sandbox MCP process is packaged or started, and release has no copied workspace to export. The relay facade never creates a server-side project workspace and never calls a user-machine localhost address.
