# Chatos Browser Bridge Extension

Manifest v3 extension for explicitly sharing existing Chrome tabs with Chatos Browser MCP.

Development:

```sh
npm test
npm run build
npm run test:chrome
```

Load the generated `dist/` directory as an unpacked extension. Production packaging must inject or
verify the fixed Chrome Web Store extension identity expected by Browser MCP. The extension requires
the `ai.chatos.browser_bridge` native messaging host installed by the Browser MCP binary.

The extension never connects directly to an MCP or remote service. Initial pairing is initiated only
by the popup. After pairing, the extension retries the local native bridge every two seconds without
showing another prompt, so MCP or extension restarts recover automatically. Choosing **Disconnect**
in the popup clears the pairing and stops automatic reconnection.

Explicitly shared tabs are held in service-worker memory so an extension restart fails closed. Tabs
created by an MCP task can be placed in that task's named native Chrome tab group; ending the task
keeps the tabs available for review and collapses the group.
