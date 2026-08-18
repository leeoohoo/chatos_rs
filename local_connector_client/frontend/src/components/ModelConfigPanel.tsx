// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';
import { Cloud, RefreshCw } from 'lucide-react';

import {
  api,
  type LocalModelConfig,
  type LocalModelSettings,
} from '../api';
import { LocalDefaultModelSettings } from './LocalDefaultModelSettings';

export function ModelConfigPanel() {
  const [items, setItems] = React.useState<LocalModelConfig[]>([]);
  const [settings, setSettings] = React.useState<LocalModelSettings>({});
  const [loading, setLoading] = React.useState(true);
  const [saving, setSaving] = React.useState(false);
  const [message, setMessage] = React.useState<string | null>(null);
  const [error, setError] = React.useState<string | null>(null);

  const applyResponse = React.useCallback((next: {
    items: LocalModelConfig[];
    settings: LocalModelSettings;
  }) => {
    setItems(next.items);
    setSettings(next.settings || {});
  }, []);

  const load = React.useCallback(async (showSuccess = false) => {
    setLoading(true);
    setError(null);
    if (showSuccess) {
      setMessage(null);
    }
    try {
      const next = await api.refreshModelConfigs();
      applyResponse(next);
      if (showSuccess) {
        setMessage(`已与云端同步 ${next.items.length} 个模型`);
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : '同步云端模型配置失败');
      try {
        applyResponse(await api.modelConfigs());
      } catch {
        // Keep the primary synchronization error visible.
      }
    } finally {
      setLoading(false);
    }
  }, [applyResponse]);

  React.useEffect(() => {
    void load();
  }, [load]);

  const saveSettings = async () => {
    setSaving(true);
    setMessage(null);
    setError(null);
    try {
      const next = await api.saveModelSettings({
        model_request_max_retries: settings.model_request_max_retries ?? 5,
        command_approval_model_config_id: settings.command_approval_model_config_id || null,
        command_approval_thinking_level: settings.command_approval_thinking_level || null,
      });
      setSettings(next);
      setMessage('审批模型已保存到本机');
    } catch (err) {
      setError(err instanceof Error ? err.message : '保存审批模型失败');
    } finally {
      setSaving(false);
    }
  };

  return (
    <section className="modelPage">
      <section className="panel">
        <div className="panelHeader">
          <div>
            <h2><Cloud size={18} />云端模型</h2>
            <p>模型供应商、凭据与运行参数统一由云端管理；客户端自动同步只读副本。</p>
          </div>
          <button
            className="iconButton"
            disabled={loading}
            onClick={() => void load(true)}
            title="同步云端模型"
          >
            <RefreshCw className={loading ? 'spinIcon' : ''} size={17} />
          </button>
        </div>
        {message ? <div className="banner">{message}</div> : null}
        {error ? <div className="formError">云端同步失败，当前显示本机最近一次缓存：{error}</div> : null}
        <div className="modelList cloudModelList">
          <div className="modelSectionTitle">
            <span>可用模型</span>
            <small>{items.length} 个</small>
          </div>
          {items.map((item) => (
            <div className={item.enabled ? 'modelRow' : 'modelRow muted'} key={item.id}>
              <div>
                <div className="modelTitleLine">
                  <strong>{item.name}</strong>
                  <span className={item.enabled ? 'status ok' : 'status warn'}>
                    {item.enabled ? '启用' : '停用'}
                  </span>
                  <span className={item.has_api_key ? 'status ok' : 'status bad'}>
                    {item.has_api_key ? '凭据已同步' : '缺少凭据'}
                  </span>
                </div>
                <span>{item.provider} · {item.model}</span>
                <span className="mono">{item.server_model_config_id || item.id}</span>
              </div>
            </div>
          ))}
          {!items.length ? (
            <div className="emptyState">{loading ? '正在同步云端模型...' : '云端还没有可用模型。'}</div>
          ) : null}
        </div>
      </section>

      <LocalDefaultModelSettings
        models={items}
        settings={settings}
        disabled={loading || saving}
        onChange={setSettings}
        onSave={() => void saveSettings()}
      />
    </section>
  );
}
