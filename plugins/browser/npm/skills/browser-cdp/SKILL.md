---
name: browser-cdp
description: Route Browser CDP work to the smallest specialist workflow for navigation, interaction, observation, network diagnostics, or file transfer while preserving the task-owned browser session.
metadata:
  chatos.role: router
  chatos.related-skills: "browser-navigation,browser-interaction,browser-observation-verification,browser-network-debugging,browser-file-transfer"
---

# Browser CDP

Use Browser CDP for goal-oriented work in a real or managed Chrome session. Users describe outcomes; do not ask them to translate the request into tool calls.

## Runtime invariants

- Open or reuse the task-owned session with `browser_session_open`. Never invent or pass a browser session ID; the plugin binds it as program state.
- Read the returned top-level `mode`. `chrome_extension` means the paired user Chrome; `managed` means the isolated fallback. Never guess which browser is active.
- A navigation, tab switch, reload, modal transition, or DOM-changing action invalidates old element refs. Acquire a fresh snapshot.
- Prefer high-level tools. Use raw CDP only when a high-level operation cannot express the requested read or interaction.
- Verify meaningful outcomes. A successful click or navigation response is not proof that the intended page state exists.

## Route only what is needed

- Activate `browser-navigation` for URL selection, search, redirects, tabs, and navigation recovery.
- Activate `browser-interaction` for clicking, typing, forms, scrolling, and stale-ref recovery.
- Activate `browser-observation-verification` for snapshots, screenshots, extraction, and completion evidence.
- Activate `browser-network-debugging` for console, requests, WebSocket, HAR, interception, or raw CDP diagnostics.
- Activate `browser-file-transfer` for uploads, downloads, chooser handling, and artifact delivery.

Use this activation as `parent_activation_ref` when activating a leaf. Activate only leaves that change the execution decisions for the current request.

## Platform Skill protocol

1. Activate this router with `skill_skill_activate` using its catalog `skill_ref`.
2. Choose the smallest leaf from the routing list above and activate it with this router's `activation_ref` as `parent_activation_ref`.
3. Pass both returned `activation_evidence` tokens in the business tool's `skillEvidence` array. Never invent, reuse across tasks, or edit an evidence token.
4. Read a leaf reference only through `skill_skill_list_resources` and `skill_skill_read_resource` with that leaf's activation reference and evidence.

## MCP tool directory

- Session lifecycle: `browser_session_open`, `browser_session_status`, and `browser_session_close` create, inspect, and end the task-owned browser session.
- Tabs and navigation: `browser_tabs`, `browser_tab_new`, `browser_tab_switch`, `browser_tab_close`, and `browser_navigate` manage destinations and invalidate stale refs after changes.
- Observation: `browser_snapshot`, `browser_find`, `browser_screenshot`, and `browser_wait` locate current state and gather completion evidence.
- Interaction: `browser_click`, `browser_type`, `browser_fill_form`, `browser_press`, `browser_scroll`, and `browser_handle_dialog` act only on freshly observed state.
- Files: `browser_upload` supplies an authorized workspace file; `browser_downloads` observes and returns completed download artifacts.
- Console and network: `browser_console`, `browser_network`, and `browser_network_request` inspect bounded diagnostics.
- HAR: `browser_har_start` begins a bounded capture and `browser_har_stop` returns and ends it.
- WebSocket: `browser_websocket_start`, `browser_websocket_events`, and `browser_websocket_stop` control a bounded frame stream.
- Request routing: `browser_route_add`, `browser_route_list`, `browser_route_remove`, and `browser_route_clear` manage explicit temporary interception rules and must be cleaned up.
- Raw CDP: `browser_cdp_targets`, `browser_cdp_attach`, `browser_cdp_detach`, `browser_cdp_send`, `browser_cdp_subscribe`, `browser_cdp_events`, and `browser_cdp_unsubscribe` are last-resort protocol tools; detach and unsubscribe when finished.

## Completion

Continue until the requested visible or data outcome is verified. Close the session when no further browser work is expected. On a recoverable session failure, reopen once and replay from the last verified state; do not create an unbounded retry loop.
