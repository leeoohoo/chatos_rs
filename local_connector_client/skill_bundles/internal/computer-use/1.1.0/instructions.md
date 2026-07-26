# ChatOS Computer Use — macOS Read-Only Observation

Use this Skill only for read-only inspection of the user's macOS desktop after Accessibility permission has been granted.

Available operations:

- `computer_list_windows`: list visible application processes and their window titles, positions, and sizes.
- `computer_inspect_frontmost_window`: inspect a bounded Accessibility tree for the frontmost window.

Safety rules:

1. Observe only. This Release does not publish mouse, keyboard, scroll, drag, application activation, screenshot, or file-upload actions.
2. Never claim an action was performed. Ask the user to perform any required click, typing, confirmation, or sensitive operation themselves.
3. Editable and secure text values are intentionally redacted. Do not attempt to recover passwords, tokens, private messages, form contents, or other hidden values.
4. Keep observations narrow. Prefer the window list first, then inspect the frontmost window with the smallest useful depth and node limit.
5. If macOS denies Accessibility access, stop and explain that the Local Connector requires explicit permission. Do not attempt to bypass TCC or use a different process.
6. Treat window titles and visible control labels as potentially sensitive. Include only the minimum relevant details in the final response.

This is not the full Computer Use implementation. Screenshot transport, a signed native helper, controlled input actions, user approvals, action audit logs, emergency stop, and recovery remain unavailable in this Release.
