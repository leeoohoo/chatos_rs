// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

pub(super) const FRONTMOST_WINDOW_CONTROL_TARGET_JXA: &str = r#"
function safe(callable, fallback) {
  try { return callable(); } catch (_) { return fallback; }
}
function text(value, maxLength) {
  var output = value === undefined || value === null ? "" : String(value);
  return output.length <= maxLength ? output : output.slice(0, maxLength);
}
function pair(value) {
  try {
    var first = Number(value[0]);
    var second = Number(value[1]);
    if (!Number.isFinite(first) || !Number.isFinite(second)) return null;
    return [first, second];
  } catch (_) {
    return null;
  }
}
function attribute(window, name) {
  return safe(function() { return window.attributes.byName(name); }, null);
}
function attributeValue(window, name, fallback) {
  var candidate = attribute(window, name);
  return candidate === null ? fallback : safe(function() { return candidate.value(); }, fallback);
}
function attributeSettable(window, name) {
  var candidate = attribute(window, name);
  return candidate !== null && Boolean(safe(function() { return candidate.settable(); }, false));
}
function run() {
  var systemEvents = Application("System Events");
  var processes = systemEvents.applicationProcesses.whose({frontmost: true})();
  if (processes.length !== 1) throw new Error("A unique frontmost application process is required");
  var process = processes[0];
  var windows = safe(function() { return process.windows(); }, []);
  if (windows.length === 0) throw new Error("The frontmost application has no controllable window");
  var window = windows[0];
  var visible = Boolean(safe(function() { return window.visible(); }, false));
  var minimized = Boolean(attributeValue(window, "AXMinimized", false));
  var windowId = Number(attributeValue(window, "AXWindowNumber", safe(function() { return window.id(); }, 0)));
  var pid = Number(safe(function() { return process.unixId(); }, 0));
  var position = pair(safe(function() { return window.position(); }, null));
  var size = pair(safe(function() { return window.size(); }, null));
  var fullscreenAttribute = attribute(window, "AXFullScreen");
  var fullscreen = fullscreenAttribute === null ? false : Boolean(safe(function() { return fullscreenAttribute.value(); }, false));
  if (!visible || minimized) throw new Error("The frontmost window is not visibly controllable");
  if (!Number.isFinite(windowId) || windowId < 1 || Math.floor(windowId) !== windowId) {
    throw new Error("The frontmost window identity is invalid");
  }
  if (!Number.isFinite(pid) || pid < 1 || Math.floor(pid) !== pid) {
    throw new Error("The frontmost application identity is invalid");
  }
  if (position === null || size === null || size[0] <= 0 || size[1] <= 0) {
    throw new Error("The frontmost window geometry is invalid");
  }
  if (!Boolean(process.frontmost())) throw new Error("The frontmost application changed during observation");
  return JSON.stringify({
    platform: "macos",
    application: text(safe(function() { return process.name(); }, ""), 240),
    pid: pid,
    window_id: String(windowId),
    title: text(safe(function() { return window.name(); }, ""), 500),
    position: position,
    size: size,
    fullscreen: fullscreen,
    maximized: null,
    position_settable: attributeSettable(window, "AXPosition"),
    size_settable: attributeSettable(window, "AXSize"),
    fullscreen_settable: fullscreenAttribute !== null && attributeSettable(window, "AXFullScreen")
  });
}
"#;

