---
name: visual-computer-use
description: Route visible macOS automation through screenshot observation, pointer and keyboard interaction, application focus, and safety verification using the Visual Computer Use MCP.
metadata:
  chatos.role: router
  chatos.related-skills: "visual-observation-targeting,visual-pointer-keyboard,visual-application-focus,visual-safety-verification"
---

# Visual Computer Use

Use this plugin only when a purpose-built connector, API, or CLI cannot complete the task and visible local macOS interaction is genuinely required. The plugin exposes screenshots and public process metadata, not DOM or Accessibility UI trees.

## Start

1. Call `check_permissions` before the first UI task.
2. If Screen Recording or Accessibility is missing, call `request_permissions` and stop tool retries until the user completes the system step.
3. Call `observe_screen` and treat the returned screenshot plus `activeApplication` as the UI source of truth.

## Route the work

- Activate `visual-observation-targeting` for screenshot regions, coordinates, Retina or multiple-display mapping, and virtual-cursor targeting.
- Activate `visual-pointer-keyboard` for clicks, scrolling, typing, key chords, focus, and action recovery.
- Activate `visual-application-focus` for foreground identity, application activation, window transitions, and shortcuts.
- Activate `visual-safety-verification` before consequential external actions or whenever completion evidence matters.

Activate only the leaves needed by the current visible workflow. ChatOS associates each selected leaf with this router internally.

## Platform Skill protocol

Activate this router with `skill_skill_activate`, then activate the chosen leaf. ChatOS records and validates those activations automatically. Call Visual Computer Use tools with visible-action arguments only; never add user, project, workspace, device, session, activation, or authentication fields. Use `skill_skill_list_resources` and `skill_skill_read_resource` only for references declared by the activated leaf.

## MCP tool directory

- Permissions: `check_permissions` reads current authorization; `request_permissions` opens the bounded native onboarding UI when authorization is missing.
- Observation and targeting: `observe_screen` captures current visible truth; `move_mouse` moves only the visible AI reticle and returns a verification screenshot.
- Pointer and keyboard: `click` acts at the verified reticle hotspot; `scroll` scrolls the targeted visible container; `type_text` enters Unicode text; `key_press` sends one key or chord.
- Application focus: `active_application` reads frontmost public process identity; `activate_application` launches or focuses a known bundle identifier and observes it; `list_shortcuts` reads known shortcuts for the active app.

Do not substitute one tool for another: `move_mouse` never clicks, an input event never proves success, and application identity never proves that the intended field or dialog is focused.

## Invariants

- Move and visually verify the AI cursor before every click.
- The cyan center of the blue-purple orbit reticle is the click hotspot; the physical macOS arrow is unrelated.
- Every action that can change layout or focus requires a fresh screenshot before the next target is chosen.
- Never claim success from an input event alone.
