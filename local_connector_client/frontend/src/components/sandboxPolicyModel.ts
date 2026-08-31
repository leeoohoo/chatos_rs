// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type {
  PermissionProfileId,
  SandboxApprovalPolicy,
  SandboxApprovalReviewer,
  SandboxBackendKind,
  SandboxCapabilities,
  SandboxNetworkAccess,
  SandboxNetworkRequirements,
  SandboxSettings,
  SandboxSettingsUpdate,
} from '../api';

export type SandboxApprovalMode = 'user' | 'auto_review' | 'never';

export function resolveSandboxPolicyView(
  settings: SandboxSettings | null,
  capabilities: SandboxCapabilities | null,
) {
  const backend = normalizeSandboxBackend(
    settings?.default_backend,
  );
  const permissionProfile = normalizePermissionProfile(
    settings?.default_permission_profile_id,
  );
  const permissionProfileName =
    settings?.default_permission_profile_name
    || permissionProfileCodexName(permissionProfile);
  const customPermissionProfileActive = !permissionProfileName.startsWith(':');
  const approvalPolicy = normalizeApprovalPolicy(
    settings?.default_approval_policy,
  );
  const approvalReviewer = normalizeApprovalReviewer(
    settings?.default_approval_reviewer,
  );
  const approvalMode = approvalModeFromPolicy(approvalPolicy, approvalReviewer);
  const network = resolveEffectiveNetwork(settings);
  const profileCatalog = settings?.permission_profiles || [];
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
    networkPresentation: describeNetworkAccess(network, approvalMode),
    permissionProfile,
    permissionProfileName,
    recommended:
      !customPermissionProfileActive
      && backend === 'local_process'
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
    default_network_access: 'disabled',
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
    return '可以访问项目以外的本机文件；网络访问由互联网访问设置单独控制。';
  }
  return '只允许读取和修改你已经授权的项目目录。';
}

export function approvalModeDescription(mode: SandboxApprovalMode) {
  if (mode === 'auto_review') {
    return '由命令审批模型审核；AI 可以批准、拒绝或转交给你。';
  }
  if (mode === 'never') {
    return '超出当前范围的文件或网络访问会直接失败。';
  }
  return '访问项目外文件或互联网前会先征求你的同意。';
}

export function sandboxBackendLabel(_backend: SandboxBackendKind) {
  return '本机进程权限';
}

export function sandboxBackendDescription(_backend: SandboxBackendKind) {
  return '任务在授权工作区内通过本机进程运行；文件、网络与审批由 Local Connector 权限策略控制。';
}

function resolveEffectiveNetwork(settings: SandboxSettings | null): {
  access: SandboxNetworkAccess;
  unrestricted: boolean;
  requirements: SandboxNetworkRequirements;
} {
  const fallbackRequirements =
    settings?.default_network_requirements
    || { enabled: false };
  const configuredAccess = normalizeNetworkAccess(
    settings?.default_network_access,
    fallbackRequirements,
  );
  const effective = settings?.effective_permissions;
  if (effective?.network.type === 'unrestricted') {
    return { access: 'host', unrestricted: true, requirements: {} };
  }
  if (effective?.network.type === 'restricted') {
    return {
      access: configuredAccess,
      unrestricted: false,
      requirements: effective.network.requirements,
    };
  }
  return {
    access: configuredAccess,
    unrestricted: false,
    requirements: fallbackRequirements,
  };
}

function describeNetworkAccess(network: {
  access: SandboxNetworkAccess;
  unrestricted: boolean;
  requirements: SandboxNetworkRequirements;
}, approvalMode: SandboxApprovalMode) {
  if (network.unrestricted) {
    return {
      access: 'host' as const,
      enabled: true,
      label: '宿主机网络',
      unrestricted: true,
      detail: '任务进程使用宿主机网络，不走逐次联网审批。',
    };
  }
  if (network.requirements.enabled === true) {
    return {
      access: network.access,
      enabled: true,
      label: '按本机策略限制',
      unrestricted: false,
      detail: hasNetworkDomainRules(network.requirements)
        ? '任务只能主动访问客户端策略预设的网站。'
        : '任务默认可通过本机受控网络代理访问互联网。',
    };
  }
  if (approvalMode === 'auto_review') {
    return {
      access: network.access,
      enabled: false,
      label: '默认关闭，由 AI 审批',
      unrestricted: false,
      detail: '任务确需联网时，由命令审批模型决定批准、拒绝或转交给你。',
    };
  }
  if (approvalMode === 'never') {
    return {
      access: network.access,
      enabled: false,
      label: '默认关闭，直接拒绝',
      unrestricted: false,
      detail: '任务发起的临时联网请求会直接失败。',
    };
  }
  return {
    access: network.access,
    enabled: false,
    label: '默认关闭，需要时询问',
    unrestricted: false,
    detail: '任务默认断网；确需联网时会弹出授权请求。',
  };
}

function normalizeNetworkAccess(
  value: string | null | undefined,
  requirements: SandboxNetworkRequirements,
): SandboxNetworkAccess {
  if (value === 'host' || value === 'controlled' || value === 'disabled') {
    return value;
  }
  return requirements.enabled === true ? 'controlled' : 'disabled';
}

function hasNetworkDomainRules(requirements: SandboxNetworkRequirements) {
  return Object.keys(requirements.domains || {}).length > 0
    || (requirements.allowedDomains?.length || 0) > 0
    || (requirements.deniedDomains?.length || 0) > 0;
}

function normalizeSandboxBackend(_value?: string | null): SandboxBackendKind {
  return 'local_process';
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
