// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';
import { CheckCircle2, PackageCheck, RefreshCw, Search, ShieldAlert, ShieldCheck } from 'lucide-react';

import {
  api,
  type LocalSkillCatalogItem,
  type LocalSkillCatalogResponse,
  type SystemPermissionsResponse,
} from '../api';
import {
  loadSystemPermissions,
  permissionsForSkill,
  systemPermissionReady,
} from '../systemPermissions';

const CATEGORY_LABELS: Record<string, string> = {
  automation: '自动化',
  creativity: '创作',
  development: '开发',
  documentation: '文档检索',
  figma: 'Figma',
  productivity: '生产力',
  video: '视频',
};

export function SkillSettingsPanel({ onOpenPermissions }: { onOpenPermissions?: () => void }) {
  const [catalog, setCatalog] = React.useState<LocalSkillCatalogResponse | null>(null);
  const [permissions, setPermissions] = React.useState<SystemPermissionsResponse | null>(null);
  const [loading, setLoading] = React.useState(true);
  const [syncing, setSyncing] = React.useState(false);
  const [savingId, setSavingId] = React.useState<string | null>(null);
  const [query, setQuery] = React.useState('');
  const [error, setError] = React.useState<string | null>(null);

  const load = React.useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const [nextCatalog, nextPermissions] = await Promise.all([
        api.skills(),
        loadSystemPermissions(),
      ]);
      setCatalog(nextCatalog);
      setPermissions(nextPermissions);
    } catch (err) {
      setError(err instanceof Error ? err.message : '读取 Skill 列表失败');
    } finally {
      setLoading(false);
    }
  }, []);

  React.useEffect(() => {
    void load();
  }, [load]);

  const sync = async () => {
    setSyncing(true);
    setError(null);
    try {
      await api.syncSkills();
      await load();
    } catch (err) {
      setError(err instanceof Error ? err.message : '检测本机 Skill 失败');
    } finally {
      setSyncing(false);
    }
  };

  const toggle = async (item: LocalSkillCatalogItem) => {
    const nextEnabled = !item.user_enabled;
    const missingPermissions = permissionsForSkill(permissions, item.skill.id)
      .filter((permission) => !systemPermissionReady(permission));
    if (nextEnabled && missingPermissions.length > 0) {
      setError(`启用 ${item.skill.display_name} 前，请先处理：${missingPermissions.map((permission) => permission.label).join('、')}`);
      onOpenPermissions?.();
      return;
    }
    setSavingId(item.skill.id);
    setError(null);
    try {
      const updated = await api.setSkillEnabled(item.skill.id, nextEnabled);
      setCatalog((current) => current ? {
        ...current,
        items: current.items.map((candidate) => candidate.skill.id === item.skill.id ? updated : candidate),
      } : current);
    } catch (err) {
      setError(err instanceof Error ? err.message : '更新 Skill 设置失败');
    } finally {
      setSavingId(null);
    }
  };

  const normalizedQuery = query.trim().toLocaleLowerCase();
  const visibleItems = (catalog?.items || []).filter((item) => {
    if (!normalizedQuery) return true;
    return [item.skill.name, item.skill.display_name, item.skill.description || '']
      .some((value) => value.toLocaleLowerCase().includes(normalizedQuery));
  });
  const enabledCount = (catalog?.items || []).filter((item) => item.user_enabled).length;
  const availableCount = (catalog?.items || []).filter((item) => item.available).length;

  return (
    <section className="skillsPage">
      <div className="skillsSummary">
        <div>
          <span className="pageEyebrow">SIGNED INTERNAL BUNDLES</span>
          <h3>本机 Skills</h3>
          <p>安装包内置的 Admin Skills 默认关闭。只有你在这台 Local Connector 上启用后，Task Runner 才能选择。</p>
        </div>
        <div className="skillsMetrics">
          <span><strong>{catalog?.total || 0}</strong> 内置</span>
          <span><strong>{availableCount}</strong> 当前可用</span>
          <span><strong>{enabledCount}</strong> 已启用</span>
        </div>
      </div>

      <div className="skillsToolbar">
        <label className="skillsSearch">
          <Search size={16} />
          <input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="搜索 Skill" />
        </label>
        <button className="ghostButton compact" type="button" disabled={syncing} onClick={() => void sync()}>
          <RefreshCw className={syncing ? 'spinIcon' : ''} size={16} />
          {syncing ? '检测中' : '检测本机 Skills'}
        </button>
      </div>

      {error ? <div className="banner error">{error}</div> : null}
      {loading ? <div className="skillsEmpty">正在读取本机 Skill 目录…</div> : null}
      {!loading && visibleItems.length === 0 ? <div className="skillsEmpty">没有匹配的 Skill。</div> : null}

      <div className="skillsGrid">
        {visibleItems.map((item) => {
          const category = item.skill.metadata.category || 'other';
          const implementationStatus = String(item.skill.metadata.extra?.implementation_status || 'planned');
          const canToggle = item.available || item.user_enabled;
          const linkedPermissions = permissionsForSkill(permissions, item.skill.id);
          return (
            <article className={`skillCard ${item.user_enabled ? 'enabled' : ''}`} key={item.skill.id}>
              <div className="skillCardHeader">
                <div className="skillIcon"><PackageCheck size={19} /></div>
                <div>
                  <div className="skillTitleLine">
                    <h4>{item.skill.display_name}</h4>
                    <span className={`skillStatus ${item.available ? 'available' : 'unavailable'}`}>
                      {statusLabel(item.status, implementationStatus)}
                    </span>
                  </div>
                  <span className="skillInternalName">{item.skill.name}</span>
                </div>
              </div>
              <p>{item.skill.description || '安装包内置 Skill'}</p>
              <div className="skillMeta">
                <span>{CATEGORY_LABELS[category] || category}</span>
                <span>{item.skill.content.entrypoint_kind || 'native'}</span>
                <span>v{item.skill.content.bundle_version || item.skill.metadata.version || '1.0.0'}</span>
              </div>
              {linkedPermissions.length > 0 ? (
                <div className="skillPermissions">
                  <span><ShieldAlert size={13} />所需系统能力</span>
                  <div>
                    {linkedPermissions.map((permission) => (
                      <button
                        type="button"
                        key={permission.id}
                        className={systemPermissionReady(permission) ? 'ready' : 'missing'}
                        onClick={() => onOpenPermissions?.()}
                        title={permission.note}
                      >
                        {permission.label} · {permission.status_label}
                      </button>
                    ))}
                  </div>
                </div>
              ) : null}
              {!item.available && item.reason ? <div className="skillReason">{item.reason}</div> : null}
              <div className="skillCardFooter">
                <span className="skillTrust"><ShieldCheck size={14} />系统签名 Bundle</span>
                <button
                  type="button"
                  className={`skillToggle ${item.user_enabled ? 'on' : ''}`}
                  disabled={savingId === item.skill.id || !canToggle}
                  onClick={() => void toggle(item)}
                  aria-pressed={item.user_enabled}
                >
                  <span />
                  {savingId === item.skill.id ? '保存中' : item.user_enabled ? '已启用' : '启用'}
                </button>
              </div>
            </article>
          );
        })}
      </div>

      <div className="skillsFuture">
        <CheckCircle2 size={18} />
        <div><strong>用户安装 Skills</strong><p>接口和页面位置已预留，当前版本只允许使用安装包原生签名 Bundle。</p></div>
        <button type="button" className="ghostButton compact" disabled>即将开放</button>
      </div>
    </section>
  );
}

function statusLabel(status: string, implementationStatus: string): string {
  if (implementationStatus !== 'ready') return '尚未适配';
  switch (status) {
    case 'available': return '可用';
    case 'offline': return '客户端离线';
    case 'not_installed': return '未上报';
    case 'unavailable': return '依赖未就绪';
    case 'missing_dependency': return '缺少依赖';
    case 'unsupported': return '当前平台不支持';
    case 'error': return '检测失败';
    default: return status || '不可用';
  }
}
