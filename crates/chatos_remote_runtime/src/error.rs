// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteRuntimeErrorKind {
    InvalidPort,
    AddressResolution,
    NoResolvedAddress,
    Connect,
    ReadTimeoutConfiguration,
    WriteTimeoutConfiguration,
    SessionCreation,
    Handshake,
    HostKey,
    Authentication,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteRuntimeError {
    kind: RemoteRuntimeErrorKind,
    detail: String,
}

impl RemoteRuntimeError {
    pub fn new(kind: RemoteRuntimeErrorKind, detail: impl Into<String>) -> Self {
        Self {
            kind,
            detail: detail.into(),
        }
    }

    pub const fn kind(&self) -> RemoteRuntimeErrorKind {
        self.kind
    }

    pub fn detail(&self) -> &str {
        self.detail.as_str()
    }

    pub fn format_tcp_context(&self, address_label: &str, timeout_label: &str) -> String {
        match self.kind {
            RemoteRuntimeErrorKind::InvalidPort => self.detail.clone(),
            RemoteRuntimeErrorKind::AddressResolution => {
                format!("解析{address_label}地址失败: {}", self.detail)
            }
            RemoteRuntimeErrorKind::NoResolvedAddress => {
                format!("无法解析{address_label}地址: {}", self.detail)
            }
            RemoteRuntimeErrorKind::Connect => {
                format!("连接{address_label}失败: {}", self.detail)
            }
            RemoteRuntimeErrorKind::ReadTimeoutConfiguration => {
                format!("设置{timeout_label}读超时失败: {}", self.detail)
            }
            RemoteRuntimeErrorKind::WriteTimeoutConfiguration => {
                format!("设置{timeout_label}写超时失败: {}", self.detail)
            }
            _ => self.to_string(),
        }
    }
}

impl fmt::Display for RemoteRuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            RemoteRuntimeErrorKind::InvalidPort => formatter.write_str(self.detail.as_str()),
            RemoteRuntimeErrorKind::AddressResolution => {
                write!(formatter, "解析远端地址失败: {}", self.detail)
            }
            RemoteRuntimeErrorKind::NoResolvedAddress => {
                write!(formatter, "无法解析远端地址: {}", self.detail)
            }
            RemoteRuntimeErrorKind::Connect => {
                write!(formatter, "连接远端失败: {}", self.detail)
            }
            RemoteRuntimeErrorKind::ReadTimeoutConfiguration => {
                write!(formatter, "设置 SSH 读超时失败: {}", self.detail)
            }
            RemoteRuntimeErrorKind::WriteTimeoutConfiguration => {
                write!(formatter, "设置 SSH 写超时失败: {}", self.detail)
            }
            RemoteRuntimeErrorKind::SessionCreation
            | RemoteRuntimeErrorKind::Handshake
            | RemoteRuntimeErrorKind::HostKey
            | RemoteRuntimeErrorKind::Authentication => formatter.write_str(self.detail.as_str()),
        }
    }
}

impl std::error::Error for RemoteRuntimeError {}
