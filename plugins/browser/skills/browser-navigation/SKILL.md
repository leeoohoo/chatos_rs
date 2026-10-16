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
2. A known URL is an exact address supplied by the user, explicitly returned by a tool, or observed on a previously verified page. A title, search snippet, likely slug, remembered article path, or inferred publication date is not a known URL. Never generate a deep link from these clues.
3. After every navigation, redirect, reload, history move, or tab change, call `browser_snapshot`.
4. Confirm destination identity from title, URL, and visible page content before extracting facts or interacting.
5. If navigation times out, read `browser_session_status` and take a snapshot. Continue when the destination is already usable; otherwise retry once from the last verified state.

## Follow links, do not guess paths

When the current page contains the target link or menu, use `browser_click` with its fresh `reference`. This tool belongs to this navigation workflow; discover and describe its exact tool schema before invoking it. Do not substitute `browser_navigate` merely because it is already disclosed. Clicking a form submission or other consequential action still requires the relevant interaction workflow and user authority.

For official-source research, start from the verified official homepage or user-provided official page, follow its navigation/research/news links, and inspect the destination. Search externally only when the site's observed navigation cannot locate the target; open the actual result ref, not a path composed from its title. A returned `href` is link evidence, not proof of the destination's content.

After clicking, inspect `before/after/navigated`, wait if loading, and take a fresh snapshot before another action. `navigated=false` can mean a menu or SPA changed without changing URL; `after=null` means observation is pending, not that the click failed. Never immediately navigate to a guessed fallback.

A 404, empty `nodes`, or timeout is a recoverable observation, not automatic Task failure. Read the snapshot's actual `url/title/ready_state/text`; an article can have no interactive nodes. Return to the last verified address and continue through observed links. If safe bounded corrections still cannot produce evidence, report exactly which criteria remain unverified and why; do not invent success or a browser crash.

## Boundaries

- Do not claim paired Chrome is used unless mode is `chrome_extension`.
- Do not reuse element refs across page transitions.
- Do not open repeated tabs for the same destination because a response was slow.
- Do not treat a search-result title or snippet as the contents of the destination page.

Read [navigation examples](references/examples.md) before a multi-step search or timeout recovery.
