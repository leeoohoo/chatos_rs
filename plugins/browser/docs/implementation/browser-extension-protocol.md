# Chrome Extension to Browser MCP protocol v1

The Manifest v3 extension uses Native Messaging only for authenticated bootstrap and pairing. The
high-volume CDP data plane then uses a loopback WebSocket. This keeps extension identity validation
inside Chrome's native messaging boundary while avoiding native messaging frame limits for CDP
screenshots and other large results.

## Bootstrap and pairing

The extension connects to the fixed native host `ai.chatos.browser_bridge` and sends:

```json
{"type":"request","id":"opaque","method":"extension.bootstrap","params":{"protocol_version":"1.0","pairing_requested":true}}
```

Browser MCP's Native Messaging Host receives the calling Chrome extension origin from Chrome and
compares it with the fixed production extension ID. Pairing is initiated only by the user's popup
Connect action. A successful
response contains a numeric-loopback `ws://` endpoint, a short-lived token, and its expiration:

```json
{"type":"response","id":"opaque","result":{"protocol_version":"1.0","endpoint":"ws://127.0.0.1:39001/v1/extension","token":"<secret>","expires_at_unix_ms":1787280000000}}
```

The endpoint cannot contain userinfo, a query, or a fragment. The extension never persists or logs
the endpoint token. It keeps the native port open while the WebSocket is active; either transport
closing invalidates the browser connection.

Previously paired extensions may bootstrap with `pairing_requested:false`. An unpaired request must
fail closed without silently creating a pairing. The popup's Connect action is the only operation
that sends `pairing_requested:true`.

## WebSocket data plane

The WebSocket subprotocol is `chatos-browser-extension.v1`. The first frame authenticates using the
bootstrap token:

```json
{"type":"request","id":1,"method":"extension.authenticate","params":{"protocol_version":"1.0","token":"<secret>"}}
```

The Browser MCP Bridge validates token expiry, one-time/process binding, the native bootstrap,
pairing, and extension ID. Messages are JSON and bounded to 8 MiB.

Browser MCP Bridge requests:

| Method | Description |
| --- | --- |
| `extension.listTargets` | List only tabs explicitly shared from the extension popup. |
| `extension.createTarget` | Create and authorize a new http(s) or `about:blank` tab for this connection. |
| `extension.closeTarget` | Close an authorized target. |
| `extension.attachTarget` | Attach with `chrome.debugger` and return an opaque session ID. |
| `extension.detachTarget` | Detach an extension-owned debugger session. |
| `extension.cdpSend` | Send a session-scoped CDP command. Browser-scoped commands fail explicitly. |
| `extension.subscribe` | Subscribe to exact session-scoped CDP event method names. |
| `extension.unsubscribe` | Remove an event subscription. |
| `extension.getCapabilities` | Return current protocol and capability information. |

Browser MCP Bridge translates these methods to the MCP-facing methods documented in
`browser-bridge-protocol.md`; the two sockets are separately authenticated and identifiers are
scoped to one Browser MCP process.

Extension notifications:

- `extension.cdpEvent`
- `extension.targetsChanged`
- `extension.detached`
- `extension.eventDropped`

All target and session identifiers are random and connection-local. Raw Chrome tab IDs are never
sent to MCP. Existing tabs are never shared automatically. Revoking a tab detaches the debugger,
removes its subscriptions, and immediately publishes a new target catalog.

## Failure behavior

- Native host disconnect, WebSocket close, token expiry, extension suspension, debugger detach, or
  pairing revoke closes the data plane and rejects pending work.
- `chrome://`, `chrome-extension://`, `devtools://`, `file://`, and other privileged targets cannot
  be shared or created.
- Unsupported browser-level CDP methods return `unsupported_by_backend`; no synthetic success is
  allowed.
- The extension has no content scripts, externally connectable messaging, remote code, telemetry,
  or page-accessible control channel.
