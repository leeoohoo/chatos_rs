---
name: visual-computer-use
description: Route visible macOS and Windows automation through screenshot observation, pointer and keyboard interaction, application focus, and safety verification using the Visual Computer Use MCP.
metadata:
  chatos.role: router
  chatos.related-skills: "visual-observation-targeting,visual-pointer-keyboard,visual-application-focus,visual-safety-verification"
---

# Visual Computer Use

Use this plugin only when a purpose-built connector, API, or CLI cannot complete the task and visible local desktop interaction is genuinely required. The plugin exposes screenshots and public process metadata, not DOM, macOS Accessibility UI trees, or Windows UI Automation trees.

## Start

1. Call `check_permissions` before the first UI task.
2. Read the platform-specific result. On macOS, Screen Recording or Accessibility may require the user to grant access. On Windows, resolve interactive-desktop, capture, input, or Administrator/UIPI limitations reported by the tool. If the result requires user action, call `request_permissions` once and stop retries until that action is complete.
3. Call `observe_screen` and treat the returned screenshot plus `activeApplication` as the UI source of truth.

## Route the work

- Activate `visual-observation-targeting` for screenshot regions, coordinate scaling, Retina/DPI or multiple-display mapping, and virtual-cursor targeting.
- Activate `visual-pointer-keyboard` for clicks, scrolling, typing, key chords, focus, and action recovery.
- Activate `visual-application-focus` for foreground identity, application activation, window transitions, and shortcuts.
- Activate `visual-safety-verification` before consequential external actions or whenever completion evidence matters.

Activate only the leaves needed by the current visible workflow. ChatOS associates each selected leaf with this router internally.

## Platform Skill protocol

Activate this router and the chosen leaf through the host's advertised Skill tools. Main Chat and Task Execution use `agent_skill_activate` with the exact catalog `skill_ref`; its result already lists declared references, so read only a relevant one with `agent_skill_read_resource`. If that result is unavailable after resume, activate the same exact ref again. Other host surfaces use their advertised tool names and schemas. ChatOS manages activation evidence; never invent it or call an unadvertised alias. Call Visual Computer Use tools with visible-action arguments only; never add user, project, workspace, device, session, activation, or authentication fields.

## MCP tool directory

- Permissions: `check_permissions` reads current authorization; `request_permissions` opens the bounded native onboarding UI when authorization is missing.
- Observation and targeting: `observe_screen` captures current visible truth; `move_mouse` moves only the visible AI reticle and returns a verification screenshot.
- Pointer and keyboard: `click` acts at the verified reticle hotspot; `scroll` scrolls the targeted visible container; `type_text` enters Unicode text; `key_press` sends one key or chord.
- Application focus: `active_application` reads public foreground-process identity and an available visible window title; `activate_application` launches or focuses an app and observes it; `list_shortcuts` reads known shortcuts for the active app. Follow the advertised tool schema exactly: macOS requires `bundle_identifier`, while Windows requires `application` and accepts a process name, executable, Shell application name, or path. Never send both or guess an unadvertised field.

Do not substitute one tool for another: `move_mouse` never clicks, an input event never proves success, and application identity never proves that the intended field or dialog is focused.

## Invariants

- Move and visually verify the AI cursor before every click.
- The cyan center of the blue-purple orbit reticle is the click hotspot; the physical system pointer is unrelated.
- Every action that can change layout or focus requires a fresh screenshot before the next target is chosen.
- Never claim success from an input event alone.
