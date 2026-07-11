// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';
import {
  CheckCircle2,
  ChevronLeft,
  CloudOff,
  FolderOpen,
  LogOut,
  Plus,
  Server,
  Trash2,
} from 'lucide-react';

import { api, type ConnectorStatus, type FsEntry } from '../api';

const DEFAULT_CLOUD_URL =
  import.meta.env.VITE_LOCAL_CONNECTOR_CLOUD_BASE_URL || 'https://local-connector.jgoool.com';

export function AuthPanel({ onDone }: { onDone: (status: ConnectorStatus) => void }) {
  const [mode, setMode] = React.useState<'login' | 'register'>('login');
  const [cloudBaseUrl, setCloudBaseUrl] = React.useState(DEFAULT_CLOUD_URL);
  const [username, setUsername] = React.useState('');
  const [displayName, setDisplayName] = React.useState('');
  const [password, setPassword] = React.useState('');
  const [inviteCode, setInviteCode] = React.useState('');
  const [verificationCode, setVerificationCode] = React.useState('');
  const [deviceName, setDeviceName] = React.useState(defaultDeviceName());
  const [submitting, setSubmitting] = React.useState(false);
  const [sendingCode, setSendingCode] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);

  const sendCode = async () => {
    setSendingCode(true);
    setError(null);
    try {
      await api.sendRegisterEmailCode({
        cloud_base_url: cloudBaseUrl,
        email: username,
        invite_code: inviteCode,
      });
    } catch (err) {
      setError(err instanceof Error ? err.message : '验证码发送失败');
    } finally {
      setSendingCode(false);
    }
  };

  const submit = async (event: React.FormEvent) => {
    event.preventDefault();
    setSubmitting(true);
    setError(null);
    try {
      const payload = {
        cloud_base_url: cloudBaseUrl,
        user_service_base_url: cloudBaseUrl,
        username,
        password,
        device_name: deviceName,
      };
      const next =
        mode === 'login'
          ? await api.login(payload)
          : await api.register({
              ...payload,
              display_name: displayName,
              invite_code: inviteCode,
              verification_code: verificationCode,
            });
      onDone(next);
    } catch (err) {
      setError(err instanceof Error ? err.message : '登录失败');
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <form className="authPanel" onSubmit={submit}>
      <div className="segmented">
        <button type="button" className={mode === 'login' ? 'active' : ''} onClick={() => setMode('login')}>
          登录
        </button>
        <button type="button" className={mode === 'register' ? 'active' : ''} onClick={() => setMode('register')}>
          注册
        </button>
      </div>
      <label>
        Local Connector Service
        <input value={cloudBaseUrl} onChange={(event) => setCloudBaseUrl(event.target.value)} />
      </label>
      <div className="twoCols">
        <label>
          用户名
          <input value={username} onChange={(event) => setUsername(event.target.value)} />
        </label>
        <label>
          设备名
          <input value={deviceName} onChange={(event) => setDeviceName(event.target.value)} />
        </label>
      </div>
      {mode === 'register' ? (
        <label>
          显示名
          <input value={displayName} onChange={(event) => setDisplayName(event.target.value)} />
        </label>
      ) : null}
      {mode === 'register' ? (
        <>
          <label>
            邀请码
            <input value={inviteCode} onChange={(event) => setInviteCode(event.target.value)} />
          </label>
          <label>
            邮箱验证码
            <div className="inlineField">
              <input value={verificationCode} onChange={(event) => setVerificationCode(event.target.value)} />
              <button
                type="button"
                className="ghostButton compact"
                disabled={sendingCode}
                onClick={() => void sendCode()}
              >
                {sendingCode ? '发送中' : '发送验证码'}
              </button>
            </div>
          </label>
        </>
      ) : null}
      <label>
        密码
        <input type="password" value={password} onChange={(event) => setPassword(event.target.value)} />
      </label>
      {error ? <div className="formError">{error}</div> : null}
      <button className="primaryButton" disabled={submitting}>
        {submitting ? '处理中...' : mode === 'login' ? '登录并配对设备' : '注册并配对设备'}
      </button>
    </form>
  );
}

export function ConnectionCard({
  status,
  onStatus,
}: {
  status: ConnectorStatus;
  onStatus: (status: ConnectorStatus) => void;
}) {
  const logout = async () => {
    onStatus(await api.logout());
  };
  return (
    <section className="panel">
      <div className="panelHeader">
        <div>
          <h2><Server size={18} />连接状态</h2>
          <p>{status.cloud_base_url}</p>
        </div>
        <span className={status.connector_running ? 'status ok' : 'status warn'}>
          {status.connector_running ? '已连接' : '未连接'}
        </span>
      </div>
      <div className="metaGrid">
        <div><span>用户</span><strong>{status.user?.display_name || status.user?.username || '-'}</strong></div>
        <div><span>设备</span><strong>{status.device_name || status.device_id || '-'}</strong></div>
        <div><span>Device ID</span><strong className="mono">{status.device_id || '-'}</strong></div>
      </div>
      <button className="ghostButton" onClick={() => void logout()}>
        <LogOut size={16} />退出本机配对
      </button>
    </section>
  );
}

