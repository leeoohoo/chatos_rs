# Chatos Browser CDP MCP

This repository implements the native Rust browser MCP described in
`docs/plans/browser-cdp-mcp-development-and-publishing-spec.zh-CN.md`.

## Current milestone: managed browser plus Browser MCP owned Extension Bridge transport

Implemented:

- JSON-lines JSON-RPC/MCP lifecycle: `initialize`, `notifications/initialized`, `ping`,
  `tools/list`, `tools/call`, cancellation, shutdown, and direct-`tools/list` compatibility.
- Chromiumoxide managed Chrome backend behind a backend/factory abstraction.
- Existing-Chrome backend over an authenticated, loopback-only Browser MCP owned Bridge,
  including target/session routing, raw CDP commands, bounded events, explicit capabilities, and
  fail-closed disconnect handling.
- Manifest v3 Chrome Extension using Native Messaging bootstrap, short-lived loopback WebSocket
  authentication, explicit per-tab sharing, task-owned native Chrome tab groups, opaque
  identifiers, and `chrome.debugger` routing.
- Opaque browser, tab, CDP-session, element-reference, and artifact identifiers.
- Session/tab, navigation, snapshot/find, click/type/fill, key/scroll/wait, screenshot, and raw CDP
  tools.
- Bounded sequence-based Console, Network, WebSocket, and supported raw CDP event subscriptions.
- Sensitive network header/body redaction, HAR 1.2 artifact export, and request lookup.
- Safe request routing limited to `abort` and fixed `mock_json` responses.
- JavaScript dialog handling, download capture, and artifact registration.
- Uploads through expiring, size- and SHA-256-bound Local Connector file grants only.
- Tool policy metadata, URL/CDP input bounds, output truncation, isolated profiles, artifact output,
  and cleanup on close.
- Cross-platform npm launcher and Manifest v3 development package.

Remaining production distribution step:

- Publish Chatos Browser Bridge in the Chrome Web Store, then rebuild the Browser MCP Release with
  that store-assigned extension ID. Until that ID is embedded, existing-Chrome mode is a development
  setup; managed-browser mode and the ChatOS Marketplace package remain independently usable.

See [the Chinese user installation guide](docs/user-installation-guide.zh-CN.md) for the complete
first-run, update, troubleshooting, and publisher checklist.

## Development

```sh
cargo test --workspace
cargo run -p browser-cdp-cli -- doctor
cargo run -p browser-cdp-cli -- mcp
cd npm && npm test && npm pack --dry-run
cd ../extension && npm test
./scripts/stage-local-npm.sh
```

MCP mode writes only JSON-RPC messages to stdout. Diagnostics and logs go to stderr.

## Run with an existing Chrome profile

Build the Browser MCP and extension, then load `extension/dist` from `chrome://extensions` with
developer mode enabled. Copy the extension ID shown by Chrome. The Browser MCP binary starts the
Bridge itself and is also the Native Messaging Host executable.

```sh
cargo build -p browser-cdp-cli
cd extension && npm run build && cd ..
CHATOS_BROWSER_EXTENSION_ID=<extension-id> \
  ./target/debug/chatos-browser-cdp install-native-host <extension-id>
```

Start the MCP with the same development extension ID. Open the extension popup, select **Connect**,
then explicitly share the tab that the MCP may control:

```sh
CHATOS_BROWSER_EXTENSION_ID=<extension-id> \
  ./target/debug/chatos-browser-cdp mcp
```

In `chrome_extension` mode, `browser_session_open.session_name` becomes the native Chrome tab-group
title. Tabs created by the task join that group automatically. Explicitly shared user tabs are
never moved into it. Closing the browser session preserves and collapses the task group for review
while removing those tabs from the next task's control catalog.

Production builds compile the fixed Chrome Web Store extension ID into the Browser MCP binary.
The MCP-owned Bridge creates a fresh, single-use credential for every MCP process and cleans it up
when the process exits. `CHATOS_BROWSER_BRIDGE_*` remains only as an explicit development override.
