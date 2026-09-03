---
name: browser-cdp-workflow
description: Use Browser CDP for goal-oriented web browsing, page inspection, navigation, search, clicking, typing, and browser-based verification. Apply whenever a user asks to visit, find, inspect, or interact with web content through the Browser CDP plugin; users do not need to name tools or prescribe a call sequence.
---

# Browser CDP Workflow

Users normally describe the outcome they want, not the Browser CDP implementation. Expand a short request such as “去 GitHub 找 DeepSeek Harness” into the reliable browser workflow below. Do not ask the user to provide tool names or restate the request as a tool sequence.

## Reliable workflow

1. Open a browser session with `browser_session_open`, or reuse the task's still-valid session. The model-facing tool input has no `mode`, `headless`, or `persistent_profile` fields; never add or invent them. ChatOS selects the browser from verified local authorization: it uses the user's paired Google Chrome with a task-named native tab group after authorization, and automatically falls back to an isolated browser before authorization. The Browser MCP process binds the opened session internally and supplies it to later tools. Do not add, guess, request, copy, or reconstruct `browser_session_id`; it is program state, not model input. Keep optional tab IDs separate and use them only when a tool needs an explicit tab choice.
2. Read the top-level `mode` in the `browser_session_open` result before navigating. `chrome_extension` means this task is operating the user's Google Chrome and its native task tab group. `managed` means this task is using the isolated fallback because Chrome authorization is unavailable. Never guess the mode or claim that the user's Chrome is in use without this result. If the session was reused or the mode is uncertain, call `browser_session_status` and read its `mode`.
3. Navigate to the requested or reasonably inferred URL with `browser_navigate`.
4. After navigation, tab switching, or any page transition, call `browser_snapshot` before interacting. Use the newest returned element refs; navigation and later snapshots invalidate older refs.
5. Use high-level tools such as `browser_click`, `browser_type`, `browser_fill_form`, and `browser_scroll` with the current refs. Use `browser_cdp_send` only when the high-level tools cannot complete the requested operation.
6. Verify every meaningful action with a fresh read. Prefer `browser_snapshot` when page content or interaction state matters and `browser_session_status` when session health or the actual mode matters. Do not claim success from a click or navigation call alone.
7. Continue until the requested content or visible outcome is verified. Close the task session with `browser_session_close` when no further browser work is needed.

For search-and-open requests, do not stop at search-result snippets. Snapshot the results, open the intended result using its current ref, then inspect the destination page before reporting titles, README text, documentation, or other details.

## Timeout and recovery

- A navigation timeout does not prove that navigation failed. First call `browser_session_status` and then `browser_snapshot`; continue if the destination page is available.
- Treat a tool result marked `isError: true` as a failure even when the MCP transport returned normally.
- If a ref is missing or stale, take one fresh snapshot and reacquire the target instead of retrying the old ref.
- If the Browser MCP process or session becomes unavailable, open one new session and replay from the last verified step. Do not loop indefinitely. If the single recovery attempt fails, report the exact last verified state and the failing tool.

Keep these mechanics internal unless the user asks for execution details. User-facing responses should focus on the requested result and any action the user genuinely needs to take.
