// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

pub(super) const LIST_WINDOWS_JXA: &str = r#"
function text(value, maxLength) {
  var output = value === undefined || value === null ? "" : String(value);
  return output.length <= maxLength ? output : output.slice(0, maxLength);
}
function pair(value) {
  try {
    return [Number(value[0]), Number(value[1])];
  } catch (_) {
    return null;
  }
}
function run(argv) {
  var limit = Number.parseInt(argv[0] || "40", 10);
  if (!Number.isFinite(limit) || limit < 1) limit = 40;
  limit = Math.min(limit, 100);
  var systemEvents = Application("System Events");
  var processes = systemEvents.applicationProcesses.whose({backgroundOnly: false})();
  var rows = [];
  for (var processIndex = 0; processIndex < processes.length && rows.length < limit; processIndex += 1) {
    var process = processes[processIndex];
    var frontmost = false;
    var processName = "";
    var processId = null;
    try { frontmost = Boolean(process.frontmost()); } catch (_) {}
    try { processName = text(process.name(), 240); } catch (_) {}
    try { processId = Number(process.unixId()); } catch (_) {}
    var windows = [];
    try {
      var processWindows = process.windows();
      for (var windowIndex = 0; windowIndex < processWindows.length && windows.length < 20; windowIndex += 1) {
        var window = processWindows[windowIndex];
        var title = "";
        var position = null;
        var size = null;
        try { title = text(window.name(), 500); } catch (_) {}
        try { position = pair(window.position()); } catch (_) {}
        try { size = pair(window.size()); } catch (_) {}
        windows.push({title: title, position: position, size: size});
      }
    } catch (_) {}
    if (frontmost || windows.length > 0) {
      rows.push({name: processName, pid: processId, frontmost: frontmost, windows: windows});
    }
  }
  rows.sort(function(left, right) {
    if (left.frontmost === right.frontmost) return left.name.localeCompare(right.name);
    return left.frontmost ? -1 : 1;
  });
  return JSON.stringify({platform: "macos", process_count: rows.length, processes: rows});
}
"#;

pub(super) const CAPTURE_WINDOW_LAYOUT_JXA: &str = r#"
function safe(callable, fallback) { try { return callable(); } catch (_) { return fallback; } }
function text(value, maxLength) {
  var output = value === undefined || value === null ? "" : String(value);
  return output.length <= maxLength ? output : output.slice(0, maxLength);
}
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
function processIdentity(process) {
  var bundleIdentifier = text(safe(function() { return process.bundleIdentifier(); }, ""), 480);
  return bundleIdentifier ? "bundle:" + bundleIdentifier : "";
}
function run(argv) {
  var maximum = Number.parseInt(argv[0] || "8", 10);
  if (!Number.isFinite(maximum) || maximum < 1 || maximum > 8) throw new Error("Window layout limit is invalid");
  var systemEvents = Application("System Events");
  var processes = systemEvents.applicationProcesses.whose({backgroundOnly: false})();
  var rows = []; var excluded = 0; var truncated = false;
  for (var processIndex = 0; processIndex < processes.length && !truncated; processIndex += 1) {
    var process = processes[processIndex];
    var application = text(safe(function() { return process.name(); }, ""), 240);
    var pid = Number(safe(function() { return process.unixId(); }, 0));
    var identity = processIdentity(process);
    var windows = safe(function() { return process.windows(); }, []);
    for (var windowIndex = 0; windowIndex < windows.length; windowIndex += 1) {
      var window = windows[windowIndex];
      var windowId = Number(attributeValue(window, "AXWindowNumber", safe(function() { return window.id(); }, 0)));
      var position = pair(safe(function() { return window.position(); }, null));
      var size = pair(safe(function() { return window.size(); }, null));
      var subrole = text(safe(function() { return window.subrole(); }, ""), 120);
      var visible = Boolean(safe(function() { return window.visible(); }, false));
      var minimized = Boolean(attributeValue(window, "AXMinimized", false));
      var fullscreen = Boolean(attributeValue(window, "AXFullScreen", false));
      var eligible = application && identity && Number.isFinite(pid) && pid >= 1 && Math.floor(pid) === pid &&
        Number.isFinite(windowId) && windowId >= 1 && Math.floor(windowId) === windowId &&
        subrole === "AXStandardWindow" && visible && !minimized && !fullscreen &&
        position !== null && size !== null && size[0] >= 64 && size[1] >= 64 &&
        attributeSettable(window, "AXPosition") && attributeSettable(window, "AXSize");
      if (!eligible) { excluded += 1; continue; }
      if (rows.length >= maximum) { truncated = true; break; }
      rows.push({
        platform: "macos", application: application, process_identity: identity, pid: pid,
        window_id: String(windowId), position: position, size: size
      });
    }
  }
  if (rows.length === 0) throw new Error("No ordinary restorable macOS windows are available");
  return JSON.stringify({platform: "macos", windows: rows, excluded_window_count: excluded, truncated: truncated});
}
"#;

