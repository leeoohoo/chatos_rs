// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';
import {
  Activity,
  Boxes,
  CircleAlert,
  KeyRound,
  Link2,
  PackageCheck,
  RefreshCw,
  RotateCcw,
  ShieldCheck,
  Trash2,
  Unlink,
  X,
} from 'lucide-react';

import {
  api,
  type LocalPluginOAuthConnection,
  type LocalPluginStoreItem,
  type LocalPluginTransactionRecord,
  type PluginRuntimeTelemetrySnapshot,
} from '../../api';

type DetailTab = 'overview' | 'components' | 'permissions' | 'connections' | 'diagnostics';

export function PluginDetailDrawer({
  plugin,
  runtime,
  busy,
  installAvailable,
  onClose,
  onInstall,
  onRollback,
  onUninstall,
  onAutoUpdateChange,
  onOpenPermissions,
}: {
  plugin: LocalPluginStoreItem;
  runtime: PluginRuntimeTelemetrySnapshot;
  busy: boolean;
  installAvailable: boolean;
  onClose: () => void;
  onInstall: (plugin: LocalPluginStoreItem) => void;
  onRollback: (plugin: LocalPluginStoreItem) => void;
  onUninstall: (plugin: LocalPluginStoreItem) => void;
  onAutoUpdateChange: (plugin: LocalPluginStoreItem, enabled: boolean) => void;
  onOpenPermissions?: () => void;
}) {
  const [tab, setTab] = React.useState<DetailTab>('overview');
  const [connections, setConnections] = React.useState<LocalPluginOAuthConnection[]>([]);
  const [loadingConnections, setLoadingConnections] = React.useState(false);
  const [connectionError, setConnectionError] = React.useState<string | null>(null);
  const [connectionNotice, setConnectionNotice] = React.useState<string | null>(null);
  const [busyConnection, setBusyConnection] = React.useState<string | null>(null);
  const activeVersion = plugin.installation?.active_version || null;
  const installed = activeVersion
    ? plugin.installation?.versions[activeVersion] || null
    : null;
  const components = installed?.inventory.components || [];
  const permissions = installed?.inventory.permissions || [];
  const authComponentKeys = installed?.inventory.auth_component_keys || [];
  const autoUpdateState = plugin.auto_update_state || null;
  const autoUpdateAvailable = Boolean(plugin.installation && plugin.install_source === 'network');
  const runtimeSessions = runtime.sessions.filter((session) => session.plugin_id === plugin.plugin_id);
  const activeRuntimeSessions = runtimeSessions.filter(
    (session) => !['cancelled', 'expired'].includes(session.status),
  );
  const runtimeEvents = runtime.recent_events
    .filter((event) => event.plugin_id === plugin.plugin_id)
    .slice()
    .reverse()
    .slice(0, 20);

  const loadConnections = React.useCallback(async () => {
    if (!plugin.installation) {
      setConnections([]);
      return;
    }
    setLoadingConnections(true);
    setConnectionError(null);
    try {
      setConnections(await api.pluginOAuthConnections(plugin.plugin_id));
    } catch (err) {
      setConnectionError(err instanceof Error ? err.message : '读取 Plugin 账号连接失败');
    } finally {
      setLoadingConnections(false);
    }
  }, [plugin.installation, plugin.plugin_id]);

  React.useEffect(() => {
    setTab('overview');
    setConnectionNotice(null);
    void loadConnections();
  }, [loadConnections, plugin.plugin_id]);

  const beginConnection = async (componentKey: string) => {
    setBusyConnection(componentKey);
    setConnectionError(null);
    setConnectionNotice(null);
    try {
      const authorization = await api.beginPluginOAuth(plugin.plugin_id, componentKey);
      setConnectionNotice(authorization.browser_opened
        ? '已在系统浏览器中打开授权页面；完成后请刷新连接状态。'
        : authorization.browser_error || '请在系统浏览器中完成授权。');
    } catch (err) {
      setConnectionError(err instanceof Error ? err.message : '启动 Plugin OAuth 失败');
    } finally {
      setBusyConnection(null);
    }
  };

  const disconnect = async (connection: LocalPluginOAuthConnection) => {
    const key = `${connection.component_key}:${connection.provider}`;
    if (!window.confirm(`断开 ${connection.provider} 账号连接？本机 Access Token 和 Refresh Token 会被删除。`)) return;
    setBusyConnection(key);
    setConnectionError(null);
    setConnectionNotice(null);
    try {
      await api.disconnectPluginOAuth(
        plugin.plugin_id,
        connection.component_key,
        connection.provider,
      );
      setConnectionNotice(`${connection.provider} 已断开`);
      await loadConnections();
    } catch (err) {
      setConnectionError(err instanceof Error ? err.message : '断开 Plugin OAuth 失败');
    } finally {
      setBusyConnection(null);
    }
  };

  return (
    <div className="pluginDrawerBackdrop" role="presentation" onMouseDown={onClose}>
      <aside
        className="pluginDrawer"
        aria-label={`${plugin.display_name} Plugin 详情`}
        onMouseDown={(event) => event.stopPropagation()}
      >
        <header className="pluginDrawerHeader">
          <div className="pluginDrawerIdentity">
            <span className="pluginLogo large"><PackageCheck size={22} /></span>
            <div>
              <span className="pageEyebrow">{plugin.publisher} · {plugin.visibility === 'public' ? '公开' : '个人'}</span>
              <h3>{plugin.display_name}</h3>
              <p>{plugin.name}</p>
            </div>
          </div>
          <button type="button" className="iconButton" onClick={onClose} aria-label="关闭 Plugin 详情">
            <X size={18} />
          </button>
        </header>

        <div className="pluginDrawerStatusRow">
          <span className={`pluginLifecycle ${lifecycleTone(plugin.lifecycle_status)}`}>
            {lifecycleLabel(plugin.lifecycle_status)}
          </span>
          <span>最新 v{plugin.latest_version}</span>
          {activeVersion ? <span>当前 v{activeVersion}</span> : null}
        </div>

        <nav className="pluginDetailTabs" aria-label="Plugin 详情页签">
          {([
            ['overview', '概览'],
            ['components', `组件 ${components.length || plugin.skill_ids.length}`],
            ['permissions', `权限 ${permissions.length}`],
            ['connections', `连接 ${connections.filter((item) => item.connected).length}`],
            ['diagnostics', '诊断'],
          ] as Array<[DetailTab, string]>).map(([id, label]) => (
            <button key={id} type="button" className={tab === id ? 'active' : ''} onClick={() => setTab(id)}>
              {label}
            </button>
          ))}
        </nav>

        <div className="pluginDrawerBody">
          {tab === 'overview' ? (
            <div className="pluginDetailStack">
              <section className="pluginDetailHero">
                <h4>{plugin.description}</h4>
                <div className="pluginMetaGrid">
                  <span><small>分类</small><strong>{plugin.category}</strong></span>
                  <span><small>Marketplace</small><strong>{plugin.marketplace_id}</strong></span>
                  <span><small>Release</small><strong>{plugin.latest_release_id}</strong></span>
                  <span><small>Catalog</small><strong>{plugin.artifact_revision || 'personal'}</strong></span>
                </div>
              </section>
              {!plugin.installation ? (
                <div className="pluginNotice neutral">
                  <CircleAlert size={17} />
                  <div>
                    <strong>尚未安装到本机 Plugin Registry</strong>
                    <p>{installAvailable
                      ? '可经已认证的 Local Connector Service 代理下载 npm tgz；本机仍会校验 Marketplace、Release 签名、SHA-512 integrity、artifact SHA-256、package.json 和发布的 bin。'
                      : '当前受信安装来源不可用；不会开放任意 URL 或本地路径安装作为替代。'}</p>
                  </div>
                </div>
              ) : null}
              {plugin.installation ? (
                <section className={`pluginAutoUpdateCard ${plugin.preference?.auto_update ? 'enabled' : ''}`}>
                  <div>
                    <strong>自动更新</strong>
                    <p>仅跟随 owner 授权的 stable Release；下载、验签、事务日志和回滚路径与手动更新完全相同。</p>
                  </div>
                  <label className="pluginAutoUpdateSwitch">
                    <input
                      type="checkbox"
                      checked={Boolean(plugin.preference?.enabled && plugin.preference?.auto_update)}
                      disabled={busy || !autoUpdateAvailable}
                      onChange={(event) => onAutoUpdateChange(plugin, event.target.checked)}
                    />
                    <span>{plugin.preference?.enabled && plugin.preference?.auto_update ? '已开启' : '已关闭'}</span>
                  </label>
                  {!autoUpdateAvailable ? <small>需要当前 Plugin 仍存在于已登录用户的可信 Marketplace Catalog。</small> : null}
                </section>
              ) : null}
            </div>
          ) : null}

          {tab === 'components' ? (
            <section className="pluginDetailStack">
              {components.length > 0 ? components.map((component) => (
                <article className="pluginInventoryCard" key={component.component_key}>
                  <span className="pluginInventoryIcon"><Boxes size={17} /></span>
                  <div>
                    <h4>{component.display_name}</h4>
                    <p>{component.component_key}</p>
                    <div className="pluginTagRow">
                      <span>{component.kind.replace(/_/g, ' ')}</span>
                      <span>{component.runtime_kind}</span>
                      <span>{component.required ? '必需' : '可选'}</span>
                    </div>
                  </div>
                </article>
              )) : plugin.skill_ids.map((skillId) => (
                <article className="pluginInventoryCard" key={skillId}>
                  <span className="pluginInventoryIcon"><Boxes size={17} /></span>
                  <div><h4>Skill component</h4><p>{skillId}</p></div>
                </article>
              ))}
            </section>
          ) : null}

          {tab === 'permissions' ? (
            <section className="pluginDetailStack">
              {permissions.length === 0 ? (
                <div className="pluginNotice neutral"><ShieldCheck size={17} /><div><strong>没有已安装权限快照</strong><p>安装后会显示签名 Manifest 中的精确权限要求。</p></div></div>
              ) : permissions.map((permission) => (
                <article className="pluginPermissionCard" key={`${permission.permission}:${permission.components?.join(',') || ''}`}>
                  <ShieldCheck size={17} />
                  <div>
                    <h4>{permission.permission}</h4>
                    <p>{permission.reason || 'Plugin Manifest permission requirement'}</p>
                    <span>{permission.required ? '必需权限' : '可选权限'}{permission.components?.length ? ` · ${permission.components.join('、')}` : ''}</span>
                  </div>
                </article>
              ))}
              {installed?.inventory.auth_component_keys.length ? (
                <div className="pluginNotice warning"><KeyRound size={17} /><div><strong>需要账号连接</strong><p>{installed.inventory.auth_component_keys.join('、')}</p></div></div>
              ) : null}
              {permissions.length > 0 ? <button type="button" className="ghostButton compact" onClick={onOpenPermissions}>打开系统权限</button> : null}
            </section>
          ) : null}

          {tab === 'connections' ? (
            <section className="pluginDetailStack">
              <div className="pluginConnectionHeader">
                <div><span className="pageEyebrow">LOCAL OAUTH VAULT</span><h4>账号连接</h4></div>
                <button type="button" className="ghostButton compact" disabled={loadingConnections} onClick={() => void loadConnections()}>
                  <RefreshCw className={loadingConnections ? 'spinIcon' : ''} size={14} />刷新
                </button>
              </div>
              {connectionError ? <div className="banner error">{connectionError}</div> : null}
              {connectionNotice ? <div className="banner success">{connectionNotice}</div> : null}
              {authComponentKeys.length === 0 ? (
                <div className="pluginNotice neutral"><KeyRound size={17} /><div><strong>不需要账号连接</strong><p>当前已安装 Release 没有声明 Connected App 组件。</p></div></div>
              ) : authComponentKeys.map((componentKey) => {
                const componentConnections = connections.filter((item) => item.component_key === componentKey);
                const connected = componentConnections.filter((item) => item.connected);
                return (
                  <article className="pluginConnectionCard" key={componentKey}>
                    <div className="pluginConnectionTitle"><span className="pluginInventoryIcon"><Link2 size={16} /></span><div><h4>{componentKey}</h4><p>{connected.length ? `${connected.length} 个有效连接` : '等待账号连接'}</p></div></div>
                    {componentConnections.map((connection) => (
                      <div className="pluginConnectionRecord" key={`${connection.component_key}:${connection.provider}`}>
                        <div><strong>{connection.provider}</strong><span>{connection.account_display || connection.resource}</span><small>{connection.scopes.join(' · ') || '默认 scopes'}</small></div>
                        {connection.connected ? <button type="button" className="ghostButton compact dangerText" disabled={busyConnection === `${connection.component_key}:${connection.provider}`} onClick={() => void disconnect(connection)}><Unlink size={14} />断开</button> : null}
                      </div>
                    ))}
                    {connected.length === 0 ? <button type="button" className="primaryButton compact" disabled={busyConnection === componentKey} onClick={() => void beginConnection(componentKey)}>{busyConnection === componentKey ? '正在打开' : '连接账号'}</button> : null}
                  </article>
                );
              })}
            </section>
          ) : null}

          {tab === 'diagnostics' ? (
            <section className="pluginDetailStack">
              <div className="pluginDiagnosticGrid">
                <span><small>Lifecycle</small><strong>{lifecycleLabel(plugin.lifecycle_status)}</strong></span>
                <span><small>Rollback</small><strong>{plugin.rollback_available ? '可用' : '无目标版本'}</strong></span>
                <span><small>Active release</small><strong>{installed?.release_id || '—'}</strong></span>
                <span><small>Signature key</small><strong>{installed?.signature_key_id || '—'}</strong></span>
                <span><small>Auto update</small><strong>{plugin.preference?.auto_update ? 'stable · enabled' : 'disabled'}</strong></span>
                <span><small>Last checked</small><strong>{formatTimestamp(autoUpdateState?.last_checked_at || '')}</strong></span>
                <span><small>Last attempt</small><strong>{formatTimestamp(autoUpdateState?.last_attempted_at || '')}</strong></span>
                <span><small>Next retry</small><strong>{formatTimestamp(autoUpdateState?.next_retry_at || '')}</strong></span>
                <span><small>Failures</small><strong>{autoUpdateState?.consecutive_failures || 0}</strong></span>
                <span><small>Runtime sessions</small><strong>{activeRuntimeSessions.length} active / {runtimeSessions.length} retained</strong></span>
                <span><small>Runtime revision</small><strong>{runtime.revision}</strong></span>
              </div>
              {autoUpdateState?.last_error ? (
                <div className="pluginNotice warning"><CircleAlert size={17} /><div><strong>最近自动更新失败</strong><p>{autoUpdateState.last_error}</p></div></div>
              ) : null}
              {plugin.active_transaction ? (
                <TransactionCard title="正在执行" transaction={plugin.active_transaction} />
              ) : null}
              {plugin.latest_transaction ? (
                <TransactionCard title="最近事务" transaction={plugin.latest_transaction} />
              ) : null}
              {runtimeSessions.length ? (
                <div className="pluginRuntimeSection">
                  <div className="pluginConnectionHeader">
                    <div><span className="pageEyebrow">RUN-SCOPED RUNTIME</span><h4>运行会话</h4></div>
                    <span>{activeRuntimeSessions.length} active</span>
                  </div>
                  <div className="pluginRuntimeList">
                    {runtimeSessions.map((session) => (
                      <article className={`pluginRuntimeCard ${runtimeStatusTone(session.status)}`} key={session.adapter_session_id}>
                        <span className="pluginInventoryIcon"><Activity size={16} /></span>
                        <div>
                          <div className="pluginRuntimeCardTitle">
                            <h4>{session.component_key}</h4>
                            <span>{runtimeStatusLabel(session.status)}</span>
                          </div>
                          <p>Run {session.run_id} · Session {session.adapter_session_id}</p>
                          <div className="pluginTagRow">
                            <span>{session.active_executions} active</span>
                            <span>{session.execution_count} executions</span>
                            {session.last_operation ? <span>{session.last_operation}</span> : null}
                            {session.last_tool_name ? <span>{session.last_tool_name}</span> : null}
                            {session.health_status ? <span>health {session.health_status}</span> : null}
                          </div>
                          {session.last_error ? <strong>{session.last_error}</strong> : null}
                          <small>{formatTimestamp(session.updated_at)}</small>
                        </div>
                      </article>
                    ))}
                  </div>
                </div>
              ) : null}
              {runtimeEvents.length ? (
                <div className="pluginRuntimeSection">
                  <div className="pluginConnectionHeader">
                    <div><span className="pageEyebrow">RECENT EVENTS</span><h4>运行事件</h4></div>
                    <span>最近 {runtimeEvents.length}</span>
                  </div>
                  <div className="pluginRuntimeEventList">
                    {runtimeEvents.map((event) => (
                      <article key={event.sequence}>
                        <i className={`pluginRuntimeEventDot ${runtimeStatusTone(event.status)}`} />
                        <div>
                          <strong>{event.phase} · {event.status}</strong>
                          <span>{event.tool_name || event.operation || event.component_key}</span>
                          {event.error ? <small>{event.error}</small> : null}
                        </div>
                        <time>{event.duration_ms === null || event.duration_ms === undefined ? '' : `${event.duration_ms} ms`}<br />{formatTimestamp(event.timestamp)}</time>
                      </article>
                    ))}
                  </div>
                </div>
              ) : null}
              {!plugin.active_transaction && !plugin.latest_transaction ? (
                <div className="pluginNotice neutral"><CircleAlert size={17} /><div><strong>暂无本机事务</strong><p>安装、更新、回滚和卸载记录会在这里显示。</p></div></div>
              ) : null}
              {!runtimeSessions.length && !runtimeEvents.length ? (
                <div className="pluginNotice neutral"><Activity size={17} /><div><strong>暂无运行遥测</strong><p>任务准备、工具执行、健康检查和取消事件会在这里显示。</p></div></div>
              ) : null}
            </section>
          ) : null}
        </div>

        <footer className="pluginDrawerActions">
          {plugin.installation ? (
            <>
              {plugin.update_available ? (
                <button type="button" className="primaryButton compact" disabled={!installAvailable || busy} onClick={() => onInstall(plugin)}>
                  更新到 v{plugin.latest_version}
                </button>
              ) : null}
              <button type="button" className="ghostButton compact" disabled={!plugin.rollback_available || busy} onClick={() => onRollback(plugin)}>
                <RotateCcw size={15} />回滚
              </button>
              <button type="button" className="dangerButton compact" disabled={busy} onClick={() => onUninstall(plugin)}>
                <Trash2 size={15} />卸载
              </button>
            </>
          ) : (
            <button type="button" className="primaryButton compact" disabled={!installAvailable || busy} onClick={() => onInstall(plugin)} title={installAvailable ? '通过可信 Marketplace 代理下载 npm tgz 并在本机安装' : '当前受信安装来源不可用'}>
              {busy ? '安装中' : installAvailable ? '安装' : '安装资源不可用'}
            </button>
          )}
        </footer>
      </aside>
    </div>
  );
}

