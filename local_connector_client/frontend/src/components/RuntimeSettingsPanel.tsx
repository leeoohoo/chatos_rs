// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';
import {
  Accessibility,
  AppWindow,
  ExternalLink,
  FolderOpen,
  Globe2,
  MonitorUp,
  Network,
  RefreshCw,
  Settings2,
  ShieldCheck,
  Terminal,
} from 'lucide-react';

import {
  api,
  type LocalRuntimeSettings,
  type SystemPermissionItem,
  type SystemPermissionsResponse,
} from '../api';
import { loadSystemPermissions, systemPermissionReady } from '../systemPermissions';

const DEFAULT_AI_AGENT_MAX_ITERATIONS = 600;
const DEFAULT_DEVELOPER_CLOUD_BASE_URL = 'http://127.0.0.1:39230';
const DEFAULT_DEVELOPER_USER_SERVICE_BASE_URL = 'http://127.0.0.1:39190';
const DEFAULT_DEVELOPER_CHATOS_WEB_URL = 'http://127.0.0.1:8088';
type PermissionIcon = typeof Settings2;

export function RuntimeSettingsPanel({ developerOnly = false }: { developerOnly?: boolean }) {
  const [settings, setSettings] = React.useState<LocalRuntimeSettings>({
    ai_agent_max_iterations: DEFAULT_AI_AGENT_MAX_ITERATIONS,
    developer_mode: false,
    developer_cloud_base_url: DEFAULT_DEVELOPER_CLOUD_BASE_URL,
    developer_user_service_base_url: DEFAULT_DEVELOPER_USER_SERVICE_BASE_URL,
    developer_chatos_web_url: DEFAULT_DEVELOPER_CHATOS_WEB_URL,
  });
  const [permissions, setPermissions] = React.useState<SystemPermissionsResponse | null>(null);
  const [loading, setLoading] = React.useState(true);
  const [saving, setSaving] = React.useState(false);
  const [requestingPermissionId, setRequestingPermissionId] = React.useState<string | null>(null);
  const [message, setMessage] = React.useState<string | null>(null);
  const [error, setError] = React.useState<string | null>(null);

  const load = React.useCallback(async () => {
    setError(null);
    try {
      const next = await api.runtimeSettings();
      setSettings({
        ai_agent_max_iterations: next.ai_agent_max_iterations || DEFAULT_AI_AGENT_MAX_ITERATIONS,
        developer_mode: Boolean(next.developer_mode),
        developer_cloud_base_url: next.developer_cloud_base_url || DEFAULT_DEVELOPER_CLOUD_BASE_URL,
        developer_user_service_base_url:
          next.developer_user_service_base_url || DEFAULT_DEVELOPER_USER_SERVICE_BASE_URL,
        developer_chatos_web_url: next.developer_chatos_web_url || DEFAULT_DEVELOPER_CHATOS_WEB_URL,
      });
      if (!developerOnly) {
        setPermissions(await loadSystemPermissions());
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : '读取运行配置失败');
    } finally {
      setLoading(false);
    }
  }, [developerOnly]);

  React.useEffect(() => {
    void load();
  }, [load]);

  const save = async () => {
    setSaving(true);
    setMessage(null);
    setError(null);
    try {
      const next = await api.updateRuntimeSettings({
        ai_agent_max_iterations: Math.max(
          1,
          Number(settings.ai_agent_max_iterations) || DEFAULT_AI_AGENT_MAX_ITERATIONS,
        ),
        developer_mode: settings.developer_mode,
      });
      setSettings(next);
      await window.chatosLocalConnector?.setDeveloperMode?.(next.developer_mode);
      setMessage(
        next.developer_mode
          ? '开发者模式已开启，主页面已切换到本地服务；请在本地 Chat OS 登录完成配对。'
          : '开发者模式已关闭，主页面已切回线上服务；请在线上 Chat OS 登录完成配对。',
      );
    } catch (err) {
      setError(err instanceof Error ? err.message : '保存运行配置失败');
    } finally {
      setSaving(false);
    }
  };

  const requestPermission = async (permission: SystemPermissionItem) => {
    if (!permission.can_request) {
      return;
    }
    setRequestingPermissionId(permission.id);
    setMessage(null);
    setError(null);
    try {
      await window.chatosLocalConnector?.requestDesktopSystemPermission?.(permission.id);
      await api.requestSystemPermission(permission.id);
      setPermissions(await loadSystemPermissions());
      setMessage('已打开系统设置。完成授权后请刷新状态。');
    } catch (err) {
      setError(err instanceof Error ? err.message : '打开系统权限设置失败');
    } finally {
      setRequestingPermissionId(null);
    }
  };

  if (loading) {
    return <section className="panel"><div className="emptyState">正在读取运行配置...</div></section>;
  }

  return (
    <section className="settingsPage">
      <section className="panel">
        <div className="panelHeader">
          <div>
            <h2><Settings2 size={18} />运行配置</h2>
            <p>本机 Agent 运行参数</p>
          </div>
          <button className="iconButton" onClick={() => void load()} title="刷新配置">
            <RefreshCw size={17} />
          </button>
        </div>
        {message ? <div className="banner">{message}</div> : null}
        {error ? <div className="formError">{error}</div> : null}
        {!developerOnly ? (
          <div className="settingsFormGrid">
            <label>
              Agent 最大迭代次数
              <input
                type="number"
                min="1"
                step="1"
                value={settings.ai_agent_max_iterations}
                onChange={(event) =>
                  setSettings({
                    ...settings,
                    ai_agent_max_iterations: Number(event.target.value) || DEFAULT_AI_AGENT_MAX_ITERATIONS,
                  })
                }
              />
            </label>
          </div>
        ) : null}
        <div className={`developerModeCard ${settings.developer_mode ? 'active' : ''}`}>
          <div className="developerModeHeading">
            <div className="permissionIcon"><Globe2 size={18} /></div>
            <div>
              <strong>开发者模式</strong>
              <span>主页面与 Local Connector Core 切换到本机服务；附件对象存储继续使用后端配置的线上 MinIO。</span>
            </div>
            <label className="switch" title="切换开发者模式">
              <input
                type="checkbox"
                checked={settings.developer_mode}
                onChange={(event) =>
                  setSettings({ ...settings, developer_mode: event.target.checked })
                }
              />
              <span />
            </label>
          </div>
          <div className="developerEndpointGrid">
            <div><span>Chat OS</span><code>{settings.developer_chatos_web_url}</code></div>
            <div><span>Connector Service</span><code>{settings.developer_cloud_base_url}</code></div>
            <div><span>User Service</span><code>{settings.developer_user_service_base_url}</code></div>
            <div><span>MinIO S3 API</span><code>https://oss.jgoool.com</code></div>
          </div>
          <small>切换时会主动断开当前环境的 Connector 长连接，防止本地页面与线上 Relay 混用；目标页面登录后会自动重新配对。</small>
        </div>
        <button className="primaryButton compact" disabled={saving} onClick={() => void save()}>
          {saving ? '保存中' : '保存配置'}
        </button>
      </section>
      {!developerOnly ? (
        <SystemPermissionsPanel
          permissions={permissions}
          requestingPermissionId={requestingPermissionId}
          onRefresh={load}
          onRequest={requestPermission}
        />
      ) : null}
    </section>
  );
}

