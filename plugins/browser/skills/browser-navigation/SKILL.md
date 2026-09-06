---
name: browser-navigation
description: Navigate and search with Browser CDP using verified session mode, current tabs, destination inspection, and bounded timeout recovery.
metadata:
  chatos.role: leaf
---

# Browser navigation

Open or reuse the task session, inspect its actual mode, then navigate toward one explicit destination or search goal.

## Workflow

1. Call `browser_session_open`; if a session is already bound, treat the returned state as authoritative.
2. Navigate to a URL only when the target is known. For discovery, use a search engine or the site's own search without stopping at result snippets.
3. After every navigation, redirect, reload, history move, or tab change, call `browser_snapshot`.
4. Confirm destination identity from title, URL, and visible page content before extracting facts or interacting.
5. If navigation times out, read `browser_session_status` and take a snapshot. Continue when the destination is already usable; otherwise retry once from the last verified state.

## Boundaries

- Do not claim paired Chrome is used unless mode is `chrome_extension`.
- Do not reuse element refs across page transitions.
- Do not open repeated tabs for the same destination because a response was slow.
- Do not treat a search-result title or snippet as the contents of the destination page.

Read [navigation examples](references/examples.md) before a multi-step search or timeout recovery.
