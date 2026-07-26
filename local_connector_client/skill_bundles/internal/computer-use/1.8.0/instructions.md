# ChatOS Computer Use — macOS and Windows Observation with Approved Control

Use this Skill only on the current user's interactive desktop. Observe first; every input action is narrow, bounded, and requires explicit local user approval. macOS requires Accessibility and Screen Recording permission. Windows uses the current user's desktop and remains subject to foreground, protected-content, UAC integrity, and system-policy restrictions.

Observation operations:

- `computer_list_windows`: list visible application processes and their window titles, positions, and sizes. Use its PID values as the only source for application activation.
- `computer_inspect_frontmost_window`: on macOS only, inspect a bounded Accessibility tree for the frontmost window. Windows UI Automation inspection is not available in this release.
- `computer_capture_main_display`: capture the main display and attach it only as transient image input for the next model step.
- `computer_list_displays`: list the currently active displays. It returns a 1-based display index, stable-for-the-moment display identity, global bounds, pixel dimensions, scale, rotation when available, and main-display status. Refresh this list after display hot-plug or layout changes.
- `computer_capture_display`: capture one active display selected by the current 1-based `display_index` from `computer_list_displays`. macOS emits bounded JPEG; Windows emits bounded PNG. Image bytes are transient model input and are never persisted.

Approved control operations:

- `computer_click`: perform one left click, one right click, or one left-button double-click at an exact display-local point. `click_count` is limited to 1 or 2; a count of 2 is accepted only with the left button. Omit `display_index` for the main display or use a current index from `computer_list_displays`. Approval fixes the button, click count, point, display identity, and full geometry; any drift fails closed. Double-click posts two complete down/up pairs and checks cancellation between the pairs.
- `computer_drag`: perform one left-button drag between two points on the same display. Duration is limited to 80–1000 ms. Approval fixes the full path and display geometry. Cancellation is checked throughout the drag, and every return path forces mouse-up.
- `computer_press_key`: press one reviewed navigation key, optionally with reviewed modifiers. On Windows, `command` maps to the Windows key and `option` maps to Alt. Arbitrary letter key codes are not supported.
- `computer_type_text`: macOS only; type at most 256 visible Unicode characters into the currently focused non-secure editable text control. Windows text input is deliberately unavailable until secure/password field inspection is implemented.
- `computer_scroll`: post one bounded horizontal/vertical scroll event at the current pointer target.
- `computer_activate_application`: bring one already-running application to the front by a PID returned by `computer_list_windows`. The Local Connector resolves the real process identity before approval and rechecks it during execution.

Safety rules:

1. Observe before acting. Use a fresh window list, display list, and screenshot; on macOS also use the Accessibility tree when needed.
2. Treat `display_index`, display geometry, and PID as short-lived observations. Refresh them after application launches/exits, display hot-plug, resolution changes, rotation, mirroring, or desktop layout changes.
3. Every exact control action requires approval in the Local Connector UI. Automatic approval, full-control mode, and command whitelists do not bypass this requirement.
4. A click or drag is valid only for the exact display identity and geometry shown at approval time. If the display changes, stop, observe again, and request a new approval.
5. Use double-click only when the intended target is visibly established and its double-click behavior is understood. Never double-click destructive controls, confirmation buttons, payments, security settings, or targets with uncertain side effects.
6. Drag only when the source and destination are both visibly established on the same display. Do not use drag for destructive movement, irreversible reordering, or targets whose drop effect is unclear.
7. On macOS, text for `computer_type_text` is shown in the transient local approval request. Approval history and tool results persist only character counts and SHA-256, never the text itself.
8. Never type passwords, authentication codes, payment details, recovery secrets, private keys, or other credentials. Windows text input is unavailable; macOS also refuses secure/password fields.
9. The user may approve only the current action or the exact same action for the current Plugin session. Session approval is cleared when the Plugin session is cancelled or completed.
10. Cancelling the Task/Plugin session revokes waiting approvals, marks any running native action cancelled, and prevents queued actions from executing later. A running drag must release the mouse before returning; double-click cancellation is observed only between complete down/up pairs.
11. Stop immediately after cancellation, denial, stale-session, permission, display-drift, application-identity, foreground-policy, protected-content, or integrity-level errors.
12. Do not use control actions for payments, account recovery, security settings, destructive confirmation, legal consent, or other high-impact decisions. Ask the user to perform those actions directly.
13. Screenshot bytes are transient model input and must not be persisted in tool history, runtime events, chat records, Plugin storage, or the workspace.
14. Editable and secure Accessibility values remain redacted. Do not attempt to recover hidden values from screenshots or other sources.
15. On Windows, do not attempt to bypass UAC, protected desktops, elevated applications, foreground restrictions, or blocked `SendInput`; report the restriction and stop.
16. On macOS, if Accessibility or Screen Recording access is denied, stop and explain the missing permission. Do not prompt through a hidden API, bypass TCC, or switch processes.

This release adds Windows current-user window/display observation, bounded transient PNG capture, display-identity-bound left/right/double click, interruptible drag, reviewed navigation keys, bounded scrolling, and PID-identity-guarded application activation. Windows UI Automation inspection, secure-field-aware text entry, a separate signed helper, richer action audit UI, and generalized post-action recovery remain unavailable.
