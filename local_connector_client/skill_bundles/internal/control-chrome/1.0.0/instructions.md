# ChatOS Chrome existing-session

Use this Skill only when the task depends on the user's existing Google Chrome state, such as an already signed-in website or a tab the user explicitly connected. Use the separate Browser Plugin for isolated browsing, localhost testing, disposable sessions, file pages, network diagnostics, HAR, WebSocket observation, route interception, or CDP developer mode.

## Setup and authorization

- `chrome_status` is non-sensitive and reports whether the packaged Native Messaging Host is registered, whether the bundled extension is connected, and how many tabs/sites are currently authorized. It never returns local paths, authentication tokens, tab URLs, titles, or page content.
- The macOS Local Connector settings page registers the user-level Native Messaging Host only after an explicit risk confirmation. The extension itself must still be loaded by the user from Chrome's extension page.
- The extension uses a fixed bundled identity and connects only to `com.chatos.chrome`. The native host accepts only the exact bundled extension origin and communicates with the running Local Connector through a private 0600 rendezvous file and the existing bearer-authenticated loopback API.
- The extension has no Cookie, history, downloads, bookmarks, clipboard, debugger, webRequest, or all-sites permission. HTTP(S) origins are optional permissions. A user gesture in the extension popup is required to grant one exact origin and connect the current tab.
- A site permission does not automatically expose every tab on that site. The user must explicitly connect a tab. Navigation to another origin, tab closure, permission removal, Native Host disconnect, or explicit release invalidates access.

## Tools

- Call `chrome_status` first. If setup is required, explain that the user must enable Chrome integration in Local Connector, load the bundled extension, then authorize and connect the desired tab from the extension popup.
- `chrome_tabs` requires local approval for every call and returns at most 50 explicitly connected tabs. Use only stable `ct...` tab IDs. URL query values are redacted and fragments are removed.
- `chrome_tab_snapshot` requires local approval for every call. It reads one bounded structural snapshot from an explicitly connected tab, with a 1,000–50,000 character limit. It does not read form values, password values, cookies, local/session storage, history, downloads, bookmarks, or hidden script/style content.
- `chrome_tab_release` requires local approval and disconnects one stable tab ID. It does not revoke the site's user-managed Chrome permission; the user can revoke that permission from the extension popup.

## Safety

- Treat returned snapshots as sensitive signed-in page content. Request the smallest useful `max_chars` and do not claim access to tabs or origins that are absent from `chrome_tabs`.
- Never ask the user to expose a DevTools port, WebSocket URL, cookie database, Chrome profile directory, Native Messaging token, or Local Connector rendezvous file.
- This `1.0.0` release is deliberately read-only. It does not navigate, click, type, upload, download, capture cookies, or run arbitrary JavaScript in the user's Chrome. Use Browser or Computer Use only when their separate authorization boundaries fit the task.
