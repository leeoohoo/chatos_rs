---
name: visual-computer-use
description: Control visible macOS applications through the Visual Computer Use MCP using real screenshots, a server-rendered virtual cursor, and real mouse, keyboard, paste, and scroll events. Use for local Mac UI tasks when no more direct connector or API is available; do not use it to infer DOM, Accessibility UI trees, hidden controls, or application internals.
---

# Visual Computer Use

Operate only from what is visible in fresh screenshots. This MCP deliberately exposes no DOM or Accessibility UI tree, so screenshots and returned public process metadata are the only sources of UI truth.

Prefer a purpose-built connector, API, or CLI when one can complete the task reliably. Use this skill for visible local macOS interaction that genuinely requires the app UI.

## Prepare

1. Call `check_permissions` before the first UI task in a session.
2. If either Screen Recording or Accessibility is missing, call `request_permissions` for the missing permission and let the user complete macOS System Settings. These permissions belong to the managed `Visual Computer Use.app`, not the ChatOS Local Connector host, so use this MCP's `check_permissions` result as the source of truth. Follow the returned restart or reconnect guidance. Do not keep retrying UI actions while a permission remains unavailable.
3. Call `observe_screen` before the first action. The first observation also initializes the visible virtual-cursor overlay.
4. Every screenshot includes `activeApplication`; use it as the foreground identity check. Use `active_application` only when no screenshot is needed. Use `activate_application` with a known bundle identifier when the requested app is not frontmost, then verify both its returned screenshot and `activeApplication`.

## Identify the AI cursor

The MCP cursor is intentionally not a macOS arrow. It is a blue-purple AI orbit reticle with a dark glass core, two luminous orbit segments, four alignment ticks, a short light trail while moving, and a cyan point at its exact center.

- Treat only the cyan center point as the click hotspot. The orbit segments, ticks, glow, and trail are decorative and may overlap nearby UI.
- `virtualCursorGlobal` and `cursorScreenshotPixel` describe that cyan center, not an edge or tip. Map the desired target directly to the center; never apply a system-arrow tip offset.
- The user may still see the physical macOS arrow elsewhere on the desktop. Do not confuse it with the AI reticle and do not target from the physical arrow's location. `move_mouse` moves only the AI reticle.
- A crop that excludes the reticle shows an edge direction indicator. That indicator is not the cursor hotspot and must never be clicked as if it were the target.
- Before every click, inspect the returned image itself and verify that the cyan center—not merely the reported numeric coordinate—is inside the intended visible control.

## Core interaction loop

For each target:

1. **Observe:** inspect a fresh screenshot and its coordinate metadata.
2. **Locate:** identify the visible target in screenshot pixels, then convert that point to global macOS display points.
3. **Move:** call `move_mouse` with the global point. This moves only the MCP's visible AI orbit reticle, not the physical macOS cursor.
4. **Confirm:** inspect the returned screenshot. Confirm that the cyan center hotspot inside the non-system AI orbit reticle—not merely its decorative rings or trail—is on the intended visible control. Compare `virtualCursorGlobal` and `cursorScreenshotPixel` with the target.
5. **Act:** call `click`, `scroll`, `type_text`, or `key_press`. `click` intentionally accepts no `x` or `y`; it acts at the last verified virtual-cursor hotspot.
6. **Verify:** inspect the fresh screenshot returned by the action. Continue only when the expected visible state is present. The only exception is a deterministic intermediate keyboard chord explicitly sent with `capture_after: false`; observe the next visible state change instead.

Never combine “I expect this control to be there” with an unverified action. After any navigation, modal, focus change, scroll, app activation, page load, or animation, use the newly returned screenshot rather than stale coordinates.

## Observation and regions

- Start with a full-display `observe_screen` when the target location or active display is uncertain.
- After locating a stable window or work area, use `region` crops to improve visual detail and reduce image size. A region must have positive dimensions and fit entirely inside one active display.
- A read-only `observe_screen` region may exclude the virtual cursor and show an edge indicator.
- For pointer-dependent `click` and `scroll` calls that request a `region`, the current virtual cursor must already be inside that region. For `move_mouse`, its destination must be inside the requested region. Keyboard-only `key_press`, `type_text`, and `activate_application` calls may capture a region that excludes the cursor; their screenshots show the offscreen edge indicator while preserving `virtualCursorGlobal`.
- If the relevant UI may have moved outside a crop, return to a larger region or full display. Do not extrapolate beyond the image.
- Prefer JPEG at the default quality for ordinary UI. Use PNG or a larger `max_image_width` only when small text, thin lines, or compression artifacts prevent reliable targeting. `max_image_width: 0` requests native width and should be reserved for cases that need it.

