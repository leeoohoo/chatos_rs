// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

pub(super) const LOOKUP_APPLICATION_JXA: &str = r#"
function text(value, maxLength) {
  var output = value === undefined || value === null ? "" : String(value);
  return output.length <= maxLength ? output : output.slice(0, maxLength);
}
function processForPid(systemEvents, pid) {
  var processes = systemEvents.applicationProcesses();
  for (var index = 0; index < processes.length; index += 1) {
    var candidate = processes[index];
    try {
      if (Number(candidate.unixId()) === pid) return candidate;
    } catch (_) {}
  }
  return null;
}
function run(argv) {
  var pid = Number.parseInt(argv[0] || "0", 10);
  if (!Number.isFinite(pid) || pid < 1) throw new Error("A positive application PID is required");
  var systemEvents = Application("System Events");
  var process = processForPid(systemEvents, pid);
  if (!process) throw new Error("The requested application process is no longer running");
  return JSON.stringify({
    application: text(process.name(), 240),
    pid: Number(process.unixId()),
    frontmost: Boolean(process.frontmost())
  });
}
"#;

pub(super) const ACTIVATE_APPLICATION_JXA: &str = r#"
function text(value, maxLength) {
  var output = value === undefined || value === null ? "" : String(value);
  return output.length <= maxLength ? output : output.slice(0, maxLength);
}
function processForPid(systemEvents, pid) {
  var processes = systemEvents.applicationProcesses();
  for (var index = 0; index < processes.length; index += 1) {
    var candidate = processes[index];
    try {
      if (Number(candidate.unixId()) === pid) return candidate;
    } catch (_) {}
  }
  return null;
}
function run(argv) {
  var pid = Number.parseInt(argv[0] || "0", 10);
  var expectedName = String(argv[1] || "");
  var previousPid = Number.parseInt(argv[2] || "0", 10);
  var previousName = String(argv[3] || "");
  if (!Number.isFinite(pid) || pid < 1) throw new Error("A positive application PID is required");
  if (!expectedName) throw new Error("An approved application identity is required");
  if (!Number.isFinite(previousPid) || previousPid < 1 || !previousName) {
    throw new Error("A valid previous foreground application identity is required");
  }
  var systemEvents = Application("System Events");
  var process = processForPid(systemEvents, pid);
  if (!process) throw new Error("The requested application process is no longer running");
  var actualName = text(process.name(), 240);
  if (actualName !== expectedName) throw new Error("The approved application identity changed before activation");
  var previous = processForPid(systemEvents, previousPid);
  if (!previous || text(previous.name(), 240) !== previousName || !Boolean(previous.frontmost())) {
    throw new Error("The frontmost application changed before activation");
  }
  process.frontmost.set(true);
  return JSON.stringify({
    application: actualName,
    pid: Number(process.unixId()),
    activated: true
  });
}
"#;

pub(super) const FRONTMOST_APPLICATION_JXA: &str = r#"
function text(value, maxLength) {
  var output = value === undefined || value === null ? "" : String(value);
  return output.length <= maxLength ? output : output.slice(0, maxLength);
}
function run() {
  var systemEvents = Application("System Events");
  var processes = systemEvents.applicationProcesses();
  for (var index = 0; index < processes.length; index += 1) {
    var candidate = processes[index];
    try {
      if (Boolean(candidate.frontmost())) {
        var pid = Number(candidate.unixId());
        var application = text(candidate.name(), 240);
        if (!Number.isFinite(pid) || pid < 1 || !application) {
          throw new Error("The frontmost application identity is invalid");
        }
        return JSON.stringify({application: application, pid: pid});
      }
    } catch (error) {
      if (String(error).indexOf("identity is invalid") >= 0) throw error;
    }
  }
  throw new Error("No frontmost application process is available");
}
"#;

pub(super) const RESTORE_APPLICATION_JXA: &str = r#"
function text(value, maxLength) {
  var output = value === undefined || value === null ? "" : String(value);
  return output.length <= maxLength ? output : output.slice(0, maxLength);
}
function processForPid(systemEvents, pid) {
  var processes = systemEvents.applicationProcesses();
  for (var index = 0; index < processes.length; index += 1) {
    var candidate = processes[index];
    try {
      if (Number(candidate.unixId()) === pid) return candidate;
    } catch (_) {}
  }
  return null;
}
function run(argv) {
  var previousPid = Number.parseInt(argv[0] || "0", 10);
  var previousName = String(argv[1] || "");
  var targetPid = Number.parseInt(argv[2] || "0", 10);
  var targetName = String(argv[3] || "");
  if (!Number.isFinite(previousPid) || previousPid < 1 || !previousName ||
      !Number.isFinite(targetPid) || targetPid < 1 || !targetName) {
    throw new Error("Application activation rollback identities are invalid");
  }
  if (previousPid === targetPid && previousName === targetName) {
    return JSON.stringify({attempted: false, restored: true, reason: "activation_did_not_change_frontmost_application"});
  }
  var systemEvents = Application("System Events");
  var target = processForPid(systemEvents, targetPid);
  if (!target || text(target.name(), 240) !== targetName || !Boolean(target.frontmost())) {
    return JSON.stringify({attempted: false, restored: false, reason: "foreground_changed_after_activation"});
  }
  var previous = processForPid(systemEvents, previousPid);
  if (!previous || text(previous.name(), 240) !== previousName) {
    return JSON.stringify({attempted: false, restored: false, reason: "previous_application_identity_unavailable"});
  }
  previous.frontmost.set(true);
  if (!Boolean(previous.frontmost())) {
    return JSON.stringify({attempted: true, restored: false, reason: "platform_refused_restore"});
  }
  return JSON.stringify({attempted: true, restored: true, reason: "cancelled_activation_restored"});
}
"#;
