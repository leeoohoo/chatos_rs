# ChatOS Computer Use — Safe Native Control, Window Capture, and Activation Recovery

Use this Skill only on the current user's interactive desktop. Observe first. Every input action is narrow, bounded, requires a fresh local approval, produces a privacy-preserving structured audit summary, and then attempts one transient post-action screenshot. macOS requires Accessibility and Screen Recording permission. Windows uses the current user's desktop and remains subject to foreground, protected-content, UAC integrity, UI Automation provider, and system-policy restrictions.

Observation operations:

- "computer_list_windows": list visible application processes and sanitized window titles, positions, and sizes. Use its PID values as the only source for application activation.
- "computer_inspect_frontmost_window": inspect a bounded Accessibility tree on macOS or UI Automation control-view tree on Windows. Depth is limited to 1–6 and nodes to 1–400. Native editable controls, explicit `AXIsEditable` elements, and descendants with an editable AX ancestor are marked editable and their values are never read. Password controls and controls whose password state cannot be established remain value-redacted.
- "computer_capture_main_display": capture the main display and attach it only as transient image input for the next model step.
- "computer_capture_frontmost_window": capture only the current frontmost visible window. macOS binds the capture to the current AX window number and process identity; Windows binds it to the current foreground HWND, PID, process image, full window rectangle, and its visible virtual-desktop intersection. Both platforms re-read and compare the exact identity and geometry after capture. Foreground, identity, visibility, minimized-state, virtual-desktop, or geometry drift fails closed and returns no image.
- "computer_list_displays": list the currently active displays with a 1-based index, short-lived display identity, bounds, pixel dimensions, scale, rotation when available, and main-display status.
- "computer_capture_display": capture one current display selected by "display_index". macOS emits bounded JPEG; Windows emits bounded PNG. Image bytes are transient model input and are never persisted.

Approved control operations:

- "computer_click": perform one left click, one right click, or one left-button double-click at an exact display-local point. "click_count" is limited to 1 or 2, and 2 is valid only for the left button. Approval fixes the button, click count, point, display identity, and full geometry. Any drift fails closed. Both platforms arm matching mouse-up recovery before generated mouse-down events.
- "computer_drag": perform one left-button drag between two points on the same display over 80–1000 ms. Approval fixes the full path and display geometry. Cancellation is checked throughout the drag and every return path forces mouse-up.
- "computer_press_key": press one reviewed navigation key with optional reviewed modifiers. Arbitrary letter key codes are not supported. Enter, Backspace, and every key with a modifier are classified as high-risk and require the dedicated typed challenge described below. Generated key-up recovery is armed before or paired with the corresponding key-down sequence.
- "computer_type_text": type at most 256 visible Unicode characters into the currently focused non-secure writable native text control or explicit contenteditable target. Control characters, bidirectional controls, and zero-width formatting controls are rejected. Every text action is classified as high-risk and requires the dedicated typed challenge described below. Successful structured results expose only the stable target class, character count, UTF-16 unit count, and SHA-256; they never expose the text.
- "computer_scroll": post one bounded horizontal/vertical scroll event at the current pointer target.
- "computer_activate_application": bring one already-running application to the front by a PID from "computer_list_windows". The Local Connector resolves the real process identity before approval and rechecks it during execution. If cancellation arrives while activation is still in flight, ChatOS attempts to restore the exact previous foreground application only when the approved target is still foreground and both application identities remain unchanged. A user or system foreground change disables rollback. This recovery does not undo application content or arbitrary window changes.

Dedicated high-risk confirmation:

- Text input, Enter, Backspace, and modified shortcuts receive a fresh random "CONFIRM-XXXXXX" challenge in the pending Local Connector approval.
- The approval button remains disabled until the user types the exact challenge. The Local API independently rejects missing or mismatched challenge responses, so a UI-only bypass cannot approve the action.
- The challenge is bound to one pending approval ID and disappears when that request is approved, denied, cancelled, timed out, or abandoned.
- No Computer Use action exposes `acceptForSession`. Every action must return to the local approval UI, even when the global approval mode is Auto Approval or Full Control.
- The audit card records the high-risk category without persisting typed text. Challenges are transient approval data and are not added to approval history.

macOS signed helper boundary:

