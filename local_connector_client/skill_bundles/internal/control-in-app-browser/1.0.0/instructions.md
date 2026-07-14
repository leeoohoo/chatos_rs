# ChatOS Local Browser Control

This Bundle uses the ChatOS-owned Local Connector BrowserTools bridge. It does not contain or redistribute Codex's proprietary in-app browser implementation.

- The desktop installer includes a pinned native `agent-browser` CLI and Chrome for Testing runtime. Users do not need to install npm, run `npx`, or execute `agent-browser install`.
- The dependency check intentionally fails closed when the packaged runtime is incomplete. `AGENT_BROWSER_BIN` and `AGENT_BROWSER_EXECUTABLE_PATH` remain available only as development overrides.
- Use the published browser tools for navigation, inspection, clicking, typing, scrolling, console inspection, and research. Refresh page snapshots after navigation or large DOM changes.
- Browser commands execute from the authorized workspace and use a run-scoped conversation/session identity.
- Do not claim browser vision is available unless the prepared tool list explicitly includes it.
