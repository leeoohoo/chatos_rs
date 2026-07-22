// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SystemMcpBackend {
    Embedded,
    ServiceHttp,
    ServiceDynamic,
    HostAdapter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SystemMcpHost {
    Chatos,
    TaskRunner,
    LocalConnector,
    ProjectManagementService,
    SandboxManagerService,
}
