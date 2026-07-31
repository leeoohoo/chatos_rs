// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};

use super::{ApprovedDisplayGuard, ApprovedFrontmostWindowGuard, WindowBoundsRequest};

pub(super) enum PostActionObservationTarget {
    MainDisplay,
    ApprovedDisplay(ApprovedDisplayGuard),
    FrontmostWindow(FrontmostWindowObservationGuard),
}

#[derive(Debug, Clone)]
pub(super) struct FrontmostWindowObservationGuard {
    platform: String,
    application: String,
    pid: u32,
    window_id: String,
}

#[derive(Debug, Clone)]
pub(super) enum WindowControlRollbackGuard {
    Bounds {
        request: WindowBoundsRequest,
        approved: ApprovedFrontmostWindowGuard,
    },
    Fullscreen {
        fullscreen: bool,
        approved: ApprovedFrontmostWindowGuard,
    },
    Maximized {
        maximized: bool,
        approved: ApprovedFrontmostWindowGuard,
    },
}

impl PostActionObservationTarget {
    pub(super) fn requested_index(&self) -> Option<u32> {
        match self {
            Self::MainDisplay => None,
            Self::ApprovedDisplay(display) => Some(display.index),
            Self::FrontmostWindow(_) => None,
        }
    }

    pub(super) fn metadata(&self) -> Value {
        match self {
            Self::MainDisplay => json!({"scope": "main_display"}),
            Self::ApprovedDisplay(display) => json!({
                "scope": "approved_display",
                "display_index": display.index,
                "display_id": display.display_id,
            }),
            Self::FrontmostWindow(window) => json!({
                "scope": "frontmost_window",
                "platform": window.platform,
                "application": window.application,
                "pid": window.pid,
                "window_id": window.window_id,
            }),
        }
    }

    pub(super) fn matches_capture(&self, capture: &Value) -> bool {
        match self {
            Self::MainDisplay => capture
                .get("is_main")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            Self::ApprovedDisplay(display) => {
                capture.get("display_index").and_then(Value::as_u64)
                    == Some(u64::from(display.index))
                    && capture.get("display_id").and_then(Value::as_u64)
                        == Some(u64::from(display.display_id))
            }
            Self::FrontmostWindow(window) => {
                capture.get("capture_scope").and_then(Value::as_str) == Some("frontmost_window")
                    && capture.get("platform").and_then(Value::as_str)
                        == Some(window.platform.as_str())
                    && capture.get("application").and_then(Value::as_str)
                        == Some(window.application.as_str())
                    && capture.get("pid").and_then(Value::as_u64) == Some(u64::from(window.pid))
                    && capture.get("window_id").and_then(Value::as_str)
                        == Some(window.window_id.as_str())
            }
        }
    }

    pub(super) fn mismatch_reason(&self) -> &'static str {
        match self {
            Self::MainDisplay | Self::ApprovedDisplay(_) => "display_identity_changed",
            Self::FrontmostWindow(_) => "frontmost_window_identity_changed",
        }
    }
}

impl WindowControlRollbackGuard {
    pub(super) fn observation_target(&self) -> PostActionObservationTarget {
        let approved = match self {
            Self::Bounds { approved, .. }
            | Self::Fullscreen { approved, .. }
            | Self::Maximized { approved, .. } => approved,
        };
        PostActionObservationTarget::FrontmostWindow(FrontmostWindowObservationGuard {
            platform: approved.platform.clone(),
            application: approved.application.clone(),
            pid: approved.pid,
            window_id: approved.window_id.clone(),
        })
    }

    pub(super) fn matches_target_identity(&self, current: &ApprovedFrontmostWindowGuard) -> bool {
        let approved = match self {
            Self::Bounds { approved, .. }
            | Self::Fullscreen { approved, .. }
            | Self::Maximized { approved, .. } => approved,
        };
        current.platform == approved.platform
            && current.application == approved.application
            && current.pid == approved.pid
            && current.window_id == approved.window_id
    }

    pub(super) fn matches_applied_state(&self, current: &ApprovedFrontmostWindowGuard) -> bool {
        if !self.matches_target_identity(current) {
            return false;
        }
        match self {
            Self::Bounds { request, .. } => {
                current.position == [f64::from(request.x), f64::from(request.y)]
                    && current.size == [f64::from(request.width), f64::from(request.height)]
                    && current.fullscreen != Some(true)
                    && current.maximized != Some(true)
            }
            Self::Fullscreen { fullscreen, .. } => current.fullscreen == Some(*fullscreen),
            Self::Maximized { maximized, .. } => current.maximized == Some(*maximized),
        }
    }
}
