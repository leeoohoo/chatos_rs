// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::atomic::AtomicBool;

use anyhow::{anyhow, Context, Result};
#[cfg(target_os = "macos")]
use serde_json::json;
use serde_json::Value;

use super::reject_unknown_fields;
#[cfg(target_os = "windows")]
use super::windows;
#[cfg(target_os = "macos")]
use super::{
    ensure_action_not_cancelled, execute_jxa, ACTIVATE_APPLICATION_JXA, FRONTMOST_APPLICATION_JXA,
    LOOKUP_APPLICATION_JXA, RESTORE_APPLICATION_JXA,
};

#[cfg(target_os = "macos")]
#[derive(Debug, Clone)]
struct ApplicationIdentity {
    pid: u32,
    application: String,
}

#[cfg(target_os = "macos")]
#[derive(Debug, Clone)]
pub(super) struct ApplicationActivationRollbackGuard {
    previous: ApplicationIdentity,
    target: ApplicationIdentity,
    changed_frontmost_application: bool,
}

#[cfg(target_os = "windows")]
pub(super) type ApplicationActivationRollbackGuard = windows::ApplicationActivationRollbackGuard;

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
#[derive(Debug, Clone)]
pub(super) struct ApplicationActivationRollbackGuard;

pub(super) fn parse_application_pid(arguments: &Value) -> Result<u32> {
    reject_unknown_fields(arguments, &["pid"])?;
    let pid = arguments
        .get("pid")
        .and_then(Value::as_u64)
        .ok_or_else(|| anyhow!("pid must be a positive integer"))?;
    if pid == 0 || pid > i32::MAX as u64 {
        return Err(anyhow!("pid must be between 1 and {}", i32::MAX));
    }
    Ok(pid as u32)
}

#[cfg(target_os = "macos")]
pub(super) fn lookup_application(pid: u32) -> Result<Value> {
    execute_jxa(LOOKUP_APPLICATION_JXA, &[pid.to_string()])
}

