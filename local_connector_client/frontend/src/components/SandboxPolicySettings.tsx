// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { Cloud, CloudOff, Shield, Sparkles } from 'lucide-react';

import type {
  ConnectorStatus,
  PermissionProfileId,
  SandboxCapabilities,
  SandboxNetworkAccess,
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

  const setDefaultNetworkAccess = async (access: SandboxNetworkAccess) => {
    if (access === view.networkPresentation.access) {
      return;
    }
    if (access === 'controlled' && !window.confirm(
      '开启后，任务默认可通过本机受控网络代理访问互联网，不再每次弹出联网审批。确定开启吗？',
    )) {
      return;
    }
    if (access === 'host' && !window.confirm(
      '开启后，任务进程会使用宿主机网络。这个设置只影响网络，不改变文件访问范围。确定开启吗？',
    )) {
      return;
    }
    await onSave(
      networkAccessPatch(access),
      '互联网访问',
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
          <span>默认只读写授权项目；联网或访问项目外文件时，关闭 AI 审批会先询问你。</span>
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
        <div className="sandboxSimpleSetting">
          <span className="settingLabel">任务运行方式</span>
          <strong>本机进程隔离</strong>
          <small>{sandboxBackendDescription(view.backend)}</small>
        </div>

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
          <strong>
            {view.networkPresentation.enabled ? <Cloud size={15} /> : <CloudOff size={15} />}
            {view.networkPresentation.label}
          </strong>
          <small>{view.networkPresentation.detail}</small>
          <div className="sandboxNetworkApprovalRow">
            <div>
              <strong><Cloud size={14} />联网模式</strong>
              <small>
                文件访问范围不影响网络；宿主机网络会直接使用本机网络栈。
              </small>
            </div>
            <select
              value={view.networkPresentation.access}
              disabled={saving || view.customPermissionProfileActive}
              title="选择任务进程的默认网络模式"
              onChange={(event) => void setDefaultNetworkAccess(
                event.target.value as SandboxNetworkAccess,
              )}
            >
              {view.customPermissionProfileActive ? (
                <option value={view.networkPresentation.access}>由本机策略管理</option>
              ) : (
                <>
                  <option value="disabled">默认关闭</option>
                  <option value="controlled">受控代理</option>
                  <option value="host">宿主机网络</option>
                </>
              )}
            </select>
          </div>
          <div className="sandboxNetworkApprovalRow">
            <div>
              <strong><Sparkles size={14} />AI 自动审批</strong>
              <small>
                开启后由命令审批模型审核联网请求；同时适用于项目外文件临时访问。
              </small>
            </div>
            <label className="switch" title="让 AI 审批联网和项目外文件请求">
              <input
                type="checkbox"
                checked={view.approvalMode === 'auto_review'}
                disabled={saving}
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

function networkAccessPatch(access: SandboxNetworkAccess): SandboxSettingsUpdate {
  if (access === 'controlled') {
    return {
      default_network_access: 'controlled',
      default_network_requirements: { enabled: true, mode: 'full' },
      risk_acknowledged: true,
    };
  }
  if (access === 'host') {
    return {
      default_network_access: 'host',
      risk_acknowledged: true,
    };
  }
  return {
    default_network_access: 'disabled',
    default_network_requirements: { enabled: false },
    risk_acknowledged: false,
  };
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
