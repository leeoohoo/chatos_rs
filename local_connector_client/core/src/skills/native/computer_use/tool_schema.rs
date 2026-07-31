// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};

use super::{
    current_platform_name, DEFAULT_DRAG_DURATION_MS, DEFAULT_TREE_DEPTH, DEFAULT_TREE_NODES,
    DEFAULT_WINDOW_LIMIT, MAX_SCROLL_DELTA, MAX_TREE_DEPTH, MAX_TREE_NODES, MAX_WINDOW_COORDINATE,
    MAX_WINDOW_DIMENSION, MAX_WINDOW_LIMIT, MIN_DRAG_DURATION_MS, MIN_WINDOW_COORDINATE,
    MIN_WINDOW_DIMENSION,
};

pub(super) fn tool_definitions(include_control: bool) -> Vec<Value> {
    tool_definitions_for_platform(include_control, current_platform_name())
}

pub(super) fn tool_definitions_for_platform(include_control: bool, platform: &str) -> Vec<Value> {
    let mut tools = vec![
        json!({
            "name": "computer_list_windows",
            "description": "Read-only desktop observation on the current supported platform: list visible application windows, titles, positions, and sizes. Does not click, type, or read text-field contents.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "limit": {"type": "integer", "minimum": 1, "maximum": MAX_WINDOW_LIMIT, "default": DEFAULT_WINDOW_LIMIT}
                },
                "additionalProperties": false
            }
        }),
        json!({
            "name": "computer_capture_window_layout",
            "description": "Read-only capture of a short-lived opaque layout snapshot for at most 8 ordinary visible top-level windows on the current desktop. Only non-minimized, non-fullscreen/non-maximized windows with writable native position and size are included. The model receives only a snapshot ID, SHA-256, counts, and application summary; native window identities and coordinates remain in volatile Local Connector memory for 10 minutes and are never persisted.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }
        }),
        json!({
            "name": "computer_inspect_frontmost_window",
            "description": "Read-only bounded Accessibility/UI Automation inspection of the frontmost window on the current supported platform. Editable and secure text values are redacted; only reviewed visible control metadata is returned.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "max_depth": {"type": "integer", "minimum": 1, "maximum": MAX_TREE_DEPTH, "default": DEFAULT_TREE_DEPTH},
                    "max_nodes": {"type": "integer", "minimum": 1, "maximum": MAX_TREE_NODES, "default": DEFAULT_TREE_NODES}
                },
                "additionalProperties": false
            }
        }),
        json!({
            "name": "computer_capture_main_display",
            "description": "Read-only screenshot observation of the main display on the current supported platform. The image is delivered only as transient model input, is never persisted in tool history, and may contain sensitive visible information. Does not click, type, or change desktop state.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }
        }),
        json!({
            "name": "computer_capture_frontmost_window",
            "description": "Read-only screenshot observation limited to the current frontmost visible window on the current supported platform. The exact window identity and geometry are revalidated after capture; any foreground or layout drift fails closed. The image is transient model input and is never persisted in tool history.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }
        }),
        json!({
            "name": "computer_list_displays",
            "description": "Read-only display discovery on the current supported platform. Returns stable-for-this-moment indexes and display identities, global coordinate bounds, pixel dimensions, scale, rotation when available, and main-display status. Re-list after display hot-plug or arrangement changes.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }
        }),
        json!({
            "name": "computer_capture_display",
            "description": "Read-only screenshot observation of one currently active display selected by the 1-based display_index returned by computer_list_displays. The image is transient model input and is never persisted in tool history.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "display_index": {"type": "integer", "minimum": 1, "maximum": 16}
                },
                "required": ["display_index"],
                "additionalProperties": false
            }
        }),
    ];
    if include_control {
        tools.extend([
            json!({
                "name": "computer_click",
                "description": "Perform one left/right click or one left-button double-click at a display-local point on the current supported platform. Omit display_index for the main display, or use the current 1-based index from computer_list_displays. Every exact button, click count, point, and display requires explicit local user approval; display geometry is revalidated after approval. A best-effort transient post-action screenshot is attached without persisting pixels; if observation fails, the result still records that the click already ran and must not be replayed automatically.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "display_index": {"type": "integer", "minimum": 1, "maximum": 16, "description": "Optional 1-based active display index; defaults to the main display."},
                        "x": {"type": "number", "description": "Display-local x coordinate in platform desktop units."},
                        "y": {"type": "number", "description": "Display-local y coordinate in platform desktop units."},
                        "button": {"type": "string", "enum": ["left", "right"], "default": "left"},
                        "click_count": {"type": "integer", "enum": [1, 2], "default": 1, "description": "Two clicks are supported only with the left button."}
                    },
                    "required": ["x", "y"],
                    "additionalProperties": false
                }
            }),
            json!({
                "name": "computer_drag",
                "description": "Perform one bounded left-button drag between two display-local points on the same active display. Duration is limited to 80-1000 ms. The exact path requires explicit local user approval, display identity and geometry are revalidated after approval, and cancellation forces mouse-up before returning. A best-effort transient post-action screenshot is attached without persisting pixels; observation failure never causes an automatic drag replay.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "display_index": {"type": "integer", "minimum": 1, "maximum": 16, "description": "Optional 1-based active display index; defaults to the main display."},
                        "start_x": {"type": "number", "description": "Display-local starting x coordinate in platform desktop units."},
                        "start_y": {"type": "number", "description": "Display-local starting y coordinate in platform desktop units."},
                        "end_x": {"type": "number", "description": "Display-local ending x coordinate in platform desktop units."},
                        "end_y": {"type": "number", "description": "Display-local ending y coordinate in platform desktop units."},
                        "duration_ms": {"type": "integer", "minimum": MIN_DRAG_DURATION_MS, "maximum": 1000, "default": DEFAULT_DRAG_DURATION_MS}
                    },
                    "required": ["start_x", "start_y", "end_x", "end_y"],
                    "additionalProperties": false
                }
            }),
            json!({
                "name": "computer_press_key",
                "description": "Press one reviewed navigation key on the current supported platform, optionally with reviewed modifiers. Use computer_type_text for approved bounded text entry into a verified non-secure editable control. Arbitrary letter key codes are not supported. Enter, Backspace, and every modified shortcut additionally require the user to type a one-time random confirmation challenge; Computer Use actions cannot be approved for the whole session. A best-effort transient post-action screenshot is attached and the action is never replayed merely because observation failed.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "key": {"type": "string", "enum": ["enter", "tab", "space", "escape", "backspace", "left", "right", "up", "down", "home", "end", "page_up", "page_down"]},
                        "modifiers": {
                            "type": "array",
                            "items": {"type": "string", "enum": ["command", "control", "option", "shift"]},
                            "maxItems": 4,
                            "uniqueItems": true,
                            "default": []
                        }
                    },
                    "required": ["key"],
                    "additionalProperties": false
                }
            }),
            json!({
                "name": "computer_type_text",
                "description": "Type bounded Unicode text into the currently focused non-secure editable text control on the current supported platform. macOS requires a live frontmost Accessibility identity, focus, enabled and visible bounds, then either a writable native text role or an explicit AXIsEditable rich-text target with writable AXSelectedTextRange; the same focused and editable AX elements are compared again immediately before input. Windows requires matching foreground PID, focus, enabled and visible bounds, explicit non-password state, then either Edit plus writable ValuePattern or Document/Pane/Custom plus live TextEditPattern; the same UI Automation element is compared again before SendInput. Any unknown state fails closed. The exact text is shown only in the local approval request, while persistent approval history and structured tool results retain only length and SHA-256. Approval additionally requires the user to type a one-time random confirmation challenge and can never be remembered for the session. A transient post-action screenshot may visually contain the updated control but is never persisted.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "text": {"type": "string", "minLength": 1, "maxLength": 2048}
                    },
                    "required": ["text"],
                    "additionalProperties": false
                }
            }),
            json!({
                "name": "computer_scroll",
                "description": "Post one bounded scroll event at the current pointer target on the current supported platform. Positive delta_y scrolls up and positive delta_x scrolls right. Every exact scroll requires explicit local user approval. A best-effort transient post-action screenshot is attached and observation failure never triggers an automatic repeat.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "delta_y": {"type": "integer", "minimum": -MAX_SCROLL_DELTA, "maximum": MAX_SCROLL_DELTA, "default": 0},
                        "delta_x": {"type": "integer", "minimum": -MAX_SCROLL_DELTA, "maximum": MAX_SCROLL_DELTA, "default": 0}
                    },
                    "additionalProperties": false
                }
            }),
            json!({
                "name": "computer_activate_application",
                "description": "Bring one already-running application process to the front by its PID from computer_list_windows. The Local Connector resolves the real process identity before showing the mandatory approval and rechecks it during execution; model-provided application names are not accepted. If the action is cancelled while still in flight after activation, ChatOS attempts to restore the exact previous foreground application only when the approved target remains foreground and both identities still match; a user or system foreground change disables rollback. This recovery does not undo application content or arbitrary window changes. A best-effort transient post-action screenshot is attached and activation is not replayed if observation fails.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "pid": {"type": "integer", "minimum": 1, "maximum": 2147483647}
                    },
                    "required": ["pid"],
                    "additionalProperties": false
                }
            }),
            json!({
                "name": "computer_set_frontmost_window_bounds",
                "description": "Move and resize only the current frontmost non-fullscreen, non-maximized window to one reviewed global desktop rectangle. Approval binds the exact process, native window identity, original state and geometry, and requested rectangle. The target must leave at least 64 x 64 desktop units visible on one active display. Identity, foreground, state, capability, display-layout, or geometry drift fails closed; partial platform failures attempt an identity-bound restoration and are never automatically replayed. After settling, ChatOS revalidates the requested state and captures the exact frontmost window rather than assuming it remained on the main display.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "x": {"type": "integer", "minimum": MIN_WINDOW_COORDINATE, "maximum": MAX_WINDOW_COORDINATE, "description": "Global desktop x coordinate for the window's top-left corner."},
                        "y": {"type": "integer", "minimum": MIN_WINDOW_COORDINATE, "maximum": MAX_WINDOW_COORDINATE, "description": "Global desktop y coordinate for the window's top-left corner."},
                        "width": {"type": "integer", "minimum": MIN_WINDOW_DIMENSION, "maximum": MAX_WINDOW_DIMENSION},
                        "height": {"type": "integer", "minimum": MIN_WINDOW_DIMENSION, "maximum": MAX_WINDOW_DIMENSION}
                    },
                    "required": ["x", "y", "width", "height"],
                    "additionalProperties": false
                }
            }),
            json!({
                "name": "computer_set_frontmost_window_fullscreen",
                "description": "macOS only: set the exact current frontmost Accessibility window's native AXFullScreen state. Approval binds its process, AX window number, original geometry/state, and requested state. The AXFullScreen attribute must be explicitly writable, and foreground or identity drift fails closed. This does not simulate the green button or send a keyboard shortcut. Post-action observation revalidates the exact window and requested fullscreen state before and after capturing that window.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "fullscreen": {"type": "boolean"}
                    },
                    "required": ["fullscreen"],
                    "additionalProperties": false
                }
            }),
            json!({
                "name": "computer_set_frontmost_window_maximized",
                "description": "Windows only: maximize or restore the exact current foreground HWND. This is standard Windows maximize/restore, not true application fullscreen. Approval binds HWND, PID/process image, original geometry/state, and requested state; foreground, identity, state, or geometry drift fails closed and cancellation attempts to restore the approved prior state. Post-action observation captures only that exact foreground window after revalidating its requested maximize state.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "maximized": {"type": "boolean"}
                    },
                    "required": ["maximized"],
                    "additionalProperties": false
                }
            }),
            json!({
                "name": "computer_restore_window_layout",
                "description": "Restore one exact short-lived layout snapshot created by computer_capture_window_layout. The request accepts only the opaque snapshot ID and its SHA-256, never PID, HWND/AX window ID, application identity, or coordinates. A fresh local approval plus one-time typed confirmation is mandatory. Display-layout drift or any missing/changed/non-ordinary window fails the whole batch before mutation; partial execution rolls back only windows changed by this batch whose exact identity and target geometry still match. Application content, navigation, text, and document state are never rolled back, and automatic replay is always unsafe.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "snapshot_id": {"type": "string", "minLength": 36, "maxLength": 36},
                        "snapshot_sha256": {"type": "string", "pattern": "^[0-9a-f]{64}$"}
                    },
                    "required": ["snapshot_id", "snapshot_sha256"],
                    "additionalProperties": false
                }
            }),
        ]);
    }
    filter_tools_for_platform(&mut tools, platform);
    tools
}

fn filter_tools_for_platform(tools: &mut Vec<Value>, platform: &str) {
    tools.retain(|tool| {
        let name = tool.get("name").and_then(Value::as_str).unwrap_or_default();
        match name {
            "computer_set_frontmost_window_fullscreen" => platform == "macos",
            "computer_set_frontmost_window_maximized" => platform == "windows",
            _ => true,
        }
    });
}
