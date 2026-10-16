---
name: visual-application-focus
description: Keep Visual Computer Use actions bound to the intended foreground macOS application through visible identity checks, activation, and shortcut discovery.
metadata:
  chatos.role: leaf
---

# Application focus

Use `activeApplication` from every fresh screenshot as the foreground identity check. Call `active_application` when no screenshot is needed or when the task explicitly needs the frontmost standard-window title; report its `windowTitle` exactly when present and say it is unavailable when null. If the wrong app is frontmost, call `activate_application` with a known bundle identifier and verify both the returned application identity and screenshot.

Window, sheet, dialog, tab, and full-screen transitions can change focus without changing the bundle identifier. Re-observe before typing or confirming a consequential action. Use `list_shortcuts` when an application-specific shortcut is not already known; do not guess destructive shortcuts.

Do not infer a bundle identifier from a window title alone. Do not continue typing after the user or system changes focus.

Read [focus examples](references/examples.md) when switching applications or recovering from lost focus.
