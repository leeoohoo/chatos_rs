# Local Connector Client

This directory contains the local-side Connector implementation.

Current status:

1. `core` is a Rust local daemon.
2. `frontend` is the local React client UI for login, workspace grants, terminal testing, sandbox toggling, and image creation.
3. The daemon registers a device against `local_connector_service`.
4. It stores the local-only mapping from cloud `workspace_id` to the real local root.
5. It opens an outbound WebSocket to the cloud service.
6. It handles MCP, terminal PTY, terminal exec, and sandbox relay messages from the cloud service.

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

The script detects Apple Silicon versus Intel, installs locked frontend dependencies, builds the React UI and Rust Core, bundles the matching tools, and writes a DMG under:

```text
local_connector_client/dist/electron-macos/
```

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

It writes:

1. `local_connector_client/dist/electron-windows/Chat OS Local Connector/Chat OS Local Connector.exe`
2. `local_connector_client/dist/electron-windows/Chat-OS-Local-Connector-windows-x64.zip`

To publish the ZIP to the official website's MinIO release bucket:

```powershell
$env:OFFICIAL_WEBSITE_API_BASE = "https://www.example.com"
$env:OFFICIAL_WEBSITE_RELEASE_UPLOAD_TOKEN = "replace-with-your-token"

powershell -ExecutionPolicy Bypass `
  -File .\local_connector_client\publish-release-to-minio.ps1 `
  -Version "2.0.4"
```

The publishing script computes SHA-256, uploads the ZIP through a short-lived presigned URL, and publishes the website download manifest only after the artifact upload succeeds.

The Electron desktop app starts `local_connector_client_core` as a bundled local process, loads the React UI in a desktop window, and points the UI at `http://127.0.0.1:39232` for local APIs.

The UI supports:

1. Login/register through `user_service`.
2. Device registration with `local_connector_service`.
3. Local directory browsing and multi-directory grants.
4. Terminal relay testing through `local_connector_service`.
5. Local sandbox toggle with Docker availability/running checks.
6. Sandbox image creation and sandbox lease handling in the Local Connector core through local Docker.

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
```

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
