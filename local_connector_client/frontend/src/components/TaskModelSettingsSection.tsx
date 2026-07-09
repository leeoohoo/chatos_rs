// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';
import { ListChecks, RefreshCw } from 'lucide-react';

import { api, type LocalModelConfig } from '../api';
import {
  buildTaskModelConfigPayload,
  emptyTaskModelDraft,
  numericInput,
  taskDraftChanged,
  taskDraftFromModel,
  thinkingOptionsForProvider,
  type TaskModelDraft,
} from '../utils/modelConfigState';

export function TaskModelSettingsSection({
  items,
  loading,
  onReload,
}: {
  items: LocalModelConfig[];
  loading: boolean;
  onReload: () => Promise<void>;
}) {
  const [drafts, setDrafts] = React.useState<Record<string, TaskModelDraft>>({});
  const [saving, setSaving] = React.useState(false);
  const [message, setMessage] = React.useState<string | null>(null);
  const [error, setError] = React.useState<string | null>(null);

  const concreteModels = React.useMemo(
    () => items.filter((item) => item.model.trim()),
    [items],
  );

  React.useEffect(() => {
    setDrafts(Object.fromEntries(concreteModels.map((item) => [item.id, taskDraftFromModel(item)])));
  }, [concreteModels]);

  const updateDraft = (id: string, patch: Partial<TaskModelDraft>) => {
    setDrafts((current) => ({
      ...current,
      [id]: {
        ...(current[id] || emptyTaskModelDraft()),
        ...patch,
      },
    }));
  };

  const save = async () => {
    setSaving(true);
    setMessage(null);
    setError(null);
    try {
      let changed = 0;
      for (const item of concreteModels) {
        const draft = drafts[item.id];
        if (!draft || !taskDraftChanged(item, draft)) {
          continue;
        }
        await api.updateModelConfig(item.id, buildTaskModelConfigPayload(item, draft));
        changed += 1;
      }
      setMessage(changed ? `已保存 ${changed} 个任务模型配置` : '任务模型配置没有变化');
      await onReload();
    } catch (err) {
      setError(err instanceof Error ? err.message : '保存任务模型配置失败');
    } finally {
      setSaving(false);
    }
  };

  return (
    <section className="panel">
      <div className="panelHeader">
        <div>
          <h2><ListChecks size={18} />任务模型设置</h2>
          <p>按具体模型配置任务用途、任务 thinking 和任务运行参数。</p>
        </div>
        <div className="headerActions">
          <button className="iconButton" onClick={() => void onReload()} title="刷新任务模型">
            <RefreshCw size={17} />
          </button>
          <button className="primaryButton compact" disabled={loading || saving} onClick={() => void save()}>
            {saving ? '保存中' : '保存任务设置'}
          </button>
        </div>
      </div>
      {message ? <div className="banner">{message}</div> : null}
      {error ? <div className="formError">{error}</div> : null}
      <div className="taskModelList">
        {concreteModels.map((item) => {
          const draft = drafts[item.id] || taskDraftFromModel(item);
          return (
            <div className={draft.enabled ? 'taskModelRow' : 'taskModelRow muted'} key={item.id}>
              <div className="taskModelRowHeader">
                <div>
                  <strong>{item.name}</strong>
                  <span>{item.provider} · {item.model}</span>
                </div>
                <div className="modelActions">
                  <span className={draft.enabled ? 'status ok' : 'status warn'}>
                    {draft.enabled ? '启用' : '停用'}
                  </span>
                  <button
                    className="ghostButton compact"
                    type="button"
                    onClick={() => updateDraft(item.id, { enabled: !draft.enabled })}
                  >
                    {draft.enabled ? '停用' : '启用'}
                  </button>
                </div>
              </div>
              <div className="taskModelGrid">
                <label>
                  Task 用途
                  <input
                    value={draft.task_usage_scenario}
                    onChange={(event) => updateDraft(item.id, { task_usage_scenario: event.target.value })}
                    placeholder="例如: task planning / coding / review"
                  />
                </label>
                <label>
                  Task Thinking
                  <select
                    value={draft.task_thinking_level}
                    onChange={(event) => updateDraft(item.id, { task_thinking_level: event.target.value })}
                  >
                    {thinkingOptionsForProvider(item.provider).map((option) => (
                      <option key={option.value || 'default'} value={option.value}>
                        {option.label}
                      </option>
                    ))}
                  </select>
                </label>
                <label>
                  Temperature
                  <input
                    type="number"
                    step="0.1"
                    value={draft.temperature ?? ''}
                    onChange={(event) => updateDraft(item.id, { temperature: numericInput(event.target.value) })}
                  />
                </label>
                <label>
                  Max Tokens
                  <input
                    type="number"
                    value={draft.max_output_tokens ?? ''}
                    onChange={(event) => updateDraft(item.id, { max_output_tokens: numericInput(event.target.value) })}
                  />
                </label>
              </div>
            </div>
          );
        })}
        {!concreteModels.length ? (
          <div className="emptyState">{loading ? '正在读取任务模型配置...' : '还没有可配置的具体模型。'}</div>
        ) : null}
      </div>
    </section>
  );
}