function TransactionCard({
  title,
  transaction,
}: {
  title: string;
  transaction: NonNullable<LocalPluginStoreItem['latest_transaction']>;
}) {
  const progress = transactionDownloadProgress(transaction);
  return (
    <article className={`pluginTransactionCard ${transaction.last_error ? 'failed' : ''}`}>
      <span>{transaction.last_error ? <CircleAlert size={17} /> : <PackageCheck size={17} />}</span>
      <div>
        <h4>{title} · {transaction.operation}</h4>
        <p>{transaction.from_version || '—'} → {transaction.target_version || '—'} · {transaction.status}</p>
        {progress ? (
          <div className="pluginDownloadProgress">
            <div className={`pluginDownloadProgressTrack ${progress.percent === null ? 'indeterminate' : ''}`}>
              <i style={progress.percent === null ? undefined : { width: `${progress.percent}%` }} />
            </div>
            <span>{progress.label}</span>
          </div>
        ) : null}
        {transaction.last_error ? <strong>{transaction.last_error}</strong> : null}
        <small>{formatTimestamp(transaction.completed_at || transaction.updated_at)}</small>
      </div>
    </article>
  );
}

export function transactionDownloadProgress(
  transaction?: LocalPluginTransactionRecord | null,
): { label: string; percent: number | null } | null {
  if (!transaction || (transaction.downloaded_bytes <= 0 && transaction.status !== 'downloading')) {
    return null;
  }
  const downloaded = Math.max(0, transaction.downloaded_bytes || 0);
  const total = transaction.total_bytes && transaction.total_bytes > 0
    ? transaction.total_bytes
    : null;
  const percent = total === null
    ? null
    : Math.min(100, Math.max(0, Math.round((downloaded / total) * 100)));
  const label = total === null
    ? `已下载 ${formatByteCount(downloaded)} · 总大小未知`
    : `${formatByteCount(downloaded)} / ${formatByteCount(total)} · ${percent}%`;
  return { label, percent };
}

