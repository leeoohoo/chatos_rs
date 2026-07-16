// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type {
  ConnectorStatus,
  PermissionProfileId,
  SandboxApprovalPolicy,
  SandboxApprovalReviewer,
  SandboxBackendKind,
  SandboxCapabilities,
  SandboxNetworkRequirements,
  SandboxSettings,
  SandboxSettingsUpdate,
} from '../api';

export type SandboxApprovalMode = 'user' | 'auto_review' | 'never';

export function resolveSandboxPolicyView(
  status: ConnectorStatus,
  settings: SandboxSettings | null,
  capabilities: SandboxCapabilities | null,
) {
  const backend = normalizeSandboxBackend(
    settings?.default_backend || status.sandbox.default_backend || status.sandbox.backend,
  );
  const permissionProfile = normalizePermissionProfile(
    settings?.default_permission_profile_id || status.sandbox.default_permission_profile_id,
  );
  const permissionProfileName =
    settings?.default_permission_profile_name
    || status.sandbox.default_permission_profile_name
    || permissionProfileCodexName(permissionProfile);
  const customPermissionProfileActive = !permissionProfileName.startsWith(':');
  const approvalPolicy = normalizeApprovalPolicy(
    settings?.default_approval_policy || status.sandbox.default_approval_policy,
  );
  const approvalReviewer = normalizeApprovalReviewer(
    settings?.default_approval_reviewer || status.sandbox.default_approval_reviewer,
  );
  const approvalMode = approvalModeFromPolicy(approvalPolicy, approvalReviewer);
  const network = resolveEffectiveNetwork(status, settings);
  const profileCatalog = settings?.permission_profiles || status.sandbox.permission_profiles || [];
  const builtinProfiles = new Map(
    profileCatalog
      .filter((profile) => profile.id.startsWith(':'))
      .map((profile) => [profile.id, profile]),
  );
  const localProcessSelectable = capabilities?.backends.some(
    (capability) => capability.backend === 'local_process' && capability.selectable,
  ) === true;

  return {
    approvalMode,
    approvalReviewer,
    backend,
    builtinProfiles,
    customPermissionProfileActive,
    localProcessSelectable,
    networkPresentation: describeNetworkAccess(network),
    permissionProfile,
    permissionProfileName,
    recommended:
      !customPermissionProfileActive
      && permissionProfile === 'workspace_write'
      && approvalMode === 'user'
      && network.unrestricted !== true
      && network.requirements.enabled !== true,
  };
}

export function recommendedSandboxSettings(
  localProcessSelectable: boolean,
): SandboxSettingsUpdate {
  return {
    ...(localProcessSelectable ? { default_backend: 'local_process' as const } : {}),
    default_permission_profile_id: 'workspace_write',
    default_approval_policy: 'on_request',
    default_approval_reviewer: 'user',
    default_network_requirements: { enabled: false },
    risk_acknowledged: false,
  };
}

export function normalizePermissionProfileName(value: string): PermissionProfileId {
  if (value === ':read-only') {
    return 'read_only';
  }
  if (value === ':danger-full-access') {
    return 'full_access';
  }
  return 'workspace_write';
}

export function permissionProfileDescription(profile: PermissionProfileId) {
  if (profile === 'read_only') {
    return '可以读取授权项目，但不能修改文件。';
  }
  if (profile === 'full_access') {
    return '可以访问项目以外的本机文件，请谨慎使用。';
  }
  return '只允许读取和修改你已经授权的项目目录。';
}

export function approvalModeDescription(mode: SandboxApprovalMode) {
  if (mode === 'auto_review') {
    return '客户端根据命令风险自动决定，必要时仍会拒绝。';
  }
  if (mode === 'never') {
    return '超出当前范围的文件或网络访问会直接失败。';
  }
  return '访问项目外文件或互联网前会先征求你的同意。';
}

export function sandboxBackendLabel(backend: SandboxBackendKind) {
  return backend === 'local_process' ? '本机进程隔离' : 'Docker 容器';
}

function resolveEffectiveNetwork(
  status: ConnectorStatus,
  settings: SandboxSettings | null,
): { unrestricted: boolean; requirements: SandboxNetworkRequirements } {
  const effective = settings?.effective_permissions || status.sandbox.effective_permissions;
  if (effective?.network.type === 'unrestricted') {
    return { unrestricted: true, requirements: {} };
  }
  if (effective?.network.type === 'restricted') {
    return { unrestricted: false, requirements: effective.network.requirements };
  }
  return {
    unrestricted: false,
    requirements:
      settings?.default_network_requirements
      || status.sandbox.default_network_requirements
      || { enabled: false },
  };
}

function describeNetworkAccess(network: {
  unrestricted: boolean;
  requirements: SandboxNetworkRequirements;
}) {
  if (network.unrestricted) {
    return {
      label: '不受限制',
      detail: '当前“整台电脑”模式允许任务主动访问互联网。',
    };
  }
  if (network.requirements.enabled === true) {
    return {
      label: '按本机策略限制',
      detail: '任务只能主动访问客户端策略预设的网站。',
    };
  }
  return {
    label: '默认关闭，需要时询问',
    detail: '任务默认断网；确需联网时会弹出授权请求。',
  };
}

function normalizeSandboxBackend(value?: string | null): SandboxBackendKind {
  return value === 'local_process' ? 'local_process' : 'docker';
}

function normalizePermissionProfile(value?: string | null): PermissionProfileId {
  if (value === 'read_only' || value === 'full_access') {
    return value;
  }
  return 'workspace_write';
}

function permissionProfileCodexName(profile: PermissionProfileId): string {
  if (profile === 'read_only') {
    return ':read-only';
  }
  if (profile === 'full_access') {
    return ':danger-full-access';
  }
  return ':workspace';
}

function normalizeApprovalPolicy(value?: string | null): SandboxApprovalPolicy {
  return value === 'never' ? 'never' : 'on_request';
}

function normalizeApprovalReviewer(value?: string | null): SandboxApprovalReviewer {
  return value === 'auto_review' ? 'auto_review' : 'user';
}

function approvalModeFromPolicy(
  policy: SandboxApprovalPolicy,
  reviewer: SandboxApprovalReviewer,
): SandboxApprovalMode {
  if (policy === 'never') {
    return 'never';
  }
  return reviewer === 'auto_review' ? 'auto_review' : 'user';
}
