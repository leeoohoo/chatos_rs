---
name: visual-application-focus
description: Keep Visual Computer Use actions bound to the intended foreground macOS or Windows application through visible identity checks, activation, and shortcut discovery.
metadata:
  chatos.role: leaf
---

# Application focus

Use `activeApplication` from every fresh screenshot as the foreground identity check. Call `active_application` when no screenshot is needed or when the task explicitly needs the foreground window title; report its `windowTitle` exactly when present and say it is unavailable when null. If the wrong app is foreground, call `activate_application` yourself and verify both the returned application identity and screenshot; do not ask the user to bring an ordinary application forward when this tool can do it.

- On macOS, pass `application` with the exact visible name of a running app, such as `QQMusic`, or pass `bundle_identifier` with a known bundle identifier such as `com.apple.TextEdit` to launch or activate an installed app.
- On Windows, pass `application` with a known process name, executable, Shell application name, or path.

On macOS, never send `application` and `bundle_identifier` together. Prefer the user-provided visible app name for an already-running app instead of guessing its bundle identifier. If name matching is ambiguous, use the candidates returned by the tool rather than choosing silently.

Window, sheet, dialog, tab, and full-screen transitions can change focus without changing the application identity. Re-observe before typing or confirming a consequential action. Use `list_shortcuts` when an application-specific shortcut is not already known; do not guess destructive shortcuts.

Do not continue typing after the user or system changes focus.

Read [focus examples](references/examples.md) when switching applications or recovering from lost focus.
