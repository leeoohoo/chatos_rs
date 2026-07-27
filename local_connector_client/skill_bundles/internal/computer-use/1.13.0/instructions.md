# ChatOS Computer Use — Signed macOS Helper and Approved Desktop Control

Use this Skill only on the current user's interactive desktop. Observe first. Every input action is narrow, bounded, requires a fresh local approval, produces a privacy-preserving structured audit summary, and then attempts one transient post-action screenshot. macOS requires Accessibility and Screen Recording permission. Windows uses the current user's desktop and remains subject to foreground, protected-content, UAC integrity, UI Automation provider, and system-policy restrictions.

Observation operations:

- "computer_list_windows": list visible application processes and sanitized window titles, positions, and sizes. Use its PID values as the only source for application activation.
- "computer_inspect_frontmost_window": inspect a bounded Accessibility tree on macOS or UI Automation control-view tree on Windows. Depth is limited to 1–6 and nodes to 1–400. Editable values are never read. Password controls and controls whose password state cannot be established remain value-redacted.
- "computer_capture_main_display": capture the main display and attach it only as transient image input for the next model step.
- "computer_list_displays": list the currently active displays with a 1-based index, short-lived display identity, bounds, pixel dimensions, scale, rotation when available, and main-display status.
- "computer_capture_display": capture one current display selected by "display_index". macOS emits bounded JPEG; Windows emits bounded PNG. Image bytes are transient model input and are never persisted.

Approved control operations:

- "computer_click": perform one left click, one right click, or one left-button double-click at an exact display-local point. "click_count" is limited to 1 or 2, and 2 is valid only for the left button. Approval fixes the button, click count, point, display identity, and full geometry. Any drift fails closed. Both platforms arm matching mouse-up recovery before generated mouse-down events.
- "computer_drag": perform one left-button drag between two points on the same display over 80–1000 ms. Approval fixes the full path and display geometry. Cancellation is checked throughout the drag and every return path forces mouse-up.
- "computer_press_key": press one reviewed navigation key with optional reviewed modifiers. Arbitrary letter key codes are not supported. Enter, Backspace, and every key with a modifier are classified as high-risk and require the dedicated typed challenge described below. Generated key-up recovery is armed before or paired with the corresponding key-down sequence.
- "computer_type_text": type at most 256 visible Unicode characters into the currently focused non-secure editable text control. Control characters, bidirectional controls, and zero-width formatting controls are rejected. Every text action is classified as high-risk and requires the dedicated typed challenge described below.
- "computer_scroll": post one bounded horizontal/vertical scroll event at the current pointer target.
- "computer_activate_application": bring one already-running application to the front by a PID from "computer_list_windows". The Local Connector resolves the real process identity before approval and rechecks it during execution.

Dedicated high-risk confirmation:

- Text input, Enter, Backspace, and modified shortcuts receive a fresh random "CONFIRM-XXXXXX" challenge in the pending Local Connector approval.
- The approval button remains disabled until the user types the exact challenge. The Local API independently rejects missing or mismatched challenge responses, so a UI-only bypass cannot approve the action.
- The challenge is bound to one pending approval ID and disappears when that request is approved, denied, cancelled, timed out, or abandoned.
- No Computer Use action exposes `acceptForSession`. Every action must return to the local approval UI, even when the global approval mode is Auto Approval or Full Control.
- The audit card records the high-risk category without persisting typed text. Challenges are transient approval data and are not added to approval history.

macOS signed helper boundary:

- All macOS Accessibility and Screen Recording probes, window/control-tree observations, screenshots, and approved input actions execute inside the dedicated "chatos_computer_use_helper" process rather than the network-facing Core process.
- Core and helper use one single-request length-prefixed stdio exchange. Requests are limited to 256 KiB, responses to 4 MiB, stderr to 64 KiB, and the helper never opens a network listener or reserves a port.
- The helper path must be an executable regular non-symlink file. Production Core runs strict codesign verification and requires the helper to have the same TeamIdentifier as Core.
- The helper independently resolves its direct parent process, requires the exact Local Connector Core executable identity, verifies both running components, and requires the same TeamIdentifier before reading a request. A caller cannot turn the helper into a standalone desktop-control endpoint.
- Protocol version, operation name, approved command arguments, response envelope, and trailing bytes are validated with fail-closed limits. A protocol mismatch or malformed frame performs no desktop action.
- Every approved helper call receives a new private current-user-only cancellation directory. Core signals cancellation by atomically creating its marker; the helper polls the marker into the action cancellation flag so drag and paired input guards can release generated input.
- On timeout or cancellation, Core signals the marker first and waits a bounded two-second release grace period before terminating an unresponsive helper. The helper is one-shot and exits after exactly one response.
- Windows retains the existing in-process implementation and the same approval, display identity, input release, privacy, and no-replay contracts.

