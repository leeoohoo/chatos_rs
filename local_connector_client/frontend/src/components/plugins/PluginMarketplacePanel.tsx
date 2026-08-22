// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';
import {
  CircleAlert,
  PackageCheck,
  RefreshCw,
  RotateCcw,
  Search,
  ShieldCheck,
  Sparkles,
  Trash2,
} from 'lucide-react';

import { api, type LocalPluginStoreItem, type LocalPluginStoreSnapshot } from '../../api';
import {
  PluginDetailDrawer,
  lifecycleLabel,
  lifecycleTone,
  transactionDownloadProgress,
} from './PluginDetailDrawer';

type StoreScope = 'public' | 'personal';

export function PluginMarketplacePanel({
  onOpenPermissions,
}: {
  onOpenPermissions?: () => void;
}) {
  const [catalog, setCatalog] = React.useState<LocalPluginStoreSnapshot | null>(null);
  const [loading, setLoading] = React.useState(true);
  const [refreshing, setRefreshing] = React.useState(false);
  const [recovering, setRecovering] = React.useState(false);
  const [checkingUpdates, setCheckingUpdates] = React.useState(false);
  const [busyPluginId, setBusyPluginId] = React.useState<string | null>(null);
  const [selectedPluginId, setSelectedPluginId] = React.useState<string | null>(null);
  const [scope, setScope] = React.useState<StoreScope>('public');
  const [query, setQuery] = React.useState('');
  const [category, setCategory] = React.useState('all');
  const [error, setError] = React.useState<string | null>(null);
  const [notice, setNotice] = React.useState<string | null>(null);

  const load = React.useCallback(async (initial = false) => {
    if (initial) setLoading(true);
    else setRefreshing(true);
    setError(null);
    try {
      setCatalog(await api.plugins());
    } catch (err) {
      setError(err instanceof Error ? err.message : '读取 Plugin Catalog 失败');
    } finally {
      setLoading(false);
      setRefreshing(false);
    }
  }, []);

  React.useEffect(() => {
    void load(true);
  }, [load]);

  React.useEffect(() => {
    let cancelled = false;
    let cursor: string | undefined;
    const waitForEvents = async () => {
      while (!cancelled) {
        try {
          const event = await api.pluginEvents(cursor);
          if (cancelled) return;
          cursor = event.cursor;
          if (event.changed) await load(false);
        } catch {
          await new Promise((resolve) => window.setTimeout(resolve, 1500));
        }
      }
    };
    void waitForEvents();
    return () => { cancelled = true; };
  }, [load]);

  const selected = catalog?.items.find((item) => item.plugin_id === selectedPluginId) || null;
  const installed = (catalog?.items || []).filter((item) => Boolean(item.installation?.active_version));
  const categories = Array.from(new Set(
    (catalog?.items || []).filter((item) => item.visibility === scope).map((item) => item.category),
  )).sort();
  const normalizedQuery = query.trim().toLocaleLowerCase();
  const visible = (catalog?.items || []).filter((item) => {
    if (item.visibility !== scope) return false;
    if (category !== 'all' && item.category !== category) return false;
    if (!normalizedQuery) return true;
    return [item.display_name, item.name, item.description, item.publisher, item.category]
      .some((value) => value.toLocaleLowerCase().includes(normalizedQuery));
  });
  const featured = visible.filter((item) => item.featured);
  const byCategory = visible.reduce<Record<string, LocalPluginStoreItem[]>>((groups, item) => {
    (groups[item.category] ||= []).push(item);
    return groups;
  }, {});

  const recover = async () => {
    setRecovering(true);
    setError(null);
    setNotice(null);
    try {
      const report = await api.recoverPlugins();
      setNotice(`恢复完成：完成 ${report.completed_transactions}，回滚 ${report.rolled_back_transactions}，清理 ${report.cleaned_paths}`);
      if (report.errors.length) setError(report.errors.join('；'));
      await load(false);
    } catch (err) {
      setError(err instanceof Error ? err.message : '恢复 Plugin 事务失败');
    } finally {
      setRecovering(false);
    }
  };

  const checkUpdates = async () => {
    if (checkingUpdates) return;
    setCheckingUpdates(true);
    setError(null);
    setNotice(null);
    try {
      const report = await api.checkPluginUpdates();
      if (report.skipped_reason === 'not_authenticated') {
        setNotice('尚未登录，已跳过远程 Plugin 自动更新检查。');
      } else {
        setNotice(`更新检查完成：符合策略 ${report.eligible}，尝试 ${report.attempted}，更新 ${report.updated}，退避 ${report.deferred}，失败 ${report.failures}`);
      }
      if (report.errors.length) setError(report.errors.join('；'));
      await load(false);
    } catch (err) {
      setError(err instanceof Error ? err.message : '检查 Plugin 更新失败');
    } finally {
      setCheckingUpdates(false);
    }
  };

  const updateAutoUpdate = async (plugin: LocalPluginStoreItem, enabled: boolean) => {
    if (busyPluginId || !plugin.installation) return;
    setBusyPluginId(plugin.plugin_id);
    setError(null);
    setNotice(null);
    try {
      await api.updatePluginPreference(plugin.plugin_id, {
        enabled: plugin.preference?.enabled ?? true,
        auto_update: enabled,
        release_channel: 'stable',
        enabled_components: plugin.preference?.enabled_components || [],
      });
      setNotice(`${plugin.display_name} 自动更新已${enabled ? '开启' : '关闭'}`);
      await load(false);
    } catch (err) {
      setError(err instanceof Error ? err.message : '更新 Plugin 自动更新设置失败');
    } finally {
      setBusyPluginId(null);
    }
  };

  const rollback = async (plugin: LocalPluginStoreItem) => {
    if (!plugin.rollback_available || busyPluginId) return;
    const target = plugin.installation?.previous_version || '上一个已验证版本';
    if (!window.confirm(`确认将 ${plugin.display_name} 回滚到 ${target}？当前 Release 的本地凭据会被清理。`)) return;
    setBusyPluginId(plugin.plugin_id);
    setError(null);
    setNotice(null);
    try {
      await api.rollbackPlugin(plugin.plugin_id);
      setNotice(`${plugin.display_name} 已回滚到 ${target}`);
      await load(false);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Plugin 回滚失败');
    } finally {
      setBusyPluginId(null);
    }
  };

  const install = async (plugin: LocalPluginStoreItem) => {
    if (busyPluginId || !plugin.install_available) return;
    const action = plugin.installation ? '更新' : '安装';
    const detail = plugin.installation
      ? `当前 v${plugin.installation.active_version}，目标 v${plugin.latest_version}。旧版本会保留为已验证回滚目标。`
      : `将通过已登录的 Local Connector Service 可信代理下载并校验 npm MCP Release v${plugin.latest_version}。`;
    if (!window.confirm(`${action} ${plugin.display_name}？${detail}`)) return;
    setBusyPluginId(plugin.plugin_id);
    setError(null);
    setNotice(null);
    try {
      await api.installPlugin(plugin.plugin_id);
      setNotice(`${plugin.display_name} 已${action}到 v${plugin.latest_version}`);
      await load(false);
    } catch (err) {
      setError(err instanceof Error ? err.message : `Plugin ${action}失败`);
    } finally {
      setBusyPluginId(null);
    }
  };

  const uninstall = async (plugin: LocalPluginStoreItem) => {
    if (busyPluginId) return;
    if (!window.confirm(`确认卸载 ${plugin.display_name}？本地安装文件、Plugin 凭据和 Token 会被删除。此操作不会删除非敏感事务审计。`)) return;
    setBusyPluginId(plugin.plugin_id);
    setError(null);
    setNotice(null);
    try {
      await api.uninstallPlugin(plugin.plugin_id);
      setSelectedPluginId(null);
      setNotice(`${plugin.display_name} 已从这台设备卸载`);
      await load(false);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Plugin 卸载失败');
    } finally {
      setBusyPluginId(null);
    }
  };

  return (
    <section className="pluginStorePage">
      <div className="pluginStoreHero">
        <div>
          <span className="pageEyebrow">SIGNED PLUGIN MARKETPLACE</span>
          <h3>外挂程式</h3>
          <p>浏览可信 Catalog，检查精确 Release、组件和权限，并在本机完成恢复、回滚与卸载。</p>
        </div>
        <div className="pluginStoreMetrics">
          <span><strong>{catalog?.items.length || 0}</strong> Catalog</span>
          <span><strong>{installed.length}</strong> 已安装</span>
          <span><strong>{catalog?.items.filter((item) => item.update_available).length || 0}</strong> 可更新</span>
        </div>
      </div>

      <section className="installedPluginSection">
        <div className="pluginSectionTitle">
          <div><span className="pageEyebrow">ON THIS DEVICE</span><h4>已安装</h4></div>
          <div className="pluginHeaderActions">
            <button type="button" className="ghostButton compact" disabled={checkingUpdates} onClick={() => void checkUpdates()}>
              <RefreshCw className={checkingUpdates ? 'spinIcon' : ''} size={15} />
              {checkingUpdates ? '检查中' : '检查更新'}
            </button>
            <button type="button" className="ghostButton compact" disabled={recovering} onClick={() => void recover()}>
              <RotateCcw className={recovering ? 'spinIcon' : ''} size={15} />
              {recovering ? '恢复中' : '恢复事务'}
            </button>
            <button type="button" className="ghostButton compact" disabled={refreshing} onClick={() => void load(false)}>
              <RefreshCw className={refreshing ? 'spinIcon' : ''} size={15} />刷新
            </button>
          </div>
        </div>
        {installed.length ? (
          <div className="installedPluginStrip">
            {installed.map((plugin) => (
              <button type="button" key={plugin.plugin_id} onClick={() => setSelectedPluginId(plugin.plugin_id)}>
                <span className="pluginLogo"><PackageCheck size={18} /></span>
                <span><strong>{plugin.display_name}</strong><small>v{plugin.installation?.active_version}</small></span>
                <i className={`pluginStateDot ${lifecycleTone(plugin.lifecycle_status)}`} />
              </button>
            ))}
          </div>
        ) : <div className="pluginEmpty compact">这台设备还没有本机 Plugin Registry 安装记录。</div>}
      </section>

      <div className="pluginStoreToolbar">
        <div className="pluginScopeTabs" role="tablist" aria-label="Plugin Catalog 范围">
          <button type="button" className={scope === 'public' ? 'active' : ''} onClick={() => { setScope('public'); setCategory('all'); }}>公开</button>
          <button type="button" className={scope === 'personal' ? 'active' : ''} onClick={() => { setScope('personal'); setCategory('all'); }}>个人</button>
        </div>
        <label className="pluginSearch">
          <Search size={16} />
          <input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="搜索 Plugin、publisher 或分类" />
        </label>
        <select value={category} onChange={(event) => setCategory(event.target.value)} aria-label="筛选 Plugin 分类">
          <option value="all">全部分类</option>
          {categories.map((item) => <option key={item} value={item}>{item}</option>)}
        </select>
      </div>

      {error ? <div className="banner error">{error}</div> : null}
      {catalog?.network_catalog_error ? <div className="banner warning">远程 Marketplace 暂不可用：{catalog.network_catalog_error}</div> : null}
      {catalog?.auto_update_error ? <div className="banner warning">自动更新状态不可用：{catalog.auto_update_error}</div> : null}
      {notice ? <div className="banner success">{notice}</div> : null}
      {loading ? <div className="pluginEmpty">正在读取签名 Plugin Catalog…</div> : null}

      {!loading && featured.length > 0 ? (
        <section className="pluginCategorySection featured">
          <div className="pluginSectionTitle"><div><span className="pageEyebrow">FEATURED</span><h4><Sparkles size={17} />常用推荐</h4></div></div>
          <div className="pluginCardGrid">{featured.map((plugin) => (
            <PluginCard key={plugin.plugin_id} plugin={plugin} installAvailable={plugin.install_available} busy={busyPluginId === plugin.plugin_id} onDetail={setSelectedPluginId} onInstall={install} onRollback={rollback} onUninstall={uninstall} />
          ))}</div>
        </section>
      ) : null}

      {!loading && Object.keys(byCategory).map((group) => (
        <section className="pluginCategorySection" key={group}>
          <div className="pluginSectionTitle">
            <div><span className="pageEyebrow">CATEGORY</span><h4>{group}</h4></div>
            <span>{byCategory[group].length} 个</span>
          </div>
          <div className="pluginCardGrid">{byCategory[group].map((plugin) => (
            <PluginCard key={plugin.plugin_id} plugin={plugin} installAvailable={plugin.install_available} busy={busyPluginId === plugin.plugin_id} onDetail={setSelectedPluginId} onInstall={install} onRollback={rollback} onUninstall={uninstall} />
          ))}</div>
        </section>
      ))}

      {!loading && visible.length === 0 ? (
        <div className="pluginEmpty">
          <CircleAlert size={18} />
          <div><strong>{scope === 'personal' ? '暂无个人 Plugin' : '没有匹配的 Plugin'}</strong><p>{scope === 'personal' ? '从个人 Marketplace 安装的 Plugin 会显示在这里。' : '请调整搜索关键词或分类筛选。'}</p></div>
        </div>
      ) : null}

      <div className="pluginTrustFooter">
        <ShieldCheck size={18} />
        <div><strong>{catalog?.marketplace_name || 'ChatOS Bundled'}</strong><p>Catalog revision {catalog?.catalog_revision || '—'}；本机生命周期操作保留事务日志并在失败时关闭。</p></div>
      </div>

      {selected ? (
        <PluginDetailDrawer
          plugin={selected}
          runtime={catalog?.runtime || { schema_version: 1, revision: 0, sessions: [], recent_events: [] }}
          busy={busyPluginId === selected.plugin_id}
          installAvailable={selected.install_available}
          onClose={() => setSelectedPluginId(null)}
          onInstall={(plugin) => void install(plugin)}
          onRollback={(plugin) => void rollback(plugin)}
          onUninstall={(plugin) => void uninstall(plugin)}
          onAutoUpdateChange={(plugin, enabled) => void updateAutoUpdate(plugin, enabled)}
          onOpenPermissions={onOpenPermissions}
        />
      ) : null}
    </section>
  );
}

