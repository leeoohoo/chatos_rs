// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod client;
mod snapshot;

pub use client::ConfigClient;
pub use snapshot::{
    ConfigSnapshot, ConfigWatchState, PlatformPressureLevel, ServicePressureSignal,
};

pub const CONFIG_CENTER_AUDIENCE: &str = "configuration-center";
pub const CONFIG_SNAPSHOT_READ_SCOPE: &str = "config.snapshot.read";
pub const CONFIG_INSTANCE_HEARTBEAT_SCOPE: &str = "config.instance.heartbeat";
pub const CONFIG_CENTER_CALLER_HEADER: &str = "x-config-center-caller";
pub const CONFIG_CENTER_TOKEN_HEADER: &str = "x-config-center-internal-token";
