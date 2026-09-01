# Chatos Browser CDP

Native Rust stdio MCP for isolated Chrome/Chromium automation and bounded raw Chrome DevTools
Protocol access. The npm package is a platform launcher only; it never downloads a browser or
executable during installation.

The current `0.1.7` milestone supports managed Chrome, bounded Console/Network/
WebSocket observation, HAR export, safe request routes, dialog handling, grant-only uploads,
artifact-confined downloads, raw CDP commands, and an authenticated Existing-Chrome backend. The
Existing-Chrome path requires the Chatos Browser Bridge Chrome extension. The installed Browser MCP
binary owns the loopback Bridge, Native Messaging Host registration, pairing credentials, and
cleanup; it does not depend on a browser-specific Local Connector service.

For existing-Chrome installation, first install this plugin from the ChatOS Marketplace, then
install **Chatos Browser Bridge** from the Chrome Web Store. Start one Browser CDP task in ChatOS
and click **首次连接** once in the extension popup. Subsequent MCP or extension restarts reconnect
automatically. The full Chinese installation, update, and troubleshooting guide is maintained in
`docs/user-installation-guide.zh-CN.md` in the source distribution.
