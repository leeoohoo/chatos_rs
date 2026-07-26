# ChatOS Computer Use — macOS Observation and Approved Input

Use this Skill only after Accessibility and Screen Recording permissions have been granted. Read-only observation is preferred; input actions are deliberately narrow and require explicit local user approval.

Observation operations:

- `computer_list_windows`: list visible application processes and their window titles, positions, and sizes.
- `computer_inspect_frontmost_window`: inspect a bounded Accessibility tree for the frontmost window.
- `computer_capture_main_display`: capture the main display as a bounded JPEG and attach it only as transient image input for the next model step.

Approved input operations:

- `computer_click`: perform one left or right click at an exact point inside the main display.
- `computer_press_key`: press one reviewed navigation key, optionally with reviewed Command, Control, Option, or Shift modifiers. Arbitrary text and letter keys are unavailable.

Safety rules:

1. Observe before acting. Use window discovery, the bounded Accessibility tree, and a fresh screenshot to establish the target and current state.
2. Every exact input action requires approval in the Local Connector UI. Global automatic approval or full-control settings do not bypass this requirement. Never claim an action happened before approval and a successful tool result.
3. The user may approve only the current action or the exact same action for the current Plugin session. Approval is cleared when the session is cancelled or completed.
4. Cancelling the Task/Plugin session revokes waiting approvals and prevents them from executing later. Stop immediately after cancellation, denial, stale-session, or permission errors.
5. Do not use input actions for passwords, authentication codes, payment, account recovery, security settings, destructive confirmation, legal consent, or other high-impact decisions. Ask the user to perform those actions directly.
6. Arbitrary typing, clipboard access, drag, scroll, multi-display targeting, repeated clicks, and application activation are not available in this Release. Do not simulate them through shell commands, AppleScript, or hidden APIs.
7. Capture a screenshot only when visual evidence is necessary. Screenshot bytes are transient model input and must not be persisted in tool history, runtime events, chat records, Plugin storage, or the workspace.
8. Screenshots may contain sensitive visible information. Describe only what is needed and never reproduce passwords, tokens, private messages, or unrelated personal data.
9. Editable and secure Accessibility text values remain redacted. Do not attempt to recover hidden values from other sources.
10. If macOS denies Accessibility or Screen Recording access, stop and explain the missing permission. Do not prompt through a hidden API, bypass TCC, or switch processes.

This is still not the full Computer Use implementation. Arbitrary text entry, scroll/drag, application activation, multi-display selection, a separate signed helper, richer action audit UI, and post-action recovery remain unavailable.