function formatByteCount(value: number): string {
  if (value < 1024) return `${value} B`;
  if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KiB`;
  return `${(value / (1024 * 1024)).toFixed(1)} MiB`;
}

export function lifecycleLabel(status: string): string {
  switch (status) {
    case 'not_installed': return '未安装';
    case 'downloading': return '下载中';
    case 'verifying': return '校验中';
    case 'installing': return '安装中';
    case 'installed': return '已安装';
    case 'updating': return '更新中';
    case 'rolling_back': return '回滚中';
    case 'uninstalling': return '卸载中';
    case 'needs_update': return '需要更新';
    case 'update_failed_rollback_available': return '更新失败，可回滚';
    case 'update_failed': return '更新失败';
    case 'install_failed':
    case 'rejected': return '安装失败';
    default: return status.replace(/_/g, ' ');
  }
}

export function lifecycleTone(status: string): string {
  if (['installed'].includes(status)) return 'ready';
  if (['needs_update', 'downloading', 'verifying', 'installing', 'updating', 'rolling_back', 'uninstalling'].includes(status)) return 'working';
  if (status.includes('failed') || status === 'rejected') return 'error';
  return 'neutral';
}

function formatTimestamp(value: string): string {
  if (!value) return '—';
  const parsed = new Date(value);
  return Number.isNaN(parsed.getTime()) ? value : parsed.toLocaleString();
}

function runtimeStatusLabel(status: string): string {
  switch (status) {
    case 'ready': return '就绪';
    case 'executing': return '执行中';
    case 'degraded': return '降级';
    case 'failed': return '失败';
    case 'cancelled': return '已取消';
    case 'expired': return '已过期';
    case 'started': return '开始';
    case 'succeeded': return '成功';
    default: return status;
  }
}

function runtimeStatusTone(status: string): string {
  if (['ready', 'succeeded'].includes(status)) return 'ready';
  if (['executing', 'started'].includes(status)) return 'working';
  if (['degraded'].includes(status)) return 'warning';
  if (['failed'].includes(status)) return 'error';
  return 'neutral';
}
