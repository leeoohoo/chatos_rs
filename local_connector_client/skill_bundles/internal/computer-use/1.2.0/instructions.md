# ChatOS Computer Use — macOS Read-Only Visual Observation

Use this Skill only for read-only inspection of the user's macOS desktop after Accessibility and Screen Recording permissions have been granted.

Available operations:

- `computer_list_windows`: list visible application processes and their window titles, positions, and sizes.
- `computer_inspect_frontmost_window`: inspect a bounded Accessibility tree for the frontmost window.
- `computer_capture_main_display`: capture the main display as a bounded JPEG and attach it only as transient image input for the next model step.

Safety rules:

1. Observe only. This Release does not publish mouse, keyboard, scroll, drag, application activation, file-upload, or other write actions.
2. Capture a screenshot only when visual evidence is necessary for the user's current task. Prefer the window list and bounded Accessibility tree first.
3. Screenshots may contain sensitive visible information. Describe only what is needed for the task and never reproduce passwords, tokens, private messages, or unrelated personal data.
4. Screenshot bytes are transient model input. They must not be written to tool history, runtime events, persistent chat records, Plugin storage, or the user's workspace.
5. Editable and secure Accessibility text values remain redacted. Do not attempt to recover hidden values from other sources.
6. If macOS denies Accessibility or Screen Recording access, stop and explain which explicit permission is missing. Do not prompt through a hidden API, bypass TCC, or switch processes.
7. Never claim an input action was performed. Ask the user to perform any required click, typing, confirmation, or sensitive operation themselves.

This is not the full Computer Use implementation. A signed native helper, controlled input actions, per-action approvals, action audit logs, emergency stop, multi-display selection, and recovery remain unavailable in this Release.