function SystemPermissionsPanel({
  permissions,
  requestingPermissionId,
  onRefresh,
  onRequest,
}: {
  permissions: SystemPermissionsResponse | null;
  requestingPermissionId: string | null;
  onRefresh: () => Promise<void>;
  onRequest: (permission: SystemPermissionItem) => Promise<void>;
}) {
  return (
    <section className="panel">
      <div className="panelHeader">
        <div>
          <h2><ShieldCheck size={18} />Skills 与 MCP 系统权限</h2>
          <p>
            {permissions
              ? `${permissions.platform_label} 下本机 Skills 与 MCP 能力的系统访问状态`
              : '正在读取本机系统权限状态'}
          </p>
        </div>
        <button className="iconButton" onClick={() => void onRefresh()} title="刷新权限状态">
          <RefreshCw size={17} />
        </button>
      </div>
      {permissions ? (
        <div className="permissionList">
          {permissions.items.map((permission) => (
            <PermissionRow
              key={permission.id}
              permission={permission}
              requesting={requestingPermissionId === permission.id}
              onRequest={onRequest}
            />
          ))}
        </div>
      ) : (
        <div className="emptyState">暂时无法读取系统权限状态。</div>
      )}
    </section>
  );
}

function PermissionRow({
  permission,
  requesting,
  onRequest,
}: {
  permission: SystemPermissionItem;
  requesting: boolean;
  onRequest: (permission: SystemPermissionItem) => Promise<void>;
}) {
  const Icon = permissionIcon(permission.id);
  const ready = permissionReady(permission);
  const disabled = requesting || ready || !permission.can_request;
  return (
    <div className="permissionRow">
      <div className="permissionIcon"><Icon size={18} /></div>
      <div className="permissionBody">
        <div className="permissionTitleLine">
          <strong>{permission.label}</strong>
          <span className={`status ${statusTone(permission.status)}`}>{permission.status_label}</span>
        </div>
        <span>{permission.summary}</span>
        <small>{permission.note}</small>
        {permission.last_error ? <em>{permission.last_error}</em> : null}
        <div className="permissionKinds">
          {permission.builtin_kinds.map((kind) => <code key={kind}>{kind}</code>)}
          {permission.skill_ids.map((skillId) => <code key={skillId}>{skillId}</code>)}
        </div>
      </div>
      <div className="permissionAction">
        <label
          className="switch"
          title={permission.can_request ? permission.request_label : permission.status_label}
        >
          <input
            type="checkbox"
            checked={ready}
            disabled={disabled}
            onChange={(event) => {
              if (event.target.checked) {
                void onRequest(permission);
              }
            }}
          />
          <span />
        </label>
        {permission.can_request && !ready ? (
          <button
            type="button"
            className="ghostButton compact"
            disabled={requesting}
            onClick={() => void onRequest(permission)}
            title={permission.settings_target || permission.request_label}
          >
            <ExternalLink size={14} />
            {requesting ? '打开中' : permission.request_label}
          </button>
        ) : null}
      </div>
    </div>
  );
}

function permissionIcon(permissionId: string): PermissionIcon {
  switch (permissionId) {
    case 'workspace_files':
      return FolderOpen;
    case 'terminal_execution':
      return Terminal;
    case 'browser_automation':
      return Globe2;
    case 'network_access':
      return Network;
    case 'accessibility_control':
      return Accessibility;
    case 'screen_recording':
      return MonitorUp;
    case 'office_automation':
      return AppWindow;
    default:
      return Settings2;
  }
}

function permissionReady(permission: SystemPermissionItem): boolean {
  return systemPermissionReady(permission);
}

function statusTone(status: string): 'ok' | 'warn' | 'bad' {
  if (status === 'ready' || status === 'not_applicable' || status === 'on_demand') {
    return 'ok';
  }
  if (status === 'missing_dependency') {
    return 'bad';
  }
  return 'warn';
}
