// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';
import {
  Accessibility,
  AppWindow,
  Copy,
  ExternalLink,
  FolderOpen,
  Globe2,
  MonitorUp,
  Network,
  RefreshCw,
  Settings2,
  ShieldAlert,
  ShieldCheck,
  Terminal,
} from 'lucide-react';

import {
  api,
  type LocalRuntimeSettings,
  type ChromeIntegrationStatus,
  type SystemPermissionItem,
  type SystemPermissionsResponse,
} from '../api';
import { loadSystemPermissions, systemPermissionReady } from '../systemPermissions';
import { AgentPromptUpdateSettings } from './AgentPromptUpdateSettings';

const DEFAULT_DEVELOPER_CLOUD_BASE_URL = 'http://127.0.0.1:39230';
const DEFAULT_DEVELOPER_USER_SERVICE_BASE_URL = 'http://127.0.0.1:39190';
const DEFAULT_DEVELOPER_CHATOS_WEB_URL = 'http://127.0.0.1:8088';
type PermissionIcon = typeof Settings2;

export function RuntimeSettingsPanel({ developerOnly = false }: { developerOnly?: boolean }) {
  const [settings, setSettings] = React.useState<LocalRuntimeSettings>({
    developer_mode: false,
    browser_full_cdp_access_enabled: false,
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
        developer_mode: Boolean(next.developer_mode),
        browser_full_cdp_access_enabled: Boolean(next.browser_full_cdp_access_enabled),
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
        developer_mode: settings.developer_mode,
        browser_full_cdp_access_enabled: settings.browser_full_cdp_access_enabled,
        acknowledge_browser_full_cdp_risk: settings.browser_full_cdp_access_enabled,
      });
      setSettings(next);
      await window.chatosLocalConnector?.setDeveloperMode?.(next.developer_mode);
      setMessage([
        next.developer_mode
          ? '本机服务开发者模式已开启。'
          : '本机服务开发者模式已关闭。',
        next.browser_full_cdp_access_enabled
          ? '浏览器完整 CDP 存取已开启；每条命令仍需本机逐次批准。'
          : '浏览器完整 CDP 存取已关闭。',
      ].join(' '));
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

  const setBrowserFullCdpAccessEnabled = (enabled: boolean) => {
    if (
      enabled
      && !settings.browser_full_cdp_access_enabled
      && !window.confirm(
        '完整 CDP 存取属于较高风险能力，可能读取 Cookie、储存空间、页面内容和浏览器诊断资料，也可能导航、修改或关闭页面。\n\n开启后，每一条 CDP 命令仍需在本机逐次明确批准。确认开启吗？',
      )
    ) {
      return;
    }
    setSettings((current) => ({
      ...current,
      browser_full_cdp_access_enabled: enabled,
    }));
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
            <p>本机开发模式与系统权限设置</p>
          </div>
          <button className="iconButton" onClick={() => void load()} title="刷新配置">
            <RefreshCw size={17} />
          </button>
        </div>
        {message ? <div className="banner">{message}</div> : null}
        {error ? <div className="formError">{error}</div> : null}
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
        <div className={`developerModeCard highRisk ${settings.browser_full_cdp_access_enabled ? 'active' : ''}`}>
          <div className="developerModeHeading">
            <div className="permissionIcon"><ShieldAlert size={18} /></div>
            <div>
              <strong>启用完整 CDP 存取权限</strong>
              <span className="riskLabel">较高风险</span>
              <span>
                启用完整 CDP 存取权限后，Chat OS 可在已连接的 Browser Use 会话中检查和控制敏感的浏览器内部机制。
                工具默认不发布；开启后每一条完整 CDP 命令仍需在本机逐次明确批准。
              </span>
            </div>
            <label className="switch" title="启用完整 CDP 存取权限">
              <input
                type="checkbox"
                checked={settings.browser_full_cdp_access_enabled}
                onChange={(event) => setBrowserFullCdpAccessEnabled(event.target.checked)}
              />
              <span />
            </label>
          </div>
          <small>
            完整 CDP 可能读取 Cookie、储存空间、页面内容和浏览器诊断资料，也可能导航、修改或关闭页面。
            调试端口与 WebSocket 地址不会提供给网页前端或模型。
          </small>
        </div>
        <button className="primaryButton compact" disabled={saving} onClick={() => void save()}>
          {saving ? '保存中' : '保存配置'}
        </button>
      </section>
      {!developerOnly ? (
        <>
          <AgentPromptUpdateSettings />
          <ChromeIntegrationPanel />
          <SystemPermissionsPanel
            permissions={permissions}
            requestingPermissionId={requestingPermissionId}
            onRefresh={load}
            onRequest={requestPermission}
          />
        </>
      ) : null}
    </section>
  );
}