pub(super) const SET_FRONTMOST_WINDOW_BOUNDS_JXA: &str = r#"
function safe(callable, fallback) {
  try { return callable(); } catch (_) { return fallback; }
}
function pair(value) {
  try {
    var first = Number(value[0]);
    var second = Number(value[1]);
    if (!Number.isFinite(first) || !Number.isFinite(second)) return null;
    return [first, second];
  } catch (_) { return null; }
}
function attribute(window, name) {
  return safe(function() { return window.attributes.byName(name); }, null);
}
function attributeValue(window, name, fallback) {
  var candidate = attribute(window, name);
  return candidate === null ? fallback : safe(function() { return candidate.value(); }, fallback);
}
function attributeSettable(window, name) {
  var candidate = attribute(window, name);
  return candidate !== null && Boolean(safe(function() { return candidate.settable(); }, false));
}
function currentTarget() {
  var systemEvents = Application("System Events");
  var processes = systemEvents.applicationProcesses.whose({frontmost: true})();
  if (processes.length !== 1) return null;
  var process = processes[0];
  var windows = safe(function() { return process.windows(); }, []);
  if (windows.length === 0) return null;
  var window = windows[0];
  var position = pair(safe(function() { return window.position(); }, null));
  var size = pair(safe(function() { return window.size(); }, null));
  var fullscreenAttribute = attribute(window, "AXFullScreen");
  if (position === null || size === null) return null;
  return {
    process: process,
    window: window,
    application: String(safe(function() { return process.name(); }, "")),
    pid: Number(safe(function() { return process.unixId(); }, 0)),
    window_id: String(Number(attributeValue(window, "AXWindowNumber", safe(function() { return window.id(); }, 0)))),
    position: position,
    size: size,
    fullscreen: fullscreenAttribute === null ? false : Boolean(safe(function() { return fullscreenAttribute.value(); }, false)),
    position_settable: attributeSettable(window, "AXPosition"),
    size_settable: attributeSettable(window, "AXSize"),
    fullscreen_settable: fullscreenAttribute !== null && attributeSettable(window, "AXFullScreen"),
    visible: Boolean(safe(function() { return window.visible(); }, false)),
    minimized: Boolean(attributeValue(window, "AXMinimized", false)),
    frontmost: Boolean(safe(function() { return process.frontmost(); }, false))
  };
}
function equalPair(left, right) {
  return left !== null && right !== null && left[0] === right[0] && left[1] === right[1];
}
function matchesApproved(target, approved) {
  return target !== null && target.frontmost && target.visible && !target.minimized &&
    target.application === approved.application && target.pid === approved.pid &&
    target.window_id === approved.window_id && equalPair(target.position, approved.position) &&
    equalPair(target.size, approved.size) && target.fullscreen === approved.fullscreen &&
    target.position_settable === approved.position_settable &&
    target.size_settable === approved.size_settable &&
    target.fullscreen_settable === approved.fullscreen_settable;
}
function identityMatches(target, approved) {
  return target !== null && target.frontmost && target.visible && !target.minimized &&
    target.application === approved.application && target.pid === approved.pid &&
    target.window_id === approved.window_id;
}
function recoveryResult(approved) {
  var current = currentTarget();
  if (!identityMatches(current, approved)) {
    return {attempted: false, restored: false, reason: "foreground_or_identity_changed"};
  }
  try {
    current.window.size.set(approved.size);
    current.window.position.set(approved.position);
  } catch (_) {
    return {attempted: true, restored: false, reason: "platform_restore_failed"};
  }
  var restored = currentTarget();
  var exact = matchesApproved(restored, approved);
  return {attempted: true, restored: exact, reason: exact ? "original_geometry_restored" : "restore_readback_mismatch"};
}
function run(argv) {
  var approved = JSON.parse(argv[0]);
  var requested = JSON.parse(argv[1]);
  var before = currentTarget();
  if (!matchesApproved(before, approved)) {
    throw new Error("The approved frontmost window identity, state, capability, or geometry changed before bounds control");
  }
  if (before.fullscreen || !before.position_settable || !before.size_settable) {
    throw new Error("The approved frontmost window is not safely movable and resizable");
  }
  try {
    before.window.size.set([requested.width, requested.height]);
    before.window.position.set([requested.x, requested.y]);
  } catch (_) {
    return JSON.stringify({
      success: false,
      mode: "approved_input",
      action: "set_frontmost_window_bounds",
      target_geometry_applied: false,
      action_already_executed: true,
      automatic_replay_safe: false,
      failure_reason: "platform_apply_failed",
      window_geometry_recovery: recoveryResult(approved)
    });
  }
  var after = currentTarget();
  var exact = identityMatches(after, approved) &&
    equalPair(after.position, [requested.x, requested.y]) &&
    equalPair(after.size, [requested.width, requested.height]) && !after.fullscreen;
  if (!exact) {
    return JSON.stringify({
      success: false,
      mode: "approved_input",
      action: "set_frontmost_window_bounds",
      target_geometry_applied: false,
      action_already_executed: true,
      automatic_replay_safe: false,
      failure_reason: "target_geometry_readback_mismatch",
      window_geometry_recovery: recoveryResult(approved)
    });
  }
  return JSON.stringify({
    success: true,
    mode: "approved_input",
    action: "set_frontmost_window_bounds",
    platform: "macos",
    application: approved.application,
    pid: approved.pid,
    window_id: approved.window_id,
    original_position: approved.position,
    original_size: approved.size,
    position: after.position,
    size: after.size,
    target_geometry_applied: true,
    identity_and_geometry_revalidated_after_action: true,
    window_geometry_recovery: {attempted: false, restored: false, reason: "action_completed"}
  });
}
"#;