#[cfg(target_os = "windows")]
pub(super) fn lookup_application(pid: u32) -> Result<Value> {
    windows::lookup_application(pid)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(super) fn lookup_application(_pid: u32) -> Result<Value> {
    Err(anyhow!(
        "Computer Use application discovery is unsupported on this platform"
    ))
}

pub(super) fn approved_application_name(
    approved_command_args: Option<&[String]>,
) -> Result<String> {
    let arguments = approved_command_args.ok_or_else(|| {
        anyhow!("Computer Use application activation is missing approved identity context")
    })?;
    let encoded = arguments
        .iter()
        .find_map(|argument| argument.strip_prefix("--application-json="))
        .ok_or_else(|| anyhow!("approved application identity is missing"))?;
    let application =
        serde_json::from_str::<String>(encoded).context("decode approved application identity")?;
    if application.is_empty() || application.chars().count() > 120 {
        return Err(anyhow!("approved application identity is invalid"));
    }
    Ok(application)
}

#[cfg(target_os = "macos")]
pub(super) fn activate_application_with_rollback(
    pid: u32,
    approved_application: String,
    action_cancelled: Option<&AtomicBool>,
) -> Result<(Value, ApplicationActivationRollbackGuard)> {
    let previous = frontmost_application_identity()?;
    ensure_action_not_cancelled(action_cancelled)?;
    let mut result = execute_jxa(
        ACTIVATE_APPLICATION_JXA,
        &[
            pid.to_string(),
            approved_application.clone(),
            previous.pid.to_string(),
            previous.application.clone(),
        ],
    )?;
    let map = result
        .as_object_mut()
        .ok_or_else(|| anyhow!("Computer Use activation result must be an object"))?;
    map.insert(
        "mode".to_string(),
        Value::String("approved_input".to_string()),
    );
    map.insert(
        "action".to_string(),
        Value::String("activate_application".to_string()),
    );
    map.remove("sensitive_text_policy");
    Ok((
        result,
        ApplicationActivationRollbackGuard {
            changed_frontmost_application: previous.pid != pid,
            previous,
            target: ApplicationIdentity {
                pid,
                application: approved_application,
            },
        },
    ))
}

#[cfg(target_os = "windows")]
pub(super) fn activate_application_with_rollback(
    pid: u32,
    approved_application: String,
    action_cancelled: Option<&AtomicBool>,
) -> Result<(Value, ApplicationActivationRollbackGuard)> {
    windows::activate_application_with_rollback(pid, approved_application, action_cancelled)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(super) fn activate_application_with_rollback(
    _pid: u32,
    _approved_application: String,
    _action_cancelled: Option<&AtomicBool>,
) -> Result<(Value, ApplicationActivationRollbackGuard)> {
    Err(anyhow!(
        "Computer Use application activation is unsupported on this platform"
    ))
}

#[cfg(target_os = "macos")]
fn frontmost_application_identity() -> Result<ApplicationIdentity> {
    let result = execute_jxa(FRONTMOST_APPLICATION_JXA, &[])?;
    let pid = result
        .get("pid")
        .and_then(Value::as_u64)
        .and_then(|pid| u32::try_from(pid).ok())
        .filter(|pid| *pid > 0)
        .ok_or_else(|| anyhow!("macOS frontmost application PID is invalid"))?;
    let application = result
        .get("application")
        .and_then(Value::as_str)
        .filter(|application| !application.is_empty() && application.chars().count() <= 240)
        .ok_or_else(|| anyhow!("macOS frontmost application identity is invalid"))?
        .to_string();
    Ok(ApplicationIdentity { pid, application })
}

#[cfg(target_os = "macos")]
pub(super) fn rollback_application_activation(
    guard: &ApplicationActivationRollbackGuard,
) -> Result<Value> {
    if !guard.changed_frontmost_application {
        return Ok(json!({
            "scope": "frontmost_application_activation_only",
            "rollback_on_in_flight_cancel": true,
            "attempted": false,
            "restored": true,
            "reason": "activation_did_not_change_frontmost_application",
            "previous_pid": guard.previous.pid,
            "target_pid": guard.target.pid,
            "application_content_rollback": false,
            "window_geometry_rollback": false,
        }));
    }
    let result = execute_jxa(
        RESTORE_APPLICATION_JXA,
        &[
            guard.previous.pid.to_string(),
            guard.previous.application.clone(),
            guard.target.pid.to_string(),
            guard.target.application.clone(),
        ],
    )?;
    normalize_application_rollback_result(result, guard.previous.pid, guard.target.pid)
}

#[cfg(target_os = "windows")]
pub(super) fn rollback_application_activation(
    guard: &ApplicationActivationRollbackGuard,
) -> Result<Value> {
    windows::rollback_application_activation(guard)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(super) fn rollback_application_activation(
    _guard: &ApplicationActivationRollbackGuard,
) -> Result<Value> {
    Err(anyhow!(
        "Computer Use application activation rollback is unsupported on this platform"
    ))
}

#[cfg(target_os = "macos")]
fn normalize_application_rollback_result(
    result: Value,
    previous_pid: u32,
    target_pid: u32,
) -> Result<Value> {
    let attempted = result
        .get("attempted")
        .and_then(Value::as_bool)
        .ok_or_else(|| anyhow!("application activation rollback result is missing attempted"))?;
    let restored = result
        .get("restored")
        .and_then(Value::as_bool)
        .ok_or_else(|| anyhow!("application activation rollback result is missing restored"))?;
    let reason = result
        .get("reason")
        .and_then(Value::as_str)
        .filter(|reason| {
            matches!(
                *reason,
                "activation_did_not_change_frontmost_application"
                    | "foreground_changed_after_activation"
                    | "previous_application_identity_unavailable"
                    | "platform_refused_restore"
                    | "cancelled_activation_restored"
            )
        })
        .ok_or_else(|| anyhow!("application activation rollback result has an invalid reason"))?;
    Ok(json!({
        "scope": "frontmost_application_activation_only",
        "rollback_on_in_flight_cancel": true,
        "attempted": attempted,
        "restored": restored,
        "reason": reason,
        "previous_pid": previous_pid,
        "target_pid": target_pid,
        "application_content_rollback": false,
        "window_geometry_rollback": false,
    }))
}
