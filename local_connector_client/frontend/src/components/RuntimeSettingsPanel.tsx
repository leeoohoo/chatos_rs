// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';
import { RefreshCw, Settings2 } from 'lucide-react';

import { api, type LocalRuntimeSettings } from '../api';

const DEFAULT_AI_AGENT_MAX_ITERATIONS = 600;

export function RuntimeSettingsPanel() {
  const [settings, setSettings] = React.useState<LocalRuntimeSettings>({
    ai_agent_max_iterations: DEFAULT_AI_AGENT_MAX_ITERATIONS,
  });
  const [loading, setLoading] = React.useState(true);
  const [saving, setSaving] = React.useState(false);
  const [message, setMessage] = React.useState<string | null>(null);
  const [error, setError] = React.useState<string | null>(null);

  const load = React.useCallback(async () => {
    setError(null);
    try {
      const next = await api.runtimeSettings();
      setSettings({
        ai_agent_max_iterations: next.ai_agent_max_iterations || DEFAULT_AI_AGENT_MAX_ITERATIONS,
      });
    } catch (err) {
      setError(err instanceof Error ? err.message : '读取运行配置失败');
    } finally {
      setLoading(false);
    }
  }, []);

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
      });
      setSettings(next);
      setMessage('运行配置已保存');
    } catch (err) {
      setError(err instanceof Error ? err.message : '保存运行配置失败');
    } finally {
      setSaving(false);
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
        <button className="primaryButton compact" disabled={saving} onClick={() => void save()}>
          {saving ? '保存中' : '保存配置'}
        </button>
      </section>
    </section>
  );
}