pub(super) const RESTORE_FRONTMOST_WINDOW_BOUNDS_JXA: &str = r#"
function safe(callable, fallback) { try { return callable(); } catch (_) { return fallback; } }
function pair(value) {
  try {
    var first = Number(value[0]); var second = Number(value[1]);
    return Number.isFinite(first) && Number.isFinite(second) ? [first, second] : null;
  } catch (_) { return null; }
}
function attributeValue(window, name, fallback) {
  return safe(function() { return window.attributes.byName(name).value(); }, fallback);
}
function currentTarget() {
  var systemEvents = Application("System Events");
  var processes = systemEvents.applicationProcesses.whose({frontmost: true})();
  if (processes.length !== 1) return null;
  var process = processes[0]; var windows = safe(function() { return process.windows(); }, []);
  if (windows.length === 0) return null;
  var window = windows[0];
  return {
    process: process, window: window,
    application: String(safe(function() { return process.name(); }, "")),
    pid: Number(safe(function() { return process.unixId(); }, 0)),
    window_id: String(Number(attributeValue(window, "AXWindowNumber", safe(function() { return window.id(); }, 0)))),
    position: pair(safe(function() { return window.position(); }, null)),
    size: pair(safe(function() { return window.size(); }, null)),
    frontmost: Boolean(safe(function() { return process.frontmost(); }, false))
  };
}
function equalPair(left, right) { return left !== null && left[0] === right[0] && left[1] === right[1]; }
function identityMatches(target, approved) {
  return target !== null && target.frontmost && target.application === approved.application &&
    target.pid === approved.pid && target.window_id === approved.window_id;
}
function run(argv) {
  var approved = JSON.parse(argv[0]); var requested = JSON.parse(argv[1]);
  var current = currentTarget();
  if (!identityMatches(current, approved) || !equalPair(current.position, [requested.x, requested.y]) ||
      !equalPair(current.size, [requested.width, requested.height])) {
    return JSON.stringify({attempted: false, restored: false, reason: "foreground_identity_or_target_geometry_changed"});
  }
  try { current.window.size.set(approved.size); current.window.position.set(approved.position); }
  catch (_) { return JSON.stringify({attempted: true, restored: false, reason: "platform_restore_failed"}); }
  var after = currentTarget();
  var restored = identityMatches(after, approved) && equalPair(after.position, approved.position) && equalPair(after.size, approved.size);
  return JSON.stringify({attempted: true, restored: restored, reason: restored ? "cancelled_action_restored" : "restore_readback_mismatch"});
}
"#;

