---
name: browser-network-debugging
description: Diagnose browser console, HTTP, WebSocket, HAR, and CDP behavior with a narrow observation scope and explicit authorization for interception or raw commands.
metadata:
  chatos.role: leaf
---

# Browser network debugging

Start with passive observation: console output, failed requests, request/response metadata, WebSocket events, or HAR capture. Narrow by host, resource type, time window, or request pattern before collecting large traces.

Use raw CDP only when a high-level tool cannot expose the needed state. Name the CDP domain and method, keep parameters minimal, and avoid credential-, cookie-, storage-, or token-reading commands unless the user explicitly requested that sensitive inspection and the host authorizes it.

Interception, request abortion, response mocking, and cache changes alter page behavior. Treat them as mutations, scope them narrowly, verify that the rule is active, and remove or disable them after the diagnostic when possible.

Read [network examples](references/examples.md) before interception or raw CDP work.