pub(super) const PREFLIGHT_WINDOW_LAYOUT_JXA: &str = r#"
function safe(callable, fallback) { try { return callable(); } catch (_) { return fallback; } }
function pair(value) {
  try { var a = Number(value[0]); var b = Number(value[1]); return Number.isFinite(a) && Number.isFinite(b) ? [a, b] : null; }
  catch (_) { return null; }
}
function attribute(window, name) { return safe(function() { return window.attributes.byName(name); }, null); }
function attributeValue(window, name, fallback) { var candidate = attribute(window, name); return candidate === null ? fallback : safe(function() { return candidate.value(); }, fallback); }
function attributeSettable(window, name) { var candidate = attribute(window, name); return candidate !== null && Boolean(safe(function() { return candidate.settable(); }, false)); }
function processIdentity(process) { var value = String(safe(function() { return process.bundleIdentifier(); }, "")); return value ? "bundle:" + value : ""; }
function currentWindow(systemEvents, guard) {
  var processes = systemEvents.applicationProcesses(); var process = null;
  for (var p = 0; p < processes.length; p += 1) {
    var candidate = processes[p];
    if (Number(safe(function() { return candidate.unixId(); }, 0)) === guard.pid &&
        String(safe(function() { return candidate.name(); }, "")) === guard.application &&
        processIdentity(candidate) === guard.process_identity) { process = candidate; break; }
  }
  if (process === null) return null;
  var windows = safe(function() { return process.windows(); }, []);
  for (var w = 0; w < windows.length; w += 1) {
    var window = windows[w];
    var id = String(Number(attributeValue(window, "AXWindowNumber", safe(function() { return window.id(); }, 0))));
    if (id !== guard.window_id) continue;
    var position = pair(safe(function() { return window.position(); }, null));
    var size = pair(safe(function() { return window.size(); }, null));
    var ordinary = String(safe(function() { return window.subrole(); }, "")) === "AXStandardWindow" &&
      Boolean(safe(function() { return window.visible(); }, false)) && !Boolean(attributeValue(window, "AXMinimized", false)) &&
      !Boolean(attributeValue(window, "AXFullScreen", false)) && attributeSettable(window, "AXPosition") && attributeSettable(window, "AXSize");
    return ordinary && position !== null && size !== null && size[0] >= 64 && size[1] >= 64 ? {position: position, size: size} : null;
  }
  return null;
}
function run(argv) {
  var snapshot = JSON.parse(argv[0]); var systemEvents = Application("System Events");
  for (var index = 0; index < snapshot.windows.length; index += 1) {
    if (currentWindow(systemEvents, snapshot.windows[index]) === null) {
      throw new Error("A snapshotted macOS window identity, capability, or ordinary-window state changed before approval");
    }
  }
  return JSON.stringify({validated: true, window_count: snapshot.windows.length});
}
"#;