export function LocalBoundaryPanel({ status }: { status: ConnectorStatus }) {
  return (
    <section className="panel">
      <div className="panelHeader">
        <div>
          <h2><CloudOff size={18} />本机边界</h2>
          <p>目录、终端和沙箱运行在当前电脑，云端只通过已登录设备的长连接发起授权请求。</p>
        </div>
      </div>
      <div className="metaGrid">
        <div>
          <span>开放目录</span>
          <strong>{status.workspaces.length} 个</strong>
        </div>
        <div>
          <span>本地沙箱</span>
          <strong>{status.sandbox.enabled ? '已开启' : '已关闭'}</strong>
        </div>
        <div>
          <span>Docker</span>
          <strong>{status.docker.installed ? (status.docker.running ? '运行中' : '未运行') : '未安装'}</strong>
        </div>
      </div>
      <div className="boundaryList">
        <div><CheckCircle2 size={16} />Local Connector Core 执行 Docker、文件和终端操作。</div>
        <div><CheckCircle2 size={16} />Local Connector Service 只负责登录设备、保存配对和 relay 消息。</div>
        <div><CheckCircle2 size={16} />本地沙箱不调用云端 Sandbox Manager，也不复用云端沙箱实例。</div>
      </div>
    </section>
  );
}

export function WorkspacePanel({
  status,
  onStatus,
}: {
  status: ConnectorStatus;
  onStatus: (status: ConnectorStatus) => void;
}) {
  const [pickerOpen, setPickerOpen] = React.useState(false);
  return (
    <section className="panel">
      <div className="panelHeader">
        <div>
          <h2><FolderOpen size={18} />开放目录</h2>
          <p>默认关闭。只有这里授权过的目录，ChatOS 才能看到并用于创建项目或终端。</p>
        </div>
        <button className="primaryButton compact" onClick={() => setPickerOpen(true)}>
          <Plus size={16} />开放目录
        </button>
      </div>
      {status.workspaces.length === 0 ? (
        <div className="emptyState">还没有开放任何本地目录。</div>
      ) : (
        <div className="workspaceList">
          {status.workspaces.map((workspace) => (
            <div className="workspaceRow" key={workspace.id}>
              <div>
                <strong>{workspace.alias}</strong>
                <span>{workspace.absolute_root}</span>
              </div>
              <button
                className="iconButton danger"
                title="移除"
                onClick={async () => onStatus(await api.removeWorkspace(workspace.id))}
              >
                <Trash2 size={16} />
              </button>
            </div>
          ))}
        </div>
      )}
      {pickerOpen ? <DirectoryPicker onClose={() => setPickerOpen(false)} onStatus={onStatus} /> : null}
    </section>
  );
}

function DirectoryPicker({
  onClose,
  onStatus,
}: {
  onClose: () => void;
  onStatus: (status: ConnectorStatus) => void;
}) {
  const [path, setPath] = React.useState<string | null>(null);
  const [items, setItems] = React.useState<FsEntry[]>([]);
  const [parent, setParent] = React.useState<string | null>(null);
  const [selected, setSelected] = React.useState<Set<string>>(new Set());
  const [alias, setAlias] = React.useState('');
  const [error, setError] = React.useState<string | null>(null);

  const load = React.useCallback(async (nextPath?: string | null) => {
    setError(null);
    try {
      const result = await api.fsList(nextPath);
      setPath(result.path);
      setParent(result.parent || null);
      setItems(result.entries.filter((entry) => entry.is_dir));
    } catch (err) {
      setError(err instanceof Error ? err.message : '目录读取失败');
    }
  }, []);

  React.useEffect(() => {
    void load(null);
  }, [load]);

  const toggle = (entryPath: string) => {
    setSelected((current) => {
      const next = new Set(current);
      if (next.has(entryPath)) {
        next.delete(entryPath);
      } else {
        next.add(entryPath);
      }
      return next;
    });
  };

  const apply = async () => {
    setError(null);
    try {
      let latest: ConnectorStatus | null = null;
      for (const selectedPath of selected) {
        latest = await api.addWorkspace({
          path: selectedPath,
          alias: selected.size === 1 ? alias.trim() || undefined : undefined,
        });
      }
      if (latest) {
        onStatus(latest);
      }
      onClose();
    } catch (err) {
      setError(err instanceof Error ? err.message : '授权目录失败');
    }
  };

  return (
    <div className="modalBackdrop">
      <div className="modal">
        <div className="panelHeader">
          <div>
            <h2>选择要开放的本地目录</h2>
            <p className="mono">{path || '-'}</p>
          </div>
          <button className="iconButton" onClick={onClose}>×</button>
        </div>
        <div className="pickerToolbar">
          <button className="ghostButton" disabled={!parent} onClick={() => void load(parent)}>
            <ChevronLeft size={16} />上一级
          </button>
          <button className="ghostButton" disabled={!path} onClick={() => path && toggle(path)}>
            选择当前目录
          </button>
        </div>
        <div className="dirList">
          {items.map((entry) => (
            <div className="dirRow" key={entry.path}>
              <input
                type="checkbox"
                checked={selected.has(entry.path)}
                onChange={() => toggle(entry.path)}
              />
              <button type="button" onClick={() => void load(entry.path)}>
                <FolderOpen size={15} />{entry.name}
              </button>
            </div>
          ))}
        </div>
        <label>
          单目录别名
          <input value={alias} onChange={(event) => setAlias(event.target.value)} />
        </label>
        {selected.size > 0 ? (
          <div className="selectionSummary">已选择 {selected.size} 个目录</div>
        ) : null}
        {error ? <div className="formError">{error}</div> : null}
        <div className="modalActions">
          <button className="ghostButton" onClick={onClose}>取消</button>
          <button className="primaryButton compact" disabled={selected.size === 0} onClick={() => void apply()}>
            开放所选目录
          </button>
        </div>
      </div>
    </div>
  );
}

function defaultDeviceName(): string {
  return typeof navigator !== 'undefined' ? `Local Connector - ${navigator.platform || 'Desktop'}` : 'Local Connector';
}