## Fast path without lower targeting accuracy

- Full-display screenshots are for discovery, recovery, and app/window changes. Once a stable panel or editor is located, pass the smallest useful global `region` to `move_mouse` and the following `click`; keep enough surrounding UI to recognize the target and the click result.
- Never suppress the screenshot from `move_mouse`, `click`, or `scroll`. Their images are required to confirm the cyan hotspot, the real click result, and scroll progress.
- `key_press` and `type_text` support `capture_after: false`. Use it only for deterministic intermediate keyboard steps after the foreground app and focus were just verified, such as Command+A followed by Command+C. Keep screenshots enabled for navigation, opening or closing UI, paste, submission, text whose placement matters, or any step that can change layout or focus.
- After one or more suppressed intermediate steps, the next state-changing action must return a screenshot, or call `observe_screen` before locating another target. Do not suppress all visual evidence for a write sequence.
- Do not call `active_application` when a fresh screenshot already provides `activeApplication`. Recheck through the next screenshot at app boundaries, after user interference, or before a sensitive write when foreground identity is uncertain.

Read [coordinates-and-workflows.md](references/coordinates-and-workflows.md) whenever converting screenshot pixels to global coordinates, using multiple displays or Retina scaling, or troubleshooting an uncertain target.

## Clicking, focus, and text

- Always call `move_mouse` and visually verify immediately before `click`, even when a prior screenshot appeared stable.
- Use one left click by default. Use double-click, right-click, middle-click, or repeated clicks only when the visible UI and user request require them.
- To enter text, first place focus visibly: move to the field, click, and verify a focus indicator or insertion point when the app exposes one. Then call `type_text`.
- `type_text` emits real CoreGraphics Unicode keyboard events and leaves the clipboard untouched. Treat it as a real write action and verify the result. It does not prove which field had focus.
- Use `key_press` for one key or one chord, such as `["command", "shift", "p"]`. Prefer `list_shortcuts` when the shortcut is not already known and the foreground application is supported.
- Menus, autocomplete lists, sheets, alerts, and popovers can invalidate prior coordinates. Re-observe and retarget after they appear.

## Scrolling

- Move the virtual cursor over the intended scroll container first; scrolling is delivered at that point.
- Scroll deltas are pixels. Positive `delta_y` scrolls up and negative `delta_y` scrolls down. Prefer 200-500 pixels per call, observe the result, and repeat instead of attempting one large jump.
- Prefer moderate increments followed by screenshot inspection. Repeat only after verifying direction and progress.
- Stop when the target is visible or when successive screenshots show no meaningful movement. Do not issue a large blind sequence of scrolls.

## Timing and recovery

- Screenshot-producing action tools wait briefly before capture. Increase `settle_ms` when the image still shows loading, animation, or a transition in progress; do not assume the eventual state. For `capture_after: false`, keep a small nonzero settle only when the following keyboard event depends on the application processing the first one.
- If an action appears ineffective, first inspect focus, overlays, disabled state, cursor location, and whether the app is frontmost. Re-observe before retrying.
- Do not repeatedly click an uncertain target. After one ambiguous result, enlarge the observation, reactivate the app if necessary, and reacquire the target.
- If the visible UI cannot support a reliable next action, stop and report what is visible and what remains uncertain.

## Safety and authorization

Computer-use events affect the user's real desktop. Preserve the user's scope and the host's confirmation policy.

- Before a consequential action—sending or publishing content, purchase, deletion, permission change, credential submission, account change, installation, or other hard-to-reverse mutation—show that the exact visible target and resulting effect are understood, and obtain any confirmation required by the host or user.
- Do not treat permission to inspect or navigate as permission to complete a materially different external action.
- Do not expose screenshot contents, credentials, tokens, personal data, or clipboard data beyond what the task requires.
- Never claim success from an input event alone. Success requires visible post-action evidence or another purpose-built read-only check.

## Completion conditions

Finish when the requested visible outcome is verified in a fresh screenshot or through a more authoritative in-scope check. If completion cannot be verified, state the last confirmed UI state, the failed or ambiguous action, and the smallest user intervention needed.
