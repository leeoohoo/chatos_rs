// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { CloudOff, Shield, Sparkles } from 'lucide-react';

import type {
  ConnectorStatus,
  PermissionProfileId,
  SandboxBackendKind,
  SandboxCapabilities,
  SandboxSettings,
  SandboxSettingsUpdate,
} from '../api';
import { SandboxTechnicalDetails } from './SandboxTechnicalDetails';
import {
  normalizePermissionProfileName,
  permissionProfileDescription,
  recommendedSandboxSettings,
  resolveSandboxPolicyView,
  sandboxBackendDescription,
} from './sandboxPolicyModel';

export function SandboxPolicySettings({
  status,
  settings,
  capabilities,
  saving,
  onSave,
}: {
  status: ConnectorStatus;
  settings: SandboxSettings | null;
  capabilities: SandboxCapabilities | null;
  saving: boolean;
  onSave: (patch: SandboxSettingsUpdate, label: string) => Promise<void>;
}) {
  const view = resolveSandboxPolicyView(status, settings, capabilities);

  const setPermissionProfile = async (profile: PermissionProfileId) => {
    if (profile === view.permissionProfile) {
      return;
    }
    if (profile === 'full_access' && !window.confirm(
      '“整台电脑”会允许本地任务访问授权项目以外的文件。确定继续吗？',
    )) {
      return;
    }
    await onSave(
      {
        default_permission_profile_id: profile,
        risk_acknowledged: profile === 'full_access',
      },
      '文件访问范围',
    );
  };

  const setBackend = async (backend: SandboxBackendKind) => {
    if (backend === view.backend) {
      return;
    }
    if (backend === 'docker' && !window.confirm(
      'Docker 容器当前使用桥接网络，不能提供按域名的出站网络审批。确定切换吗？',
    )) {
      return;
    }
    await onSave(
      { default_backend: backend },
      '任务运行方式',
    );
  };

  const setAiApproval = async (enabled: boolean) => {
    if (enabled && !window.confirm(
      '开启后，命令审批模型会审核联网和项目外文件请求。AI 可以批准、拒绝或转交给你；模型不可用时会默认拒绝。确定开启吗？',
    )) {
      return;
    }
    await onSave(
      {
        default_approval_policy: 'on_request',
        default_approval_reviewer: enabled ? 'auto_review' : 'user',
        risk_acknowledged: enabled,
      },
      'AI 自动审批',
    );
  };

  const restoreRecommendedSettings = async () => {
    await onSave(
      recommendedSandboxSettings(view.localProcessSelectable),
      '推荐保护设置',
    );
  };

  return (
    <>
      <div className="sandboxSimpleIntro">
        <Shield size={19} />
        <div>
          <strong>{view.recommended ? '推荐安全策略已启用' : '当前安全策略'}</strong>
          <span>{view.backend === 'docker'
            ? '任务仅挂载授权项目，并使用无公网出口的内部网络。'
            : '默认只读写授权项目；联网或访问项目外文件时，关闭 AI 审批会先询问你。'}</span>
        </div>
        {!view.recommended && !view.customPermissionProfileActive ? (
          <button
            type="button"
            className="ghostButton compact"
            disabled={saving}
            onClick={() => void restoreRecommendedSettings()}
          >
            恢复推荐设置
          </button>
        ) : null}
      </div>

      <div className="sandboxSimpleSettingsGrid">
        <label className="sandboxSimpleSetting">
          <span className="settingLabel">任务运行方式</span>
          <select
            value={view.backend}
            disabled={saving}
            onChange={(event) => void setBackend(event.target.value as SandboxBackendKind)}
          >
            <BackendOption
              backend="local_process"
              label="本机进程隔离（推荐）"
              available={view.backendCapabilities.get('local_process')?.selectable === true}
            />
            <BackendOption
              backend="docker"
              label="Docker 容器"
              available={view.backendCapabilities.get('docker')?.status === 'ready'}
            />
          </select>
          <small>{sandboxBackendDescription(view.backend)}</small>
        </label>

        <label className="sandboxSimpleSetting">
          <span className="settingLabel">本地文件访问</span>
          <select
            value={view.permissionProfileName}
            disabled={saving || view.customPermissionProfileActive}
            onChange={(event) => void setPermissionProfile(
              normalizePermissionProfileName(event.target.value),
            )}
          >
            {view.customPermissionProfileActive ? (
              <option value={view.permissionProfileName}>由本机策略管理</option>
            ) : (
              <>
                <PermissionOption
                  id=":read-only"
                  label="只查看文件"
                  catalog={view.builtinProfiles}
                />
                <PermissionOption
                  id=":workspace"
                  label="仅授权项目（推荐）"
                  catalog={view.builtinProfiles}
                />
                <PermissionOption
                  id=":danger-full-access"
                  label="整台电脑（高风险）"
                  catalog={view.builtinProfiles}
                />
              </>
            )}
          </select>
          <small>{view.customPermissionProfileActive
            ? '当前范围由本机权限策略统一设置。'
            : permissionProfileDescription(view.permissionProfile)}</small>
        </label>

        <div className="sandboxSimpleSetting networkSummarySetting">
          <span className="settingLabel">互联网访问</span>
          <strong><CloudOff size={15} />{view.networkPresentation.label}</strong>
          <small>{view.networkPresentation.detail}</small>
          <div className="sandboxNetworkApprovalRow">
            <div>
              <strong><Sparkles size={14} />AI 自动审批</strong>
              <small>
                {view.backend === 'local_process'
                  ? '开启后由命令审批模型审核联网请求；同时适用于项目外文件临时访问。'
                  : 'Docker 模式暂不支持临时权限覆盖，无法使用 AI 联网审批。'}
              </small>
            </div>
            <label className="switch" title="让 AI 审批联网和项目外文件请求">
              <input
                type="checkbox"
                checked={view.approvalMode === 'auto_review'}
                disabled={saving || view.backend !== 'local_process'}
                onChange={(event) => void setAiApproval(event.target.checked)}
              />
              <span />
            </label>
          </div>
        </div>
      </div>

      <div className="sandboxInboundNotice">
        这里控制的是任务主动访问外部网络，不会让外部通过域名访问你的电脑。
      </div>

      <SandboxTechnicalDetails status={status} settings={settings} backend={view.backend} />
    </>
  );
}

function BackendOption({
  backend,
  label,
  available,
}: {
  backend: SandboxBackendKind;
  label: string;
  available: boolean;
}) {
  return (
    <option value={backend} disabled={!available}>
      {label}{available ? '' : '（当前不可用）'}
    </option>
  );
}

function PermissionOption({
  id,
  label,
  catalog,
}: {
  id: string;
  label: string;
  catalog: Map<string, { allowed: boolean }>;
}) {
  const profile = catalog.get(id);
  return (
    <option value={id} disabled={profile?.allowed === false}>
      {label}{profile?.allowed === false ? '（策略禁用）' : ''}
    </option>
  );
}