pub(super) const RESTORE_WINDOW_LAYOUT_JXA: &str = r#"
function safe(callable, fallback) { try { return callable(); } catch (_) { return fallback; } }
function pair(value) { try { var a = Number(value[0]); var b = Number(value[1]); return Number.isFinite(a) && Number.isFinite(b) ? [a, b] : null; } catch (_) { return null; } }
function equalPair(left, right) { return left !== null && right !== null && left[0] === right[0] && left[1] === right[1]; }
function attribute(window, name) { return safe(function() { return window.attributes.byName(name); }, null); }
function attributeValue(window, name, fallback) { var candidate = attribute(window, name); return candidate === null ? fallback : safe(function() { return candidate.value(); }, fallback); }
function attributeSettable(window, name) { var candidate = attribute(window, name); return candidate !== null && Boolean(safe(function() { return candidate.settable(); }, false)); }
function processIdentity(process) { var value = String(safe(function() { return process.bundleIdentifier(); }, "")); return value ? "bundle:" + value : ""; }
function currentWindow(systemEvents, guard) {
  var processes = systemEvents.applicationProcesses(); var process = null;
  for (var p = 0; p < processes.length; p += 1) {
    var candidate = processes[p];
    if (Number(safe(function() { return candidate.unixId(); }, 0)) === guard.pid && String(safe(function() { return candidate.name(); }, "")) === guard.application && processIdentity(candidate) === guard.process_identity) { process = candidate; break; }
  }
  if (process === null) return null;
  var windows = safe(function() { return process.windows(); }, []);
  for (var w = 0; w < windows.length; w += 1) {
    var window = windows[w]; var id = String(Number(attributeValue(window, "AXWindowNumber", safe(function() { return window.id(); }, 0))));
    if (id !== guard.window_id) continue;
    var position = pair(safe(function() { return window.position(); }, null)); var size = pair(safe(function() { return window.size(); }, null));
    var ordinary = String(safe(function() { return window.subrole(); }, "")) === "AXStandardWindow" && Boolean(safe(function() { return window.visible(); }, false)) &&
      !Boolean(attributeValue(window, "AXMinimized", false)) && !Boolean(attributeValue(window, "AXFullScreen", false)) &&
      attributeSettable(window, "AXPosition") && attributeSettable(window, "AXSize");
    return ordinary && position !== null && size !== null && size[0] >= 64 && size[1] >= 64 ? {window: window, position: position, size: size} : null;
  }
  return null;
}
function rollback(systemEvents, snapshot, before, applied) {
  var restored = 0; var skipped = 0; var failed = 0;
  for (var offset = applied.length - 1; offset >= 0; offset -= 1) {
    var index = applied[offset]; var current = currentWindow(systemEvents, snapshot.windows[index]);
    if (current === null || !equalPair(current.position, snapshot.windows[index].position) || !equalPair(current.size, snapshot.windows[index].size)) { skipped += 1; continue; }
    try { current.window.size.set(before[index].size); current.window.position.set(before[index].position); }
    catch (_) { failed += 1; continue; }
    var after = currentWindow(systemEvents, snapshot.windows[index]);
    if (after !== null && equalPair(after.position, before[index].position) && equalPair(after.size, before[index].size)) restored += 1; else failed += 1;
  }
  return {attempted: applied.length > 0, restored_count: restored, skipped_count: skipped, failed_count: failed, complete: restored === applied.length};
}
function failure(reason, systemEvents, snapshot, before, applied, partialIndex) {
  var recovery = rollback(systemEvents, snapshot, before, applied);
  return JSON.stringify({
    success: false, mode: "approved_input", action: "restore_window_layout", platform: "macos",
    snapshot_id: snapshot.snapshot_id, snapshot_sha256: snapshot.snapshot_sha256,
    target_window_count: snapshot.windows.length, applied_window_count: applied.length,
    target_layout_retained: false,
    action_already_executed: applied.length > 0 || partialIndex !== null, automatic_replay_safe: false,
    failure_reason: reason, partial_window_index: partialIndex,
    window_layout_recovery: recovery,
    application_content_rollback: false, manual_review_required: partialIndex !== null || !recovery.complete
  });
}
function run(argv) {
  var snapshot = JSON.parse(argv[0]); var systemEvents = Application("System Events"); var before = [];
  for (var index = 0; index < snapshot.windows.length; index += 1) {
    var current = currentWindow(systemEvents, snapshot.windows[index]);
    if (current === null) throw new Error("A snapshotted macOS window identity, capability, or ordinary-window state changed before layout restore");
    before.push({platform: "macos", application: snapshot.windows[index].application, process_identity: snapshot.windows[index].process_identity,
      pid: snapshot.windows[index].pid, window_id: snapshot.windows[index].window_id, position: current.position, size: current.size});
  }
  var applied = [];
  for (var targetIndex = 0; targetIndex < snapshot.windows.length; targetIndex += 1) {
    var target = snapshot.windows[targetIndex]; var live = currentWindow(systemEvents, target);
    if (live === null || !equalPair(live.position, before[targetIndex].position) || !equalPair(live.size, before[targetIndex].size)) {
      return failure("window_drift_during_restore", systemEvents, snapshot, before, applied, null);
    }
    try { live.window.size.set(target.size); live.window.position.set(target.position); }
    catch (_) { return failure("platform_apply_failed", systemEvents, snapshot, before, applied, targetIndex); }
    var after = currentWindow(systemEvents, target);
    if (after === null || !equalPair(after.position, target.position) || !equalPair(after.size, target.size)) {
      return failure("target_geometry_readback_mismatch", systemEvents, snapshot, before, applied, targetIndex);
    }
    applied.push(targetIndex);
  }
  delay(0.16);
  for (var verifyIndex = 0; verifyIndex < snapshot.windows.length; verifyIndex += 1) {
    var verified = currentWindow(systemEvents, snapshot.windows[verifyIndex]);
    if (verified === null || !equalPair(verified.position, snapshot.windows[verifyIndex].position) || !equalPair(verified.size, snapshot.windows[verifyIndex].size)) {
      return failure("post_action_window_drift", systemEvents, snapshot, before, applied, null);
    }
  }
  return JSON.stringify({
    success: true, mode: "approved_input", action: "restore_window_layout", platform: "macos",
    snapshot_id: snapshot.snapshot_id, snapshot_sha256: snapshot.snapshot_sha256,
    target_window_count: snapshot.windows.length, restored_window_count: snapshot.windows.length,
    identity_geometry_and_display_layout_revalidated: true, automatic_replay_safe: false,
    application_content_rollback: false, pre_action_windows: before,
    window_layout_recovery: {attempted: false, restored_count: 0, skipped_count: 0, failed_count: 0, complete: false}
  });
}
"#;