Windows secure text target contract:

- The focused element must belong to the foreground process.
- UI Automation must explicitly confirm Edit control type, enabled state, keyboard focusability, current keyboard focus, visible non-empty bounds, non-password state, writable ValuePattern, and an unchanged focused identity immediately before Unicode SendInput.
- Unknown, unavailable, read-only, secure, stale, non-Edit, unsupported, or identity-drifted controls fail closed.

Post-action observation and recovery:

- After a control action succeeds, the Local Connector waits for a short bounded settle interval and attempts one screenshot. Click and drag recapture only the approved display identity; key, text, scroll, and activation recapture the current main display.
- The screenshot is delivered only through transient model input and is limited to 2 MiB. Structured results retain capture scope, display identity, MIME type, byte count, and SHA-256, but never image pixels or base64.
- If the display changes, capture permission disappears, capture times out, the session is cancelled after input completed, or another observation error occurs, the result remains "success: true" and records "action_already_executed: true", "automatic_replay_safe: false", and a bounded failure reason.
- Never repeat an action merely because the transient post-action screenshot was unavailable. Observe again and decide from fresh evidence.
- Mouse/key release recovery prevents generated input state from remaining latched where the platform permits recovery. It does not roll back application state, undo navigation, remove typed text, reverse a drag/drop, or make an action idempotent.

Structured approval audit:

- Every control approval includes a typed "computer_use" audit context in the pending UI and persisted approval history.
- Click and drag cards show display index, short-lived display identity, point/path, button/count or duration, and approval-time geometry.
- Keyboard cards show only the reviewed key/modifiers and the dedicated confirmation category when applicable.
- Application activation cards show the resolved PID and sanitized application identity.
- Text audit cards never contain the text itself. They retain only target class, character count, UTF-16 unit count, SHA-256, and the "sensitive_text_entry" confirmation category.
- Audit cards are descriptive evidence and never replace live display, process, focus, or control-identity revalidation.

Safety rules:

1. Observe before acting and refresh short-lived window, display, screenshot, and control-tree evidence whenever focus or layout may have changed.
2. Every exact control action requires a new approval in the Local Connector UI. Automatic approval, Full Control, command whitelists, and prior approvals do not bypass this rule.
3. For a high-risk action, read the risk statement and type the exact one-time challenge yourself. Do not approve a challenge whose action, key, modifiers, text intent, or target is unclear.
4. Never type passwords, authentication codes, payment details, recovery secrets, private keys, or other credentials. Identified or uncertain secure targets fail closed.
5. Do not use control actions for payments, account recovery, security settings, destructive confirmation, legal consent, or other high-impact decisions. Ask the user to perform those actions directly.
6. Use double-click or drag only when the source, destination, and effect are visibly established and reversible.
7. Cancelling the Task or Plugin session revokes waiting approvals and marks a running action cancelled. Cancellation after an action completed may skip its screenshot but must not convert the completed action into a replayable failure.
8. Stop on denial, stale session, permission failure, display drift, process/focus identity drift, helper identity failure, UI Automation provider failure, protected content, or integrity-level restrictions.
9. Screenshot bytes are transient model input and must not be persisted in tool history, runtime events, chat records, Plugin storage, or the workspace.
10. On Windows, never bypass UAC, protected desktops, elevated applications, foreground restrictions, blocked UI Automation providers, or blocked SendInput.
11. On macOS, never bypass TCC, helper signature checks, direct-parent verification, protocol limits, or cancellation grace. If Accessibility or Screen Recording access is denied, stop and explain the missing permission.

This release moves all macOS Computer Use TCC checks, observations, screenshots, and approved controls into a one-shot signed helper with bounded versioned stdio, strict same-team verification in both directions, private marker-based cancellation, and no network listener. It preserves the "1.12.0" high-risk challenge, per-action approval, privacy audit, post-action observation, input-release, and no-automatic-replay contracts. Non-Edit contenteditable text entry and application-state rollback remain unavailable.