- All macOS Accessibility and Screen Recording probes, window/control-tree observations, display/window screenshots, and approved input actions execute inside the dedicated "chatos_computer_use_helper" process rather than the network-facing Core process.
- Core and helper use one single-request length-prefixed stdio exchange. Requests are limited to 256 KiB, responses to 4 MiB, stderr to 64 KiB, and the helper never opens a network listener or reserves a port.
- The helper path must be an executable regular non-symlink file. Production Core runs strict codesign verification and requires the helper to have the same TeamIdentifier as Core.
- The helper independently resolves its direct parent process, requires the exact Local Connector Core executable identity, verifies both running components, and requires the same TeamIdentifier before reading a request. A caller cannot turn the helper into a standalone desktop-control endpoint.
- Protocol version, operation name, approved command arguments, response envelope, and trailing bytes are validated with fail-closed limits. A protocol mismatch or malformed frame performs no desktop action.
- Every approved helper call receives a new private current-user-only cancellation directory. Core signals cancellation by atomically creating its marker; the helper polls the marker into the action cancellation flag so drag, activation rollback, and paired input guards can finish bounded recovery.
- On timeout or cancellation, Core signals the marker first and waits a bounded two-second release grace period before terminating an unresponsive helper. The helper is one-shot and exits after exactly one response.
- Windows retains the in-process implementation and the same approval, display identity, input release, privacy, activation-recovery, and no-replay contracts.

Frontmost-window screenshot contract:

- The operation is read-only and accepts no model-supplied PID, window identifier, geometry, path, or capture option. The native adapter resolves the target from the live frontmost/foreground desktop state.
- macOS reads the frontmost process, its first current Accessibility window, `AXWindowNumber`, title, position, and size; captures that window with the fixed system `screencapture -l` path into a private temporary directory; then reacquires and compares process name, PID, window number, position, and size.
- Windows requires a visible, non-minimized foreground HWND with a positive PID and non-empty rectangle. It intersects the full window rectangle with the current virtual desktop and uses bounded GDI capture only for that visible region. After PNG encoding it reacquires and compares HWND, PID, process-image identity, full geometry, and clipped capture geometry.
- A changing title alone does not retarget a capture, but the initially observed bounded title is returned as metadata. Identity or geometry drift, a fully off-desktop window, minimized state, permission loss, capture failure, timeout, invalid image type, or an image larger than 2 MiB fails closed.
- JPEG/PNG bytes and base64 appear only in transient `_model_input`. Persistable structured metadata contains the capture scope, platform, application, PID, short-lived window identity, full and captured geometry, MIME type, byte count, SHA-256, and explicit `persisted=false`; it never contains pixels or base64.

macOS secure text target contract:

- The helper uses native Accessibility APIs for text-target validation; it does not ask JXA to return a role string and then trust that string as the target identity.
- The focused application must be explicitly frontmost. The focused element and editable target must belong to the same positive PID, and the focused element must be enabled and explicitly focused.
- Native text targets are restricted to reviewed text roles and must expose either writable `AXValue` or writable `AXSelectedTextRange`. Read-only controls fail closed.
- Non-native rich text targets are restricted to reviewed `AXWebArea`, `AXGroup`, or `AXStaticText` roles, must expose `AXIsEditable=true`, and must expose writable `AXSelectedTextRange`. A focused descendant may resolve only through the standard `AXEditableAncestor` or `AXHighestEditableAncestor` relationship.
- The focused and editable elements must have finite, non-empty Accessibility bounds. Secure/password roles and any element reporting `AXContainsProtectedContent=true` are rejected.
- The helper holds the original frontmost application, focused element, and editable target references, repeats every security property query, then requires `CFEqual` identity equality for all three immediately before posting Unicode CoreGraphics input. PID, class, focus, writability, bounds, protection, or identity drift fails closed.
- The native validator reads no current text, selected text, field value, DOM content, or clipboard data.

Windows secure text target contract:

- The focused element must belong to the foreground process.
- UI Automation must explicitly confirm enabled state, keyboard focusability, current keyboard focus, visible non-empty bounds, and non-password state.
- A native target must be `Edit` and expose writable `ValuePattern`.
- A non-Edit target is restricted to `Document`, `Pane`, or `Custom` and must successfully expose live `TextEditPattern`. `TextPattern` alone is read-only evidence and is never sufficient.
- The foreground HWND/PID and exact focused UI Automation element are acquired before input, reacquired immediately before Unicode `SendInput`, compared with `CompareElements`, and fully revalidated. Target-class, PID, focus, password, bounds, pattern, or identity drift fails closed.
- Unknown, unavailable, read-only, secure, stale, unsupported, or provider-failed controls fail closed. No existing field value or document text is read.

Activation recovery contract:

- Immediately before activating an application, ChatOS captures the current foreground application identity. Windows also captures the exact foreground HWND and whether the approved target window was minimized.
- A cancellation observed after activation but before the approved action leaves its bounded post-action phase triggers one best-effort restore. No persistent rollback token is exposed and no later model action can silently invoke the restore.
- Restore is allowed only while the exact approved target remains foreground. If the user, the OS, or another application changes the foreground, recovery records `foreground_changed_after_activation` and performs no focus change.
- Both the previous and target process identity must still match. macOS re-resolves the exact PIDs and sanitized process names. Windows revalidates the exact HWND/PID/process-image identities.
- Windows restores the prior foreground HWND and re-minimizes the target only when this activation itself restored a previously minimized target. Platform foreground policy may still refuse recovery; the result reports that failure and never claims success.
- Structured results use `scope=frontmost_application_activation_only`, report whether rollback was attempted and restored, and explicitly set application-content and arbitrary window-geometry rollback to false.

Post-action observation and recovery:

- After a control action succeeds, the Local Connector waits for a short bounded settle interval and attempts one screenshot. Click and drag recapture only the approved display identity; key, text, scroll, and activation recapture the current main display.
- The screenshot is delivered only through transient model input and is limited to 2 MiB. Structured results retain capture scope, display identity, MIME type, byte count, and SHA-256, but never image pixels or base64.
- If the display changes, capture permission disappears, capture times out, the session is cancelled after input completed, or another observation error occurs, the result remains `success=true` and records `action_already_executed=true`, `automatic_replay_safe=false`, and a bounded failure reason.
- Never repeat an action merely because the transient post-action screenshot was unavailable. Observe again and decide from fresh evidence.
- Mouse/key release recovery prevents generated input state from remaining latched where the platform permits recovery. Activation recovery can restore only the previous foreground application during the same in-flight cancellation window. Neither mechanism undoes navigation, typed text, drag/drop, document edits, or arbitrary application state.

Structured approval audit:

- Every control approval includes a typed "computer_use" audit context in the pending UI and persisted approval history.
- Click and drag cards show display index, short-lived display identity, point/path, button/count or duration, and approval-time geometry.
- Keyboard cards show only the reviewed key/modifiers and the dedicated confirmation category when applicable.
- Application activation cards show the resolved PID and sanitized application identity.
- Text audit cards never contain the text itself. They retain only the reviewed target category, character count, UTF-16 unit count, SHA-256, and the "sensitive_text_entry" confirmation category.
- Audit cards are descriptive evidence and never replace live display, process, focus, writability, pattern, or control-identity revalidation.

Safety rules:

1. Observe before acting and refresh short-lived window, display, screenshot, and control-tree evidence whenever focus or layout may have changed.
2. Every exact control action requires a new approval in the Local Connector UI. Automatic approval, Full Control, command whitelists, and prior approvals do not bypass this rule.
3. For a high-risk action, read the risk statement and type the exact one-time challenge yourself. Do not approve a challenge whose action, key, modifiers, text intent, or target is unclear.
4. Never type passwords, authentication codes, payment details, recovery secrets, private keys, or other credentials. Identified or uncertain secure targets fail closed.
5. Do not use control actions for payments, account recovery, security settings, destructive confirmation, legal consent, or other high-impact decisions. Ask the user to perform those actions directly.
6. Use double-click or drag only when the source, destination, and effect are visibly established and reversible.
7. Cancelling the Task or Plugin session revokes waiting approvals and marks a running action cancelled. An in-flight activation may restore only the exact prior foreground identity under the recovery contract above. Other completed side effects remain non-rollbackable and never become safe to replay.
8. Stop on denial, stale session, permission failure, display drift, window/process/focus identity drift, helper identity failure, UI Automation provider failure, protected content, or integrity-level restrictions.
9. Screenshot bytes are transient model input and must not be persisted in tool history, runtime events, chat records, Plugin storage, or the workspace.
10. On Windows, never bypass UAC, protected desktops, elevated applications, foreground restrictions, blocked UI Automation providers, or blocked `SendInput`.
11. On macOS, never bypass TCC, helper signature checks, direct-parent verification, protocol limits, native AX validation, or cancellation grace. If Accessibility or Screen Recording access is denied, stop and explain the missing permission.

This release adds a dedicated bounded frontmost-window screenshot on macOS and Windows with capture-time identity/geometry rebinding protection and transient-only pixels. It preserves the signed one-shot macOS helper, fail-closed native/contenteditable text entry, high-risk challenge, per-action approval, privacy audit, transient post-action observation, activation recovery, input-release, and no-automatic-replay contracts from `1.15.0`.