function ChromeIntegrationPanel() {
  const [status, setStatus] = React.useState<ChromeIntegrationStatus | null>(null);
  const [loading, setLoading] = React.useState(true);
  const [saving, setSaving] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  const [setupMessage, setSetupMessage] = React.useState<string | null>(null);

  const load = React.useCallback(async () => {
    setError(null);
    try {
      setStatus(await api.chromeIntegration());
    } catch (err) {
      setError(err instanceof Error ? err.message : '读取 Chrome 整合状态失败');
    } finally {
      setLoading(false);
    }
  }, []);

  React.useEffect(() => {
    void load();
  }, [load]);

  const enable = async () => {
    if (!window.confirm(
      'Chrome 整合可以读取你明确授权并连接的已登录网页内容。扩展不会申请 Cookie、历史记录、下载或书签权限；每次从 Chat OS 读取标签页仍需本机批准。\n\n确认注册 ChatOS Native Host 吗？',
    )) {
      return;
    }
    setSaving(true);
    setError(null);
    setSetupMessage(null);
    try {
      setStatus(await api.enableChromeIntegration());
    } catch (err) {
      setError(err instanceof Error ? err.message : '启用 Chrome 整合失败');
    } finally {
      setSaving(false);
    }
  };

  const disable = async () => {
    if (!window.confirm('确认移除 ChatOS Chrome Native Host 注册吗？')) {
      return;
    }
    setSaving(true);
    setError(null);
    setSetupMessage(null);
    try {
      setStatus(await api.disableChromeIntegration());
    } catch (err) {
      setError(err instanceof Error ? err.message : '停用 Chrome 整合失败');
    } finally {
      setSaving(false);
    }
  };

  const openExtensionDirectory = async () => {
    setError(null);
    try {
      await window.chatosLocalConnector?.showChromeExtensionDirectory?.();
      setSetupMessage('已打开可加载的 ChatOS Chrome 扩展目录。Chrome 里点击“加载已解压的扩展程序”后选择这个目录即可。');
    } catch (err) {
      setError(err instanceof Error ? err.message : '打开 Chrome 扩展目录失败');
    }
  };

  const openChromeExtensionsPage = async () => {
    setError(null);
    try {
      await window.chatosLocalConnector?.openChromeExtensionsPage?.();
      setSetupMessage('已打开 Chrome 扩展管理页。请开启“开发者模式”，再点击“加载已解压的扩展程序”。');
    } catch (err) {
      setError(err instanceof Error ? err.message : '打开 Chrome 扩展页失败');
    }
  };

  const copyExtensionPath = async () => {
    setError(null);
    try {
      const path = await window.chatosLocalConnector?.copyChromeExtensionInstallPath?.();
      if (path) {
        setSetupMessage(`已复制扩展目录路径：${path}`);
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : '复制 Chrome 扩展目录失败');
    }
  };

  return (
    <section className="panel">
      <div className="panelHeader">
        <div>
          <h2><Globe2 size={18} />Chrome existing-session</h2>
          <p>连接用户现有 Chrome；站点权限和标签页连接都由扩展弹窗显式控制</p>
        </div>
        <button className="iconButton" onClick={() => void load()} title="刷新 Chrome 状态">
          <RefreshCw size={17} />
        </button>
      </div>
      {error ? <div className="formError">{error}</div> : null}
      {loading ? <div className="emptyState">正在读取 Chrome 整合状态...</div> : null}
      {status ? (
        <div className={`developerModeCard ${status.enabled ? 'active' : ''}`}>
          <div className="developerModeHeading">
            <div className="permissionIcon"><Globe2 size={18} /></div>
            <div>
              <strong>{status.bridge.connected ? 'Chrome 扩展已连接' : status.enabled ? 'Native Host 已注册' : 'Chrome 整合未启用'}</strong>
              <span>{status.setup_note}</span>
            </div>
          </div>
          <div className="developerEndpointGrid">
            <div><span>Native Host</span><code>{status.enabled ? 'registered' : 'disabled'}</code></div>
            <div><span>Extension</span><code>{status.bridge.extension_version ? `${status.bridge.extension_version}${status.bridge.extension_compatible ? '' : ' · update required'}` : 'not connected'}</code></div>
            <div><span>Connected tabs</span><code>{status.bridge.claimed_tab_count}</code></div>
            <div><span>Authorized sites</span><code>{status.bridge.authorized_origin_count}</code></div>
          </div>
          <small>
            Extension ID: {status.extension_id}。模型不会获得 Native Host 路径、认证 token 或未授权标签页。
          </small>
          {status.last_error ? <div className="formError">{status.last_error}</div> : null}
          {setupMessage ? <div className="banner">{setupMessage}</div> : null}
          {status.enabled && !status.bridge.connected ? (
            <div className="chromeSetupGuide">
              <strong>首次连接现有 Chrome 需要加载一次扩展：</strong>
              <ol>
                <li>点“打开 Chrome 扩展页”，打开右上角“开发者模式”。</li>
                <li>点“打开扩展目录”，在 Chrome 里选择打开的 <code>ChatOS Chrome Extension</code> 文件夹。</li>
                <li>加载后回到这里点刷新，再在 Chrome 扩展弹窗里授权当前网页。</li>
              </ol>
            </div>
          ) : null}
          <div className="buttonRow">
            {status.enabled ? (
              <button className="ghostButton compact" disabled={saving} onClick={() => void disable()}>
                {saving ? '处理中' : '停用 Native Host'}
              </button>
            ) : (
              <button className="primaryButton compact" disabled={saving || !status.platform_supported} onClick={() => void enable()}>
                {saving ? '处理中' : '启用 Chrome 整合'}
              </button>
            )}
            <button
              className="ghostButton compact"
              disabled={!status.platform_supported}
              onClick={() => void openChromeExtensionsPage()}
            >
              <ExternalLink size={14} />打开 Chrome 扩展页
            </button>
            <button
              className="ghostButton compact"
              disabled={!status.extension_available}
              onClick={() => void openExtensionDirectory()}
            >
              <FolderOpen size={14} />打开扩展目录
            </button>
            <button
              className="ghostButton compact"
              disabled={!status.extension_available}
              onClick={() => void copyExtensionPath()}
            >
              <Copy size={14} />复制目录路径
            </button>
          </div>
        </div>
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
    case 'chrome_existing_session':
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
