# ChatOS Computer Use — macOS Observation and Approved Control

Use this Skill only after Accessibility and Screen Recording permissions have been granted. Observe first; every input action is narrow, bounded, and requires explicit local user approval.

Observation operations:

- `computer_list_windows`: list visible application processes and their window titles, positions, and sizes.
- `computer_inspect_frontmost_window`: inspect a bounded Accessibility tree for the frontmost window.
- `computer_capture_main_display`: capture the main display as a bounded JPEG and attach it only as transient image input for the next model step.

Approved control operations:

- `computer_click`: perform one left or right click at an exact point inside the main display.
- `computer_press_key`: press one reviewed navigation key, optionally with reviewed Command, Control, Option, or Shift modifiers.
- `computer_type_text`: type at most 256 visible Unicode characters into the currently focused non-secure editable text control. Control characters, bidirectional controls, and zero-width formatting controls are rejected.
- `computer_scroll`: post one bounded horizontal/vertical pixel-scroll event at the current pointer target.

Safety rules:

1. Observe before acting. Use window discovery, the Accessibility tree, and a fresh screenshot to establish the target and current state.
2. Every exact control action requires approval in the Local Connector UI. Automatic approval, full-control mode, and command whitelists do not bypass this requirement.
3. The text for `computer_type_text` is shown in the transient local approval request so the user can review it. Approval history and tool results must persist only character counts and SHA-256, never the text itself.
4. Never type passwords, authentication codes, payment details, recovery secrets, private keys, or other credentials. The Adapter refuses secure/password fields, but this policy still applies to any field whose sensitivity cannot be determined.
5. The user may approve only the current action or the exact same action for the current Plugin session. Session approval is cleared when the Plugin session is cancelled or completed.
6. Cancelling the Task/Plugin session revokes waiting approvals and prevents queued actions from executing later. Stop immediately after cancellation, denial, stale-session, or permission errors.
7. Do not use control actions for payments, account recovery, security settings, destructive confirmation, legal consent, or other high-impact decisions. Ask the user to perform those actions directly.
8. Screenshot bytes are transient model input and must not be persisted in tool history, runtime events, chat records, Plugin storage, or the workspace.
9. Editable and secure Accessibility values remain redacted. Do not attempt to recover hidden values from other sources.
10. If macOS denies Accessibility or Screen Recording access, stop and explain the missing permission. Do not prompt through a hidden API, bypass TCC, or switch processes.

This is still not the full Computer Use implementation. Drag, application activation, multi-display targeting, a separate signed helper, richer action audit UI, and post-action recovery remain unavailable.
