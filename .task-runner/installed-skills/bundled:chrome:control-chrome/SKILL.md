---
name: control-chrome
description: "Control the user's Chrome browser for tasks that depend on existing Chrome state: tabs, logged-in sessions, or extensions. Prefer purpose-built connectors, APIs, or CLIs when available."
---

# Browser
Use this skill for browser automation tasks such as inspecting pages, navigating, testing local apps, clicking, typing, taking screenshots, and reading visible page state.

If this plugin is listed as available in the session, treat that as mandatory reading before browser work. Open and follow this skill before saying that Browser is unavailable and before falling back to standalone Playwright or Computer Use.

Do not skip this skill just because Computer Use MCP tool calls are directly visible or appear easier to invoke. The presence of Computer Use tools is not evidence that Computer Use is the preferred browser surface.

## Setup Documentation
Use `await agent.documentation.get("<name>")` when one of these setup topics applies:
- `bootstrap-troubleshooting`: read when browser setup succeeds but discovery or selection fails
- `chrome-troubleshooting`: read when Chrome extension setup, installation, or communication fails

## Bootstrap
These setup details are internal. User-facing progress updates should be less technical in nature. Never mention `Node REPL`, `node_repl`, `REPL`, JavaScript sessions, module exports, reading documentation, or loading instructions unless a user is asking for that exact information. If setup or recovery is needed, describe it naturally as connecting to the browser or retrying the browser connection.

The `browser-client` module is the core entry point for browser use, and is available under `scripts/browser-client.mjs` in this plugin's root directory. ALWAYS import it using an absolute path. IMPORTANT: If this path cannot be found, stop and report that this plugin is missing `scripts/browser-client.mjs`. NEVER use the built in `browser-client` library.

Run browser setup code through the Node REPL `js` tool. In this environment the callable tool id typically appears as `mcp__node_repl__js`. If it is not already available, use tool discovery for `node_repl js` without setting a result limit. You need the `js` execution tool: `js_reset` only clears state, and `js_add_node_module_dir` only changes package resolution. Do not call either helper while trying to expose `js`. If `js` is still not available, search again for `node_repl js` with `limit: 10`.

Initialize the runtime once per fresh Node session. If `agent.browsers` already exists, reuse it; do not import or initialize another browser runtime.

```js
if (globalThis.agent?.browsers == null) {
  const { setupBrowserRuntime } = await import("<plugin root>/scripts/browser-client.mjs");
  await setupBrowserRuntime({ globals: globalThis });
}
```

## Browser selection
Select the initial browser with exactly one of these scenarios, in the order
shown. An explicit request for the in-app browser or Chrome always wins over URL
selection. Never call `getForUrl()` when the user names a browser.

Use Chrome when the user explicitly requests it or the task requires an existing Chrome tab, logged-in session, profile, or extension. Do not switch to Chrome solely because a preferred connector, API, or CLI has missing or expired authentication; ask the user to fix authentication or explicitly approve Chrome as a fallback.

Do not inspect browser cookies, local storage, profiles, passwords, or session stores. Browser discovery must remain read-only.

When authentication blocks requested browser navigation, do not replace it with web search, a search engine, another site, or another source merely to bypass sign-in.

### The user explicitly requests a browser
The in-app browser is available only when the Browser skill is listed for the session. If the user explicitly requests the in-app browser and that skill is available, use a distinct persistent binding and immediately read its complete documentation:

```js
globalThis.iab = await agent.browsers.get("iab");
nodeRepl.write(await iab.documentation());
```

If the user explicitly requests the in-app browser but the Browser skill is not available, report that the in-app browser is unavailable instead of substituting another browser.

Chrome is available only when the Chrome skill is listed for the session. If the user explicitly requests Chrome and that skill is available, use a separate persistent binding and immediately read its complete documentation:

```js
globalThis.chrome = await agent.browsers.get("extension");
nodeRepl.write(await chrome.documentation());
```

If the user explicitly requests Chrome but the Chrome skill is not available, report that Chrome is unavailable instead of substituting another browser.

An explicit browser choice remains in force for the task. If authentication blocks the task in an explicitly selected browser, your next response must explicitly ask the user to sign in in that browser and tell you when it is ready, unless that browser's documentation provides a supported authentication flow to try first. Merely reporting that sign-in is required is not sufficient. Do not switch to another browser unless the user asks or approves the switch.

### The user does not specify a browser and the task has a target URL
When the user supplies a URL or the intended URL can be reasonably inferred from the request, replace the example below with that URL and let browser-client choose the browser best suited to it:

```js
globalThis.browser = await agent.browsers.getForUrl("https://example.com/");
nodeRepl.write(await browser.documentation());
```

This runtime-selected browser is not a user constraint. If the page requires authentication and another available browser may have the needed session, try that browser before asking the user to sign in.

### The user specifies neither a browser nor a target URL
Use the runtime default, which prefers the in-app browser when it is available and otherwise uses Chrome. Do not list browsers first:

```js
globalThis.browser = await agent.browsers.getDefault();
nodeRepl.write(await browser.documentation());
```

## After setup
If setup succeeds but browser discovery or selection fails, read `await agent.documentation.get("bootstrap-troubleshooting")` before resetting the JavaScript session or trying another browser-control mechanism.

If the failure is specific to Chrome extension setup, installation, or communication, read `await agent.documentation.get("chrome-troubleshooting")` before retrying or taking another recovery action.

When the user did not explicitly choose a browser, you may select another browser later without resetting the Node session. Preserve existing `iab`, `chrome`, and `browser` bindings when they are still useful. Existing tabs remain bound to the browser that created them. After selecting another browser, obtain a tab from that browser before continuing and read its complete documentation.

The ability to interact directly with browsers is exposed through the `browser-client` runtime via the `agent.browsers.*` API. Before trying to interact with a selected browser, you MUST emit and read the complete documentation returned by its `documentation()` call in one go. For the initial documentation read, run the exact direct `nodeRepl.write(await <browser>.documentation());` call shown in the applicable scenario above. Do not assign the documentation to a variable, inspect its length, slice it, truncate it, summarize it, or emit only an excerpt. Do not proactively split the documentation into pages or chunks. Only if the tool output itself explicitly reports that it was truncated may you emit and read smaller chunks until you have read the documentation in its entirety.

Only the Node REPL `js` tool (`mcp__node_repl__js`) can be used to control the selected browser. Do not use external MCP browser-control tools, separate browser automation servers, or other browser skills for this surface. References to Playwright mean the in-skill `tab.playwright` API after browser-client setup.
