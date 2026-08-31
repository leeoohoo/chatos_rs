# Browser MCP Bridge protocol v1

`chrome_extension` mode uses an authenticated WebSocket relay started and owned by Browser MCP. The
MCP never connects directly to a Chrome debugging endpoint and the Chrome extension never accepts
connections from arbitrary local processes.

## Transport and authentication

- Browser MCP starts the Bridge on numeric loopback, creates a private credential file, and passes
  the endpoint and credential directly to its in-process Extension backend. The
  `CHATOS_BROWSER_BRIDGE_*` variables are development-only external-Bridge overrides.
- A credential file is a JSON object containing `token` and `expires_at_unix_ms`. On Unix it must
  be a regular, non-symlink file with no group/other permission bits (normally mode `0600`).
- The endpoint must use `ws://` with a numeric loopback address (`127.0.0.0/8` or `::1`). Userinfo,
  query parameters and fragments are rejected so credentials cannot accidentally enter URLs or
  proxy logs.
- The WebSocket subprotocol is `chatos-browser-bridge.v1`.
- The first client frame is an authentication request. No other request is accepted before it.
- The token is short-lived, single-use, scoped to the current Browser MCP process, and bound to the
  fixed extension ID. The Bridge rejects expired and replayed tokens.
- Tokens must never appear in logs, MCP results, errors, snapshots, manifests or catalog data.
- JSON messages are limited to 8 MiB. Client command frames are limited to 1 MiB.

Authentication request:

```json
{"type":"request","id":1,"method":"bridge.authenticate","params":{"protocol_version":"1.0","token":"<secret>","client":{"name":"chatos-browser-cdp","version":"0.1.0"}}}
```

Successful response:

```json
{"type":"response","id":1,"result":{"protocol_version":"1.0","connection_id":"opaque","product":"Chrome/140","user_agent":"...","capabilities":["page_control","raw_cdp"]}}
```

An authentication error closes the connection. The MCP intentionally returns a generic error and
does not forward authentication details.

## Message envelope

Requests and responses use monotonically increasing connection-local IDs:

```json
{"type":"request","id":2,"method":"bridge.listTargets","params":{}}
{"type":"response","id":2,"result":{"targets":[]}}
{"type":"response","id":2,"error":{"code":"unsupported_by_backend","message":"..."}}
```

Stable methods:

| Method | Result | Notes |
| --- | --- | --- |
| `bridge.listTargets` | `{targets: Target[]}` | Returns only tabs explicitly authorized for this adapter session. |
| `bridge.createTarget` | `{target: Target}` | Uses `chrome.tabs`; may be unsupported by policy. |
| `bridge.closeTarget` | `{}` | Cannot close a tab outside the authorized target set. |
| `bridge.attachTarget` | `{session_id}` | Bridge performs `chrome.debugger.attach`. |
| `bridge.detachTarget` | `{}` | Bridge performs `chrome.debugger.detach`. |
| `cdp.send` | `{result}` | Params include nullable `session_id`, CDP `method`, and `params`. |
| `bridge.subscribe` | `{}` | Params include a client-generated `subscription_id`, nullable `session_id`, and exact CDP method names. |
| `bridge.unsubscribe` | `{}` | Removes a Bridge subscription. |
| `bridge.close` | `{}` | Best-effort orderly shutdown; disconnect also revokes all attachments. |

`Target` has `id`, optional `title`, optional `url`, and `kind`. Target IDs and session IDs are
opaque. Browser-level CDP commands are permitted only when the extension/Bridge can implement them
faithfully. Otherwise the Bridge returns `unsupported_by_backend`; it must never synthesize a
successful response that changes semantics.

CDP events use:

```json
{"type":"event","method":"cdp.event","params":{"subscription_id":"opaque","session_id":"opaque","method":"Network.requestWillBeSent","params":{}}}
```

Connection invalidation uses:

```json
{"type":"event","method":"bridge.disconnected","params":{"reason":"extension_unavailable"}}
```

Unknown event types are ignored. Malformed frames, oversized frames, binary frames, a protocol
version mismatch, token expiry, extension disable/uninstall, or loss of pairing fail the backend
closed. Pending calls are rejected and event polling stops immediately.

## Error codes

- `unsupported_by_backend`
- `invalid_request`
- `not_found`
- `permission_denied`
- `token_expired`
- `extension_unavailable`
- `timeout`
- `backend_error`

ChatOS Plugin Runtime owns generic permission and per-call approval enforcement. Browser MCP owns
extension identity, pairing, raw-CDP validation, redaction, result limits, artifact confinement and
file-grant checks before or after Bridge calls.
