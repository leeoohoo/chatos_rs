// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

pub(super) fn local_connector_http_timeouts(
    control_plane_timeout: Duration,
    tool_timeout: Duration,
) -> chatos_service_runtime::HttpClientTimeouts {
    chatos_service_runtime::HttpClientTimeouts::new(tool_timeout.max(control_plane_timeout))
        .with_connect_timeout(control_plane_timeout)
}
