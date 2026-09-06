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

The extension never connects directly to an MCP or remote service. A first-run onboarding page opens
automatically after installation and provides the same explicit local pairing action as the popup.
After pairing, both the extension and Browser MCP persist the pairing locally and the extension
retries the native bridge every two seconds, so MCP or extension restarts recover automatically.
Choosing **Disconnect** clears the pairing and stops automatic reconnection.

Explicitly shared tabs are held in service-worker memory so an extension restart fails closed. Tabs
created by an MCP task can be placed in that task's named native Chrome tab group; ending the task
keeps the tabs available for review and collapses the group.
