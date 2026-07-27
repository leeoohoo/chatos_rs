# ChatOS Chrome existing-session control

Use this Skill only when the task depends on the user's existing Google Chrome state, such as an already signed-in website or a tab the user explicitly connected. Use the separate Browser Plugin for isolated browsing, localhost testing, disposable sessions, file pages, network diagnostics, HAR, WebSocket observation, route interception, or CDP developer mode.

## Setup and authorization

- `chrome_status` is non-sensitive and reports whether the packaged Native Messaging Host is registered, whether the bundled extension is connected, and how many tabs/sites are currently authorized. It never returns local paths, authentication tokens, tab URLs, titles, or page content.
- The macOS Local Connector settings page registers the user-level Native Messaging Host only after an explicit risk confirmation. The extension itself must still be loaded by the user from Chrome's extension page.
- The extension uses a fixed bundled identity and connects only to `com.chatos.chrome`. The native host accepts only the exact bundled extension origin and communicates with the running Local Connector through a private 0600 rendezvous file and the existing bearer-authenticated loopback API. It does not open another port.
- The extension has no Cookie, history, downloads, bookmarks, clipboard, debugger, webRequest, `tabs`, or all-sites permission. HTTP(S) origins are optional permissions. A user gesture in the extension popup is required to grant one exact origin and connect the current tab.
- A site permission does not automatically expose every tab on that site. The user must explicitly connect a tab. Navigation to another origin, tab closure, permission removal, Native Host disconnect, or explicit release invalidates access.

## Observe and target

- Call `chrome_status` first. If setup is required, explain that the user must enable Chrome integration in Local Connector, load the bundled extension, then authorize and connect the desired tab from the extension popup.
- `chrome_tabs` requires local approval for every call and returns at most 50 explicitly connected tabs. Use only stable `ct...` tab IDs. URL query values are redacted and fragments are removed.
- `chrome_tab_snapshot` requires local approval for every call. It reads one bounded structural snapshot from an explicitly connected tab, with a 1,000–50,000 character limit. It does not read form values, password values, cookies, local/session storage, history, downloads, bookmarks, or hidden script/style content.
- Actionable snapshot rows contain short-lived `cr...` target IDs. A target is bound to the exact tab, origin, latest snapshot, DOM path, role/type and accessible-name fingerprint. It becomes invalid after navigation, click, text input, upload, tab release, origin change or permission removal. Capture a fresh snapshot before the next action.

## Controlled actions

- `chrome_tab_navigate` requires local approval and accepts only an absolute HTTP(S) URL on the connected tab's currently authorized exact origin. It rejects embedded credentials and cross-origin navigation. The user must separately authorize and connect a tab after moving to another origin.
- `chrome_tab_click` requires local approval and clicks one visible, enabled, unchanged target from the latest snapshot. It does not accept CSS selectors or arbitrary JavaScript.
- `chrome_tab_type_text` requires local approval and writes at most 2,000 visible Unicode characters to one safe editable target. Password/secure, disabled, readonly, file and other non-text controls are rejected. Approval and results store only character count and SHA-256, not the raw text.
- `chrome_tab_upload` requires a selected workspace and local approval. It reads one 1-byte–10-MiB regular non-symlink workspace file, sends bounded chunks through Native Messaging, verifies SHA-256 inside the isolated extension world, and assigns it only to a snapshot-bound file input. It never sends the local absolute path to Chrome.
- `chrome_tab_screenshot` requires local approval and captures only the visible viewport of the active connected tab as a bounded JPEG. The image is transient model input and is omitted from persisted structured result history. If the active tab changes during capture, the image is discarded.
- `chrome_tab_release` requires local approval and disconnects one stable tab ID. It does not revoke the site's user-managed Chrome permission; the user can revoke that permission from the extension popup.

## Cancellation and safety

- Task or Plugin cancellation sends a bounded cancel message to the extension, releases the Core pending request, tolerates a late extension result and cleans up in-progress upload buffers. Cancellation stops waiting and prevents undispatched work, but it cannot reverse a navigation or click already accepted by Chrome.
- Treat returned snapshots and screenshots as sensitive signed-in page content. Request the smallest useful snapshot and screenshot quality, and do not claim access to tabs or origins absent from `chrome_tabs`.
- Never ask the user to expose a DevTools port, WebSocket URL, cookie database, Chrome profile directory, Native Messaging token, or Local Connector rendezvous file.
- This release still does not read cookies, storage, history, downloads, bookmarks or password values; does not run model-provided JavaScript; and does not provide arbitrary selectors, CDP, network interception or cross-origin silent control.