pub(super) const SET_FRONTMOST_WINDOW_FULLSCREEN_JXA: &str = r#"
function safe(callable, fallback) { try { return callable(); } catch (_) { return fallback; } }
function pair(value) {
  try {
    var first = Number(value[0]); var second = Number(value[1]);
    return Number.isFinite(first) && Number.isFinite(second) ? [first, second] : null;
  } catch (_) { return null; }
}
function attribute(window, name) { return safe(function() { return window.attributes.byName(name); }, null); }
function attributeValue(window, name, fallback) {
  var candidate = attribute(window, name);
  return candidate === null ? fallback : safe(function() { return candidate.value(); }, fallback);
}
function attributeSettable(window, name) {
  var candidate = attribute(window, name);
  return candidate !== null && Boolean(safe(function() { return candidate.settable(); }, false));
}
function currentTarget() {
  var systemEvents = Application("System Events");
  var processes = systemEvents.applicationProcesses.whose({frontmost: true})();
  if (processes.length !== 1) return null;
  var process = processes[0]; var windows = safe(function() { return process.windows(); }, []);
  if (windows.length === 0) return null;
  var window = windows[0]; var fullscreenAttribute = attribute(window, "AXFullScreen");
  return {
    process: process, window: window, fullscreen_attribute: fullscreenAttribute,
    application: String(safe(function() { return process.name(); }, "")),
    pid: Number(safe(function() { return process.unixId(); }, 0)),
    window_id: String(Number(attributeValue(window, "AXWindowNumber", safe(function() { return window.id(); }, 0)))),
    position: pair(safe(function() { return window.position(); }, null)),
    size: pair(safe(function() { return window.size(); }, null)),
    fullscreen: fullscreenAttribute === null ? false : Boolean(safe(function() { return fullscreenAttribute.value(); }, false)),
    position_settable: attributeSettable(window, "AXPosition"),
    size_settable: attributeSettable(window, "AXSize"),
    fullscreen_settable: fullscreenAttribute !== null && attributeSettable(window, "AXFullScreen"),
    visible: Boolean(safe(function() { return window.visible(); }, false)),
    minimized: Boolean(attributeValue(window, "AXMinimized", false)),
    frontmost: Boolean(safe(function() { return process.frontmost(); }, false))
  };
}
function equalPair(left, right) { return left !== null && left[0] === right[0] && left[1] === right[1]; }
function matchesApproved(target, approved) {
  return target !== null && target.frontmost && target.visible && !target.minimized &&
    target.application === approved.application && target.pid === approved.pid &&
    target.window_id === approved.window_id && equalPair(target.position, approved.position) &&
    equalPair(target.size, approved.size) && target.fullscreen === approved.fullscreen &&
    target.position_settable === approved.position_settable && target.size_settable === approved.size_settable &&
    target.fullscreen_settable === approved.fullscreen_settable;
}
function identityMatches(target, approved) {
  return target !== null && target.frontmost && target.visible && !target.minimized &&
    target.application === approved.application && target.pid === approved.pid && target.window_id === approved.window_id;
}
function waitForState(approved, expected) {
  for (var index = 0; index < 20; index += 1) {
    var current = currentTarget();
    if (!identityMatches(current, approved)) return current;
    if (current.fullscreen === expected) return current;
    delay(0.04);
  }
  return currentTarget();
}
function restoreState(approved) {
  var current = currentTarget();
  if (!identityMatches(current, approved) || current.fullscreen_attribute === null || !current.fullscreen_settable) {
    return {attempted: false, restored: false, reason: "foreground_identity_or_capability_changed"};
  }
  try { current.fullscreen_attribute.value.set(approved.fullscreen); }
  catch (_) { return {attempted: true, restored: false, reason: "platform_restore_failed"}; }
  var restored = waitForState(approved, approved.fullscreen);
  var exact = identityMatches(restored, approved) && restored.fullscreen === approved.fullscreen;
  return {attempted: true, restored: exact, reason: exact ? "original_fullscreen_state_restored" : "restore_readback_mismatch"};
}
function run(argv) {
  var approved = JSON.parse(argv[0]); var requested = argv[1] === "true";
  var before = currentTarget();
  if (!matchesApproved(before, approved)) {
    throw new Error("The approved frontmost window identity, state, capability, or geometry changed before fullscreen control");
  }
  if (before.fullscreen_attribute === null || !before.fullscreen_settable || before.fullscreen === requested) {
    throw new Error("The approved frontmost window fullscreen transition is unavailable");
  }
  try { before.fullscreen_attribute.value.set(requested); }
  catch (_) {
    return JSON.stringify({
      success: false, mode: "approved_input", action: "set_frontmost_window_fullscreen",
      target_fullscreen_applied: false, action_already_executed: true, automatic_replay_safe: false,
      failure_reason: "platform_apply_failed", window_state_recovery: restoreState(approved)
    });
  }
  var after = waitForState(approved, requested);
  if (!identityMatches(after, approved) || after.fullscreen !== requested) {
    return JSON.stringify({
      success: false, mode: "approved_input", action: "set_frontmost_window_fullscreen",
      target_fullscreen_applied: false, action_already_executed: true, automatic_replay_safe: false,
      failure_reason: "target_state_readback_mismatch", window_state_recovery: restoreState(approved)
    });
  }
  return JSON.stringify({
    success: true, mode: "approved_input", action: "set_frontmost_window_fullscreen", platform: "macos",
    application: approved.application, pid: approved.pid, window_id: approved.window_id,
    original_fullscreen: approved.fullscreen, fullscreen: after.fullscreen,
    position: after.position, size: after.size, target_fullscreen_applied: true,
    identity_and_state_revalidated_after_action: true,
    window_state_recovery: {attempted: false, restored: false, reason: "action_completed"}
  });
}
"#;

