# ChatOS Local Browser Control

This Bundle uses the ChatOS-owned Local Connector BrowserTools bridge. It does not contain or redistribute Codex's proprietary in-app browser implementation.

- The desktop installer includes pinned `agent-browser 0.31.2` and Chrome for Testing runtimes. Users do not need to install npm, run `npx`, or execute `agent-browser install`.
- The dependency check fails closed when the packaged runtime is incomplete. `AGENT_BROWSER_BIN` and `AGENT_BROWSER_EXECUTABLE_PATH` remain development overrides only.
- Use the published browser tools for navigation, inspection, clicking, typing, scrolling, console/network observation, research, and bounded workspace file transfer. Refresh page snapshots after navigation or large DOM changes.
- `browser_upload` accepts 1-10 existing workspace-relative regular files. It rejects symlinks, files over 50 MiB, totals over 100 MiB, traversal, absolute paths, and paths outside the selected workspace.
- `browser_download` clicks a fresh element ref and writes at most 100 MiB to a new workspace-relative file. The parent directory must already exist. Existing paths are never overwritten; staging and failed outputs are removed.
- Browser commands execute from the authorized workspace and use a run-scoped conversation/session identity.
- The ChatOS managed-session panel exposes the same upload/download contract using workspace-relative paths; it does not grant arbitrary host filesystem access.
- Do not claim browser vision is available unless the prepared tool list explicitly includes it.
