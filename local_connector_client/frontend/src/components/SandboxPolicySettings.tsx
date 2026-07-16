// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { CloudOff, Shield } from 'lucide-react';

import type {
  ConnectorStatus,
  PermissionProfileId,
  SandboxCapabilities,
  SandboxSettings,
  SandboxSettingsUpdate,
} from '../api';
import { SandboxTechnicalDetails } from './SandboxTechnicalDetails';
import {
  approvalModeDescription,
  normalizePermissionProfileName,
  permissionProfileDescription,
  recommendedSandboxSettings,
  resolveSandboxPolicyView,
  type SandboxApprovalMode,
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

  const setApprovalMode = async (mode: SandboxApprovalMode) => {
    if (mode === view.approvalMode) {
      return;
    }
    if (mode === 'auto_review' && !window.confirm(
      '自动判断会让客户端根据风险规则决定是否执行。确定继续吗？',
    )) {
      return;
    }
    await onSave(
      mode === 'never'
        ? {
            default_approval_policy: 'never',
            default_approval_reviewer: view.approvalReviewer,
            risk_acknowledged: false,
          }
        : {
            default_approval_policy: 'on_request',
            default_approval_reviewer: mode,
            risk_acknowledged: mode === 'auto_review',
          },
      '额外权限处理方式',
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
          <strong>默认保护已经够用</strong>
          <span>任务只能读写授权项目；访问其他文件或互联网时，客户端会先询问你。</span>
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

        <label className="sandboxSimpleSetting">
          <span className="settingLabel">需要额外权限时</span>
          <select
            value={view.approvalMode}
            disabled={saving}
            onChange={(event) => void setApprovalMode(event.target.value as SandboxApprovalMode)}
          >
            <option value="user">询问我（推荐）</option>
            <option value="auto_review">由客户端自动判断</option>
            <option value="never">直接拒绝</option>
          </select>
          <small>{approvalModeDescription(view.approvalMode)}</small>
        </label>

        <div className="sandboxSimpleSetting networkSummarySetting">
          <span className="settingLabel">互联网访问</span>
          <strong><CloudOff size={15} />{view.networkPresentation.label}</strong>
          <small>{view.networkPresentation.detail}</small>
        </div>
      </div>

      <div className="sandboxInboundNotice">
        这里控制的是任务主动访问外部网络，不会让外部通过域名访问你的电脑。
      </div>

      <SandboxTechnicalDetails status={status} settings={settings} backend={view.backend} />
    </>
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