pub(super) const ROLLBACK_WINDOW_LAYOUT_JXA: &str = r#"
function safe(callable, fallback) { try { return callable(); } catch (_) { return fallback; } }
function pair(value) { try { var a = Number(value[0]); var b = Number(value[1]); return Number.isFinite(a) && Number.isFinite(b) ? [a, b] : null; } catch (_) { return null; } }
function equalPair(left, right) { return left !== null && right !== null && left[0] === right[0] && left[1] === right[1]; }
function attribute(window, name) { return safe(function() { return window.attributes.byName(name); }, null); }
function attributeValue(window, name, fallback) { var candidate = attribute(window, name); return candidate === null ? fallback : safe(function() { return candidate.value(); }, fallback); }
function attributeSettable(window, name) { var candidate = attribute(window, name); return candidate !== null && Boolean(safe(function() { return candidate.settable(); }, false)); }
function processIdentity(process) { var value = String(safe(function() { return process.bundleIdentifier(); }, "")); return value ? "bundle:" + value : ""; }
function currentWindow(systemEvents, guard) {
  var processes = systemEvents.applicationProcesses();
  for (var p = 0; p < processes.length; p += 1) {
    var process = processes[p];
    if (Number(safe(function() { return process.unixId(); }, 0)) !== guard.pid || String(safe(function() { return process.name(); }, "")) !== guard.application || processIdentity(process) !== guard.process_identity) continue;
    var windows = safe(function() { return process.windows(); }, []);
    for (var w = 0; w < windows.length; w += 1) {
      var window = windows[w]; var id = String(Number(attributeValue(window, "AXWindowNumber", safe(function() { return window.id(); }, 0))));
      if (id !== guard.window_id) continue;
      var ordinary = String(safe(function() { return window.subrole(); }, "")) === "AXStandardWindow" && Boolean(safe(function() { return window.visible(); }, false)) &&
        !Boolean(attributeValue(window, "AXMinimized", false)) && !Boolean(attributeValue(window, "AXFullScreen", false)) &&
        attributeSettable(window, "AXPosition") && attributeSettable(window, "AXSize");
      if (!ordinary) return null;
      return {window: window, position: pair(safe(function() { return window.position(); }, null)), size: pair(safe(function() { return window.size(); }, null))};
    }
  }
  return null;
}
function run(argv) {
  var snapshot = JSON.parse(argv[0]); var before = JSON.parse(argv[1]); var systemEvents = Application("System Events");
  var restored = 0; var skipped = 0; var failed = 0;
  for (var index = snapshot.windows.length - 1; index >= 0; index -= 1) {
    var current = currentWindow(systemEvents, snapshot.windows[index]);
    if (current === null || !equalPair(current.position, snapshot.windows[index].position) || !equalPair(current.size, snapshot.windows[index].size)) { skipped += 1; continue; }
    try { current.window.size.set(before[index].size); current.window.position.set(before[index].position); }
    catch (_) { failed += 1; continue; }
    var after = currentWindow(systemEvents, snapshot.windows[index]);
    if (after !== null && equalPair(after.position, before[index].position) && equalPair(after.size, before[index].size)) restored += 1; else failed += 1;
  }
  return JSON.stringify({attempted: true, restored_count: restored, skipped_count: skipped, failed_count: failed, complete: restored === snapshot.windows.length});
}
"#;

