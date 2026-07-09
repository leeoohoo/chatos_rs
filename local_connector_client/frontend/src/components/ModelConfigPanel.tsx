// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';
import { Brain, CheckCircle2, KeyRound, RefreshCw, Settings2, Trash2 } from 'lucide-react';

import {
  api,
  type LocalModelConfig,
  type LocalModelCatalogResponse,
  type LocalModelSettings,
} from '../api';
import {
  buildImportedModelConfigPayload,
  buildProviderPreviewPayload,
  emptyModelDraft,
  findExistingImportedModel,
  formatProviderModelOption,
  groupLocalModelProviders,
  normalizeThinkingLevelForProvider,
  providerLabel,
  thinkingOptionsForProvider,
  thinkingValueForProvider,
  type LocalModelProviderGroup,
  type ModelDraftState,
} from '../utils/modelConfigState';
import { TaskModelSettingsSection } from './TaskModelSettingsSection';

export function ModelConfigPanel() {
  const [items, setItems] = React.useState<LocalModelConfig[]>([]);
  const [settings, setSettings] = React.useState<LocalModelSettings>({});
  const [draft, setDraft] = React.useState<ModelDraftState>(emptyModelDraft());
  const [loading, setLoading] = React.useState(true);
  const [saving, setSaving] = React.useState(false);
  const [message, setMessage] = React.useState<string | null>(null);
  const [error, setError] = React.useState<string | null>(null);
  const [modelCatalog, setModelCatalog] = React.useState<LocalModelCatalogResponse | null>(null);
  const [modelCatalogLoading, setModelCatalogLoading] = React.useState(false);
  const [modelCatalogError, setModelCatalogError] = React.useState<string | null>(null);

  const load = React.useCallback(async () => {
    setError(null);
    try {
      const next = await api.modelConfigs();
      setItems(next.items);
      setSettings(next.settings || {});
    } catch (err) {
      setError(err instanceof Error ? err.message : '读取模型配置失败');
    } finally {
      setLoading(false);
    }
  }, []);

  React.useEffect(() => {
    void load();
  }, [load]);

  const clearCatalog = () => {
    setModelCatalog(null);
    setModelCatalogError(null);
  };

  const resetDraft = () => {
    setDraft(emptyModelDraft());
    clearCatalog();
  };

  const updateDraft = (patch: Partial<ModelDraftState>) => {
    setDraft((current) => ({
      ...current,
      ...patch,
    }));
    clearCatalog();
  };

  const refreshModelCatalog = async () => {
    setModelCatalogLoading(true);
    setModelCatalogError(null);
    try {
      const catalog = await api.previewModelCatalog(buildProviderPreviewPayload(draft));
      setModelCatalog(catalog);
    } catch (err) {
      setModelCatalogError(err instanceof Error ? err.message : '读取模型列表失败');
    } finally {
      setModelCatalogLoading(false);
    }
  };

  const saveModel = async () => {
    setSaving(true);
    setMessage(null);
    setError(null);
    try {
      const catalog = modelCatalog?.models.length
        ? modelCatalog
        : await api.previewModelCatalog(buildProviderPreviewPayload(draft));
      if (!catalog.models.length) {
        throw new Error(catalog.error || '没有读取到可导入的模型，请先确认 API Key 和 Base URL 后刷新模型。');
      }
      let savedCount = 0;
      for (const providerModel of catalog.models) {
        const existing = findExistingImportedModel(items, draft, catalog.base_url, providerModel.id);
        const payload = buildImportedModelConfigPayload(draft, providerModel, catalog.base_url, existing);
        if (existing) {
          await api.updateModelConfig(existing.id, payload);
        } else {
          await api.saveModelConfig(payload);
        }
        savedCount += 1;
      }
      setMessage(`已保存供应商配置并导入 ${savedCount} 个模型`);
      resetDraft();
      await load();
    } catch (err) {
      setError(err instanceof Error ? err.message : '保存供应商配置失败');
    } finally {
      setSaving(false);
    }
  };

  const providerGroups = React.useMemo(
    () => groupLocalModelProviders(items),
    [items],
  );

  const providerDraftFromGroup = (group: LocalModelProviderGroup): ModelDraftState => ({
    ...emptyModelDraft(),
    id: group.items[0]?.id,
    name: group.name,
    provider: group.provider,
    base_url: group.base_url,
    api_key_text: '',
    clear_api_key: false,
    enabled: group.enabled_count > 0,
    supports_images: group.supports_images,
    supports_reasoning: group.supports_reasoning,
    supports_responses: group.supports_responses,
  });

  const editProviderGroup = (group: LocalModelProviderGroup) => {
    clearCatalog();
    setDraft(providerDraftFromGroup(group));
  };

  const refreshProviderGroup = async (group: LocalModelProviderGroup) => {
    const nextDraft = providerDraftFromGroup(group);
    setDraft(nextDraft);
    setModelCatalogLoading(true);
    setModelCatalogError(null);
    setMessage(null);
    setError(null);
    try {
      const catalog = await api.previewModelCatalog(buildProviderPreviewPayload(nextDraft));
      setModelCatalog(catalog);
      if (catalog.models.length) {
        setMessage(`已读取供应商 ${group.name} 的 ${catalog.models.length} 个模型`);
      } else if (catalog.error) {
        setModelCatalogError(catalog.error);
      }
    } catch (err) {
      setModelCatalogError(err instanceof Error ? err.message : '读取模型列表失败');
    } finally {
      setModelCatalogLoading(false);
    }
  };

  const syncProviderGroup = async (group: LocalModelProviderGroup) => {
    setSaving(true);
    setMessage(null);
    setError(null);
    try {
      for (const item of group.items) {
        await api.syncModelConfig(item.id);
      }
      setMessage(`已同步供应商 ${group.name} 的 ${group.items.length} 个模型`);
      await load();
    } catch (err) {
      setError(err instanceof Error ? err.message : '同步供应商元信息失败');
    } finally {
      setSaving(false);
    }
  };

  const editModel = (item: LocalModelConfig) => {
    clearCatalog();
    setDraft({
      id: item.id,
      server_model_config_id: item.server_model_config_id || undefined,
      name: item.name,
      provider: item.provider,
      model: item.model,
      base_url: item.base_url || '',
      api_key_text: '',
      clear_api_key: false,
      enabled: item.enabled,
      supports_images: item.supports_images,
      supports_reasoning: item.supports_reasoning,
      supports_responses: item.supports_responses,
      thinking_level: item.thinking_level || '',
      task_usage_scenario: item.task_usage_scenario || '',
      task_thinking_level: item.task_thinking_level || '',
      temperature: item.temperature ?? null,
      max_output_tokens: item.max_output_tokens ?? null,
    });
  };

  const deleteModel = async (item: LocalModelConfig) => {
    setMessage(null);
    setError(null);
    try {
      await api.deleteModelConfig(item.id);
      if (draft.id === item.id) {
        resetDraft();
      }
      setMessage(`模型已删除: ${item.name}`);
      await load();
    } catch (err) {
      setError(err instanceof Error ? err.message : '删除模型配置失败');
    }
  };

  const syncModel = async (item: LocalModelConfig) => {
    setMessage(null);
    setError(null);
    try {
      const synced = await api.syncModelConfig(item.id);
      setMessage(`元信息已同步: ${synced.name}`);
      await load();
    } catch (err) {
      setError(err instanceof Error ? err.message : '同步模型元信息失败');
    }
  };

  const saveSettings = async () => {
    setSaving(true);
    setMessage(null);
    setError(null);
    try {
      const next = await api.saveModelSettings(settings);
      setSettings(next);
      setMessage('默认模型设置已同步');
      await load();
    } catch (err) {
      setError(err instanceof Error ? err.message : '保存默认模型设置失败');
    } finally {
      setSaving(false);
    }
  };

  const enabledModels = items.filter((item) => item.enabled && item.model.trim());
  const modelById = React.useMemo(
    () => new Map(enabledModels.map((item) => [item.id, item])),
    [enabledModels],
  );
  const memoryModel = modelById.get(settings.memory_summary_model_config_id || '') || null;
  const projectAgentModel =
    modelById.get(settings.project_management_agent_model_config_id || '') || null;
  const approvalModel =
    modelById.get(settings.command_approval_model_config_id || '') || enabledModels[0] || null;

  return (
    <section className="modelPage">
      <section className="panel">
        <div className="panelHeader">
          <div>
            <h2><Brain size={18} />本地模型配置</h2>
            <p>API Key 和 Base URL 只保存在这台电脑；服务端只保存模型元信息和本地映射 id。</p>
          </div>
          <button className="iconButton" onClick={() => void load()} title="刷新模型">
            <RefreshCw size={17} />
          </button>
        </div>
        {message ? <div className="banner">{message}</div> : null}
        {error ? <div className="formError">{error}</div> : null}
        <div className="modelLayout">
          <div className="modelList">
            <div className="modelSectionTitle">
              <span>供应商配置</span>
              <small>{providerGroups.length} 个</small>
            </div>
            {providerGroups.map((group) => (
              <div className="modelRow providerRow" key={group.key}>
                <div>
                  <div className="modelTitleLine">
                    <strong>{group.name}</strong>
                    <span className="status ok">{group.items.length} 个模型</span>
                    <span className={group.enabled_count > 0 ? 'status ok' : 'status warn'}>
                      {group.enabled_count} 个启用
                    </span>
                    <span className={group.has_api_key ? 'status ok' : 'status bad'}>
                      {group.has_api_key ? 'Key 已保存' : '缺少 Key'}
                    </span>
                  </div>
                  <span>{providerLabel(group.provider)} · {group.base_url || '默认 Base URL'}</span>
                  <div className="providerModelChips">
                    {group.items.slice(0, 8).map((item) => (
                      <span key={item.id}>{item.model}</span>
                    ))}
                    {group.items.length > 8 ? <span>+{group.items.length - 8}</span> : null}
                  </div>
                </div>
                <div className="modelActions">
                  <button className="iconButton" title="编辑供应商" onClick={() => editProviderGroup(group)}>
                    <Settings2 size={16} />
                  </button>
                  <button
                    className="iconButton"
                    title="刷新供应商模型列表"
                    onClick={() => void refreshProviderGroup(group)}
                  >
                    <RefreshCw size={16} />
                  </button>
                  <button
                    className="iconButton"
                    title="同步供应商下的模型元信息"
                    onClick={() => void syncProviderGroup(group)}
                  >
                    <CheckCircle2 size={16} />
                  </button>
                </div>
              </div>
            ))}
            {!providerGroups.length ? (
              <div className="emptyState">{loading ? '正在读取供应商配置...' : '还没有保存过的供应商。'}</div>
            ) : null}

            <div className="modelSectionTitle">
              <span>导入模型</span>
              <small>{items.length} 个</small>
            </div>
            {items.map((item) => (
              <div className="modelRow" key={item.id}>
                <div>
                  <div className="modelTitleLine">
                    <strong>{item.name}</strong>
                    <span className={item.enabled ? 'status ok' : 'status warn'}>
                      {item.enabled ? '启用' : '停用'}
                    </span>
                    <span className={item.has_api_key ? 'status ok' : 'status bad'}>
                      {item.has_api_key ? 'Key 已保存' : '缺少 Key'}
                    </span>
                  </div>
                  <span>{item.provider} · {item.model}</span>
                  <span className="mono">{item.server_model_config_id || '未同步到服务端'}</span>
                </div>
                <div className="modelActions">
                  <button className="iconButton" title="编辑" onClick={() => editModel(item)}>
                    <Settings2 size={16} />
                  </button>
                  <button className="iconButton" title="同步元信息" onClick={() => void syncModel(item)}>
                    <RefreshCw size={16} />
                  </button>
                  <button className="iconButton danger" title="删除" onClick={() => void deleteModel(item)}>
                    <Trash2 size={16} />
                  </button>
                </div>
              </div>
            ))}
            {!items.length ? (
              <div className="emptyState">{loading ? '正在读取模型配置...' : '还没有导入具体模型。'}</div>
            ) : null}
          </div>

          <div className="modelEditor">
            <div className="panelHeader compactHeader">
              <div>
                <h2><KeyRound size={18} />{draft.id ? '编辑供应商' : '添加供应商'}</h2>
                <p>{draft.id ? '留空 API Key 会沿用本机已保存的值。' : '保存后会导入供应商返回的具体模型。'}</p>
              </div>
              {draft.id ? (
                <button className="ghostButton compact" onClick={() => resetDraft()}>
                  新建
                </button>
              ) : null}
            </div>
            <div className="approvalFormGrid">
              <label>
                名称
                <input value={draft.name} onChange={(event) => setDraft({ ...draft, name: event.target.value })} />
              </label>
              <label>
                Provider
                <select
                  value={draft.provider || 'gpt'}
                  onChange={(event) => updateDraft({ provider: event.target.value })}
                >
                  <option value="gpt">OpenAI</option>
                  <option value="openai_compatible">OpenAI Compatible</option>
                  <option value="deepseek">DeepSeek</option>
                  <option value="kimi">Kimi</option>
                  <option value="minimax">MiniMax</option>
                </select>
              </label>
              <label>
                Base URL
                <input
                  value={draft.base_url || ''}
                  onChange={(event) => updateDraft({ base_url: event.target.value })}
                />
              </label>
              <label>
                API Key
                <input
                  type="password"
                  value={draft.api_key_text}
                  onChange={(event) => updateDraft({ api_key_text: event.target.value, clear_api_key: false })}
                />
              </label>
              <div className="modelCatalogField">
                <span className="fieldLabel">供应商模型</span>
                <div className="modelSelectRow">
                  <button
                    type="button"
                    className="ghostButton compact"
                    onClick={() => void refreshModelCatalog()}
                    disabled={modelCatalogLoading || Boolean(draft.clear_api_key)}
                  >
                    <RefreshCw size={15} />
                    {modelCatalogLoading ? '读取中' : '刷新模型列表'}
                  </button>
                  <span className="catalogStatus">
                    {modelCatalog
                      ? `${modelCatalog.source === 'live' ? '已读取' : '使用缓存'} ${modelCatalog.models.length} 个模型 · ${modelCatalog.base_url}`
                      : '保存时会按供应商返回的模型列表导入具体模型。'}
                  </span>
                </div>
                {modelCatalog?.models.length ? (
                  <div className="catalogModelList">
                    {modelCatalog.models.slice(0, 12).map((model) => (
                      <span key={model.id}>{formatProviderModelOption(model)}</span>
                    ))}
                    {modelCatalog.models.length > 12 ? <span>+{modelCatalog.models.length - 12}</span> : null}
                  </div>
                ) : null}
                {modelCatalog?.error ? <span className="catalogError">{modelCatalog.error}</span> : null}
                {modelCatalogError ? <span className="catalogError">{modelCatalogError}</span> : null}
              </div>
              <label className="inlineSwitch">
                <span>启用</span>
                <input type="checkbox" checked={draft.enabled ?? true} onChange={(event) => setDraft({ ...draft, enabled: event.target.checked })} />
              </label>
              <label className="inlineSwitch">
                <span>Responses API</span>
                <input type="checkbox" checked={draft.supports_responses ?? true} onChange={(event) => setDraft({ ...draft, supports_responses: event.target.checked })} />
              </label>
              <label className="inlineSwitch">
                <span>图片输入</span>
                <input type="checkbox" checked={draft.supports_images || false} onChange={(event) => setDraft({ ...draft, supports_images: event.target.checked })} />
              </label>
              <label className="inlineSwitch">
                <span>Reasoning</span>
                <input type="checkbox" checked={draft.supports_reasoning || false} onChange={(event) => setDraft({ ...draft, supports_reasoning: event.target.checked })} />
              </label>
              {draft.id ? (
                <label className="inlineSwitch">
                  <span>清除已保存 Key</span>
                  <input
                    type="checkbox"
                    checked={draft.clear_api_key || false}
                    onChange={(event) =>
                      updateDraft({
                        clear_api_key: event.target.checked,
                        api_key_text: event.target.checked ? '' : draft.api_key_text,
                      })
                    }
                  />
                </label>
              ) : null}
            </div>
            <button className="primaryButton compact" disabled={saving || !draft.name.trim()} onClick={() => void saveModel()}>
              {saving ? '保存中' : '保存并导入模型'}
            </button>
          </div>
        </div>
      </section>

      <section className="panel">
        <div className="panelHeader">
          <div>
            <h2><Settings2 size={18} />默认模型</h2>
            <p>这些设置会同步模型 id 到服务端，服务端需要运行时再向本机换取 key。</p>
          </div>
          <button className="primaryButton compact" disabled={saving} onClick={() => void saveSettings()}>
            保存默认设置
          </button>
        </div>
        <div className="approvalFormGrid">
          <label>
            Memory 总结模型
            <select
              value={settings.memory_summary_model_config_id || ''}
              onChange={(event) => {
                const modelId = event.target.value || null;
                const nextModel = modelById.get(modelId || '') || null;
                setSettings({
                  ...settings,
                  memory_summary_model_config_id: modelId,
                  memory_summary_thinking_level: normalizeThinkingLevelForProvider(
                    nextModel?.provider,
                    settings.memory_summary_thinking_level,
                  ),
                });
              }}
            >
              <option value="">不指定</option>
              {enabledModels.map((item) => (
                <option key={item.id} value={item.id}>{item.name} · {item.model}</option>
              ))}
            </select>
          </label>
          <label>
            Memory Thinking
            <select
              value={thinkingValueForProvider(memoryModel?.provider, settings.memory_summary_thinking_level)}
              disabled={!memoryModel}
              onChange={(event) =>
                setSettings({ ...settings, memory_summary_thinking_level: event.target.value || null })
              }
            >
              {thinkingOptionsForProvider(memoryModel?.provider).map((option) => (
                <option key={option.value || 'default'} value={option.value}>
                  {option.label}
                </option>
              ))}
            </select>
          </label>
          <label>
            项目管理 Agent 模型
            <select
              value={settings.project_management_agent_model_config_id || ''}
              onChange={(event) => {
                const modelId = event.target.value || null;
                const nextModel = modelById.get(modelId || '') || null;
                setSettings({
                  ...settings,
                  project_management_agent_model_config_id: modelId,
                  project_management_agent_thinking_level: normalizeThinkingLevelForProvider(
                    nextModel?.provider,
                    settings.project_management_agent_thinking_level,
                  ),
                });
              }}
            >
              <option value="">不指定</option>
              {enabledModels.map((item) => (
                <option key={item.id} value={item.id}>{item.name} · {item.model}</option>
              ))}
            </select>
          </label>
          <label>
            Agent Thinking
            <select
              value={thinkingValueForProvider(projectAgentModel?.provider, settings.project_management_agent_thinking_level)}
              disabled={!projectAgentModel}
              onChange={(event) =>
                setSettings({ ...settings, project_management_agent_thinking_level: event.target.value || null })
              }
            >
              {thinkingOptionsForProvider(projectAgentModel?.provider).map((option) => (
                <option key={option.value || 'default'} value={option.value}>
                  {option.label}
                </option>
              ))}
            </select>
          </label>
          <label>
            命令审批模型
            <select
              value={settings.command_approval_model_config_id || ''}
              onChange={(event) => {
                const modelId = event.target.value || null;
                const nextModel = modelById.get(modelId || '') || enabledModels[0] || null;
                setSettings({
                  ...settings,
                  command_approval_model_config_id: modelId,
                  command_approval_thinking_level: normalizeThinkingLevelForProvider(
                    nextModel?.provider,
                    settings.command_approval_thinking_level,
                  ),
                });
              }}
            >
              <option value="">自动选择可用模型</option>
              {enabledModels.map((item) => (
                <option key={item.id} value={item.id}>{item.name} · {item.model}</option>
              ))}
            </select>
          </label>
          <label>
            审批 Thinking
            <select
              value={thinkingValueForProvider(approvalModel?.provider, settings.command_approval_thinking_level)}
              disabled={!approvalModel}
              onChange={(event) =>
                setSettings({ ...settings, command_approval_thinking_level: event.target.value || null })
              }
            >
              {thinkingOptionsForProvider(approvalModel?.provider).map((option) => (
                <option key={option.value || 'default'} value={option.value}>
                  {option.label}
                </option>
              ))}
            </select>
          </label>
        </div>
      </section>

      <TaskModelSettingsSection items={items} loading={loading} onReload={load} />
    </section>
  );
}
