# ChatOS Local Browser Control

This Bundle uses the ChatOS-owned Local Connector BrowserTools bridge. It does not contain or redistribute Codex's proprietary in-app browser implementation.

- The Skill is available only when a real `agent-browser` executable is installed or supplied through `AGENT_BROWSER_BIN`; the dependency check intentionally does not treat `npx` as an installed runtime.
- Use the published browser tools for navigation, inspection, clicking, typing, scrolling, console inspection, and research. Refresh page snapshots after navigation or large DOM changes.
- Browser commands execute from the authorized workspace and use a run-scoped conversation/session identity.
- Do not claim browser vision is available unless the prepared tool list explicitly includes it.