pub(super) const INSPECT_FRONTMOST_WINDOW_JXA: &str = r#"
function safe(callable, fallback) {
  try { return callable(); } catch (_) { return fallback; }
}
function attributeValue(element, name, fallback) {
  return safe(function() { return element.attributes.byName(name).value(); }, fallback);
}
function text(value, maxLength) {
  var output = value === undefined || value === null ? "" : String(value);
  return output.length <= maxLength ? output : output.slice(0, maxLength);
}
function editableValue(element, role) {
  if (role === "AXTextField" || role === "AXTextArea" || role === "AXComboBox" || role === "AXSearchField") {
    return true;
  }
  if (attributeValue(element, "AXIsEditable", false) === true) return true;
  return attributeValue(element, "AXEditableAncestor", null) !== null ||
    attributeValue(element, "AXHighestEditableAncestor", null) !== null;
}
function visibleValueAllowed(role, subrole) {
  var normalized = (String(role || "") + " " + String(subrole || "")).toLowerCase();
  if (normalized.indexOf("secure") >= 0 || normalized.indexOf("password") >= 0) return false;
  return role === "AXStaticText" || role === "AXButton" || role === "AXCheckBox" ||
    role === "AXRadioButton" || role === "AXMenuItem" || role === "AXPopUpButton" ||
    role === "AXSlider" || role === "AXProgressIndicator";
}
function run(argv) {
  var maxDepth = Number.parseInt(argv[0] || "4", 10);
  var maxNodes = Number.parseInt(argv[1] || "200", 10);
  if (!Number.isFinite(maxDepth) || maxDepth < 1) maxDepth = 4;
  if (!Number.isFinite(maxNodes) || maxNodes < 1) maxNodes = 200;
  maxDepth = Math.min(maxDepth, 6);
  maxNodes = Math.min(maxNodes, 400);
  var systemEvents = Application("System Events");
  var processes = systemEvents.applicationProcesses.whose({frontmost: true})();
  if (processes.length === 0) throw new Error("No frontmost application process is available");
  var process = processes[0];
  var windows = safe(function() { return process.windows(); }, []);
  if (windows.length === 0) throw new Error("The frontmost application has no inspectable window");
  var nodes = [];
  function visit(element, depth) {
    if (nodes.length >= maxNodes) return null;
    var role = text(safe(function() { return element.role(); }, ""), 120);
    var subrole = text(safe(function() { return element.subrole(); }, ""), 120);
    var editable = editableValue(element, role);
    var node = {
      ref: "u" + String(nodes.length + 1),
      role: role,
      subrole: subrole,
      name: text(safe(function() { return element.name(); }, ""), 500),
      description: text(safe(function() { return element.description(); }, ""), 500),
      enabled: Boolean(safe(function() { return element.enabled(); }, true)),
      children: []
    };
    if (editable) {
      node.editable = true;
      node.value_redacted = true;
    } else if (visibleValueAllowed(role, subrole)) {
      node.value = text(safe(function() { return element.value(); }, ""), 500);
    }
    nodes.push(node);
    if (depth < maxDepth && nodes.length < maxNodes) {
      var children = safe(function() { return element.uiElements(); }, []);
      for (var childIndex = 0; childIndex < children.length && nodes.length < maxNodes; childIndex += 1) {
        var child = visit(children[childIndex], depth + 1);
        if (child !== null) node.children.push(child);
      }
    }
    return node;
  }
  var tree = visit(windows[0], 0);
  return JSON.stringify({
    platform: "macos",
    application: text(safe(function() { return process.name(); }, ""), 240),
    pid: Number(safe(function() { return process.unixId(); }, 0)),
    window_title: text(safe(function() { return windows[0].name(); }, ""), 500),
    node_count: nodes.length,
    truncated: nodes.length >= maxNodes,
    text_entry_values_redacted: true,
    tree: tree
  });
}
"#;

pub(super) const FRONTMOST_WINDOW_CAPTURE_TARGET_JXA: &str = r#"
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
function attributeValue(element, name, fallback) {
  return safe(function() { return element.attributes.byName(name).value(); }, fallback);
}
function run() {
  var systemEvents = Application("System Events");
  var processes = systemEvents.applicationProcesses.whose({frontmost: true})();
  if (processes.length === 0) throw new Error("No frontmost application process is available");
  var process = processes[0];
  var windows = safe(function() { return process.windows(); }, []);
  if (windows.length === 0) throw new Error("The frontmost application has no capturable window");
  var window = windows[0];
  var visible = Boolean(safe(function() { return window.visible(); }, true));
  var minimized = Boolean(attributeValue(window, "AXMinimized", false));
  var windowId = Number(attributeValue(window, "AXWindowNumber", safe(function() { return window.id(); }, 0)));
  var pid = Number(safe(function() { return process.unixId(); }, 0));
  var position = pair(safe(function() { return window.position(); }, null));
  var size = pair(safe(function() { return window.size(); }, null));
  if (!visible || minimized) throw new Error("The frontmost window is not visibly capturable");
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
    window_id: windowId,
    position: position,
    size: size
  });
}
"#;