function PluginCard({
  plugin,
  installAvailable,
  busy,
  onDetail,
  onInstall,
  onRollback,
  onUninstall,
}: {
  plugin: LocalPluginStoreItem;
  installAvailable: boolean;
  busy: boolean;
  onDetail: (pluginId: string) => void;
  onInstall: (plugin: LocalPluginStoreItem) => Promise<void>;
  onRollback: (plugin: LocalPluginStoreItem) => Promise<void>;
  onUninstall: (plugin: LocalPluginStoreItem) => Promise<void>;
}) {
  const downloadProgress = transactionDownloadProgress(plugin.active_transaction);
  return (
    <article className={`pluginCard ${plugin.installation ? 'installed' : ''}`}>
      <div className="pluginCardHeader">
        <span className="pluginLogo"><PackageCheck size={19} /></span>
        <div><h5>{plugin.display_name}</h5><p>{plugin.publisher}</p></div>
        <span className={`pluginLifecycle ${lifecycleTone(plugin.lifecycle_status)}`}>{lifecycleLabel(plugin.lifecycle_status)}</span>
      </div>
      <p className="pluginCardDescription">{plugin.description}</p>
      {downloadProgress ? (
        <div className="pluginCardDownloadProgress">
          <div className={`pluginDownloadProgressTrack ${downloadProgress.percent === null ? 'indeterminate' : ''}`}>
            <i style={downloadProgress.percent === null ? undefined : { width: `${downloadProgress.percent}%` }} />
          </div>
          <span>{downloadProgress.label}</span>
        </div>
      ) : null}
      <div className="pluginTagRow">
        <span>{plugin.category}</span><span>v{plugin.latest_version}</span><span>{plugin.skill_ids.length} Skills</span>
        {plugin.preference?.enabled && plugin.preference?.auto_update ? <span>自动更新</span> : null}
      </div>
      <footer className="pluginCardActions">
        <button type="button" className="ghostButton compact" onClick={() => onDetail(plugin.plugin_id)}>详情</button>
        {plugin.installation ? (
          <div>
            {plugin.update_available ? <button type="button" className="primaryButton compact" disabled={!installAvailable || busy} onClick={() => void onInstall(plugin)}>更新</button> : null}
            {plugin.rollback_available ? <button type="button" className="iconButton compact" disabled={busy} onClick={() => void onRollback(plugin)} title="回滚"><RotateCcw size={15} /></button> : null}
            <button type="button" className="iconButton compact danger" disabled={busy} onClick={() => void onUninstall(plugin)} title="卸载"><Trash2 size={15} /></button>
          </div>
        ) : <button type="button" className="primaryButton compact" disabled={!installAvailable || busy} onClick={() => void onInstall(plugin)} title={installAvailable ? '通过可信 Marketplace 代理下载 npm tgz 并安装' : '当前安装来源不可用'}>{busy ? '安装中' : installAvailable ? '安装' : '资源不可用'}</button>}
      </footer>
    </article>
  );
}