pub(super) const RESTORE_FRONTMOST_WINDOW_FULLSCREEN_JXA: &str = r#"
function safe(callable, fallback) { try { return callable(); } catch (_) { return fallback; } }
function attributeValue(window, name, fallback) { return safe(function() { return window.attributes.byName(name).value(); }, fallback); }
function currentTarget() {
  var systemEvents = Application("System Events"); var processes = systemEvents.applicationProcesses.whose({frontmost: true})();
  if (processes.length !== 1) return null;
  var process = processes[0]; var windows = safe(function() { return process.windows(); }, []); if (windows.length === 0) return null;
  var window = windows[0]; var fullscreenAttribute = safe(function() { return window.attributes.byName("AXFullScreen"); }, null);
  return {
    window: window, fullscreen_attribute: fullscreenAttribute,
    application: String(safe(function() { return process.name(); }, "")),
    pid: Number(safe(function() { return process.unixId(); }, 0)),
    window_id: String(Number(attributeValue(window, "AXWindowNumber", safe(function() { return window.id(); }, 0)))),
    fullscreen: fullscreenAttribute === null ? false : Boolean(safe(function() { return fullscreenAttribute.value(); }, false)),
    frontmost: Boolean(safe(function() { return process.frontmost(); }, false))
  };
}
function identityMatches(target, approved) {
  return target !== null && target.frontmost && target.application === approved.application &&
    target.pid === approved.pid && target.window_id === approved.window_id;
}
function run(argv) {
  var approved = JSON.parse(argv[0]); var requested = argv[1] === "true"; var current = currentTarget();
  if (!identityMatches(current, approved) || current.fullscreen !== requested || current.fullscreen_attribute === null) {
    return JSON.stringify({attempted: false, restored: false, reason: "foreground_identity_or_target_state_changed"});
  }
  try { current.fullscreen_attribute.value.set(approved.fullscreen); }
  catch (_) { return JSON.stringify({attempted: true, restored: false, reason: "platform_restore_failed"}); }
  for (var index = 0; index < 20; index += 1) {
    var after = currentTarget();
    if (!identityMatches(after, approved) || after.fullscreen === approved.fullscreen) break;
    delay(0.04);
  }
  var restored = currentTarget(); var exact = identityMatches(restored, approved) && restored.fullscreen === approved.fullscreen;
  return JSON.stringify({attempted: true, restored: exact, reason: exact ? "cancelled_action_restored" : "restore_readback_mismatch"});
}
"#;
