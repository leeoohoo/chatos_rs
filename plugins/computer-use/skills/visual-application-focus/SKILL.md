---
name: visual-application-focus
description: Keep Visual Computer Use actions bound to the intended foreground macOS or Windows application through visible identity checks, activation, and shortcut discovery.
metadata:
  chatos.role: leaf
---

# Application focus

Use `activeApplication` from every fresh screenshot as the foreground identity check. Call `active_application` when no screenshot is needed or when the task explicitly needs the foreground window title; report its `windowTitle` exactly when present and say it is unavailable when null. If the wrong app is foreground, inspect the advertised `activate_application` schema, call it with the platform's required identifier, and verify both the returned application identity and screenshot.

- On macOS, pass `bundle_identifier` with a known bundle identifier such as `com.apple.TextEdit`.
- On Windows, pass `application` with a known process name, executable, Shell application name, or path.

Never send both fields, translate one field into the other, or infer an identifier from a window title alone.

Window, sheet, dialog, tab, and full-screen transitions can change focus without changing the application identity. Re-observe before typing or confirming a consequential action. Use `list_shortcuts` when an application-specific shortcut is not already known; do not guess destructive shortcuts.

Do not continue typing after the user or system changes focus.

Read [focus examples](references/examples.md) when switching applications or recovering from lost focus.
