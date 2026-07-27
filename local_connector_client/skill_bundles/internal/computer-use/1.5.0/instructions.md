# ChatOS Computer Use — macOS Observation and Approved Control

Use this Skill only after Accessibility and Screen Recording permissions have been granted. Observe first; every input action is narrow, bounded, and requires explicit local user approval.

Observation operations:

- `computer_list_windows`: list visible application processes and their window titles, positions, and sizes. Use its PID values as the only source for application activation.
- `computer_inspect_frontmost_window`: inspect a bounded Accessibility tree for the frontmost window.
- `computer_capture_main_display`: capture the main display as a bounded JPEG and attach it only as transient image input for the next model step.
- `computer_list_displays`: list the currently active displays. It returns a 1-based capture index, stable-for-the-moment display ID, global point bounds, pixel dimensions, scale, rotation, and main-display status. Refresh this list after display hot-plug or layout changes.
- `computer_capture_display`: capture one active display selected by the current 1-based `display_index` from `computer_list_displays`. The JPEG is transient model input and is never persisted.

Approved control operations:

- `computer_click`: perform one left or right click at an exact display-local point. Omit `display_index` for the main display or use a current index from `computer_list_displays`; display geometry is revalidated after approval.
- `computer_press_key`: press one reviewed navigation key, optionally with reviewed Command, Control, Option, or Shift modifiers.
- `computer_type_text`: type at most 256 visible Unicode characters into the currently focused non-secure editable text control. Control characters, bidirectional controls, and zero-width formatting controls are rejected.
- `computer_scroll`: post one bounded horizontal/vertical pixel-scroll event at the current pointer target.
- `computer_activate_application`: bring one already-running application to the front by a PID returned by `computer_list_windows`. The Local Connector resolves the real application name before approval and rechecks the PID/name identity during execution.

Safety rules:

1. Observe before acting. Use window discovery, the Accessibility tree, a fresh display list, and a fresh screenshot to establish the target and current state.
2. Treat `display_index`, display geometry, and PID as short-lived observations. Refresh them after application launches/exits, display hot-plug, resolution changes, rotation, mirroring, or desktop layout changes.
3. Every exact control action requires approval in the Local Connector UI. Automatic approval, full-control mode, and command whitelists do not bypass this requirement.
4. The text for `computer_type_text` is shown in the transient local approval request so the user can review it. Approval history and tool results must persist only character counts and SHA-256, never the text itself.
5. Never type passwords, authentication codes, payment details, recovery secrets, private keys, or other credentials. The Adapter refuses secure/password fields, but this policy still applies to any field whose sensitivity cannot be determined.
6. The user may approve only the current action or the exact same action for the current Plugin session. Session approval is cleared when the Plugin session is cancelled or completed.
7. Cancelling the Task/Plugin session revokes waiting approvals and prevents queued actions from executing later. Stop immediately after cancellation, denial, stale-session, permission, display-drift, or application-identity errors.
8. Do not use control actions for payments, account recovery, security settings, destructive confirmation, legal consent, or other high-impact decisions. Ask the user to perform those actions directly.
9. Screenshot bytes are transient model input and must not be persisted in tool history, runtime events, chat records, Plugin storage, or the workspace.
10. Editable and secure Accessibility values remain redacted. Do not attempt to recover hidden values from other sources.
11. If macOS denies Accessibility or Screen Recording access, stop and explain the missing permission. Do not prompt through a hidden API, bypass TCC, or switch processes.

This is still not the full Computer Use implementation. Drag, a separate signed helper, richer action audit UI, post-action recovery, and Windows support remain unavailable.
