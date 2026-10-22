import { useEffect, useState, type ReactNode } from "react";
import { useGeneralSettingsStore } from "../../general-settings/application/generalSettingsStore";
import { isAiModelSettingsDirty, useAiModelSettingsStore } from "../application/aiModelSettingsStore";
import {
  blankProviderDraft,
  providerDraft,
  selectableModels,
  type AiModelConfig,
  type AiModelDefaults,
  type AiModelProvider,
  type AiModelProviderDraft,
} from "../domain/aiModelSettings";
import type { AiModelSettingsPort } from "../ports/AiModelSettingsPort";
import "./aiModelSettings.css";

export function AiModelSettingsView({ ownerUserId, port }: { ownerUserId: string; port: AiModelSettingsPort }) {
  const snapshot = useAiModelSettingsStore((state) => state.snapshot);
  const draft = useAiModelSettingsStore((state) => state.draft);
  const isLoading = useAiModelSettingsStore((state) => state.isLoading);
  const isMutating = useAiModelSettingsStore((state) => state.isMutating);
  const error = useAiModelSettingsStore((state) => state.error);
  const notice = useAiModelSettingsStore((state) => state.notice);
  const load = useAiModelSettingsStore((state) => state.load);
  const updateDefaults = useAiModelSettingsStore((state) => state.updateDefaults);
  const updateModel = useAiModelSettingsStore((state) => state.updateModel);
  const save = useAiModelSettingsStore((state) => state.save);
  const clearMessage = useAiModelSettingsStore((state) => state.clearMessage);
  const reset = useAiModelSettingsStore((state) => state.reset);
  const language = useGeneralSettingsStore((state) => state.settings.interfaceLanguage);
  const english = language === "en-US";
  const [providersOpen, setProvidersOpen] = useState(false);

  useEffect(() => {
    void load(port, ownerUserId);
    return () => reset();
  }, [load, ownerUserId, port, reset]);

  if (!snapshot && isLoading) {
    return <ModelStatus loading title={english ? "Loading AI model settings" : "正在读取 AI 模型配置"} detail={english ? "Models, providers, and defaults are loading in parallel." : "正在并行读取模型、供应商与默认设置。"} />;
  }
  if (!snapshot || !draft) {
    return <ModelStatus title={english ? "AI model settings unavailable" : "AI 模型配置暂时不可用"} detail={error ?? (english ? "Unable to read the model catalog." : "无法读取模型目录。") } action={english ? "Retry" : "重试"} onAction={() => void load(port, ownerUserId, true)} />;
  }

  const dirty = isAiModelSettingsDirty({ snapshot, draft });
  return (
    <div className="ai-model-page" aria-label={english ? "AI model settings" : "AI 模型设置"}>
      {(error || notice) && (
        <div className={`ai-model-banner ${error ? "is-error" : "is-success"}`} role={error ? "alert" : "status"}>
          <span aria-hidden="true">{error ? "!" : "✓"}</span>
          <p>{error ?? notice}</p>
          <button onClick={clearMessage} type="button">{english ? "Dismiss" : "关闭"}</button>
        </div>
      )}

      <section className="ai-model-hero">
        <div>
          <span className="ai-model-hero__icon" aria-hidden="true">✦</span>
          <div><h2>{english ? "Unified Model Configuration" : "统一模型配置"}</h2><p>{english ? "Providers and cloud defaults are stored in ChatOS. The approval model stays on this Windows device." : "供应商与云端默认模型保存在 ChatOS；本机审批模型只保存在这台 Windows 设备。"}</p></div>
        </div>
        <div className="ai-model-actions">
          <button onClick={() => setProvidersOpen(true)} type="button">{english ? "Manage Providers" : "管理供应商"}</button>
          <button disabled={isLoading || isMutating} onClick={() => void load(port, ownerUserId, true)} type="button">{english ? "Sync" : "同步"}</button>
          <button className="is-primary" disabled={!dirty || isMutating} onClick={() => void save(port)} type="button">{isMutating ? (english ? "Saving…" : "保存中…") : (english ? "Save All" : "保存全部")}</button>
        </div>
      </section>

      <ModelCard icon="⇄" title={english ? "Default Models" : "默认模型"} subtitle={english ? "Only enabled models with available credentials can be selected." : "默认模型只从已启用且凭据可用的配置中选择。"}>
        <DefaultModelRow
          english={english}
          label={english ? "Memory Summary" : "Memory 总结"}
          detail={english ? "Compresses long-term conversation memory and generates summaries." : "用于压缩长期会话记忆与生成摘要。"}
          modelId={draft.memorySummaryModelId}
          thinking={draft.memorySummaryThinkingLevel}
          models={snapshot.models}
          onChange={(modelId, thinking) => updateDefaults({ memorySummaryModelId: modelId, memorySummaryThinkingLevel: thinking })}
        />
        <div className="ai-model-divider" />
        <DefaultModelRow
          english={english}
          label={english ? "Local Approval Agent" : "本机自动审批 Agent"}
          detail={english ? "Reviews project context locally, then approves, rejects, or asks you." : "在本机只读检查项目上下文后，决定批准、拒绝或转交用户。"}
          modelId={draft.approvalModelId}
          thinking={draft.approvalThinkingLevel}
          models={snapshot.models}
          onChange={(modelId, thinking) => updateDefaults({ approvalModelId: modelId, approvalThinkingLevel: thinking })}
        />
      </ModelCard>

      <ModelCard icon="↻" title={english ? "Request Resilience" : "请求容错"} subtitle={english ? "Maximum retries for temporary network or upstream failures." : "网络波动、限流或上游暂时不可用时的最大重试次数。"}>
        <div className="ai-model-retry-row"><div><strong>{english ? "Maximum model request retries" : "模型请求最大重试次数"}</strong><small>{english ? "Allowed range: 0–10. Default: 5." : "允许范围 0–10，默认 5。"}</small></div><div className="ai-model-stepper"><button disabled={draft.modelRequestMaxRetries <= 0 || isMutating} onClick={() => updateDefaults({ modelRequestMaxRetries: draft.modelRequestMaxRetries - 1 })} type="button">−</button><output>{draft.modelRequestMaxRetries}</output><button disabled={draft.modelRequestMaxRetries >= 10 || isMutating} onClick={() => updateDefaults({ modelRequestMaxRetries: draft.modelRequestMaxRetries + 1 })} type="button">＋</button></div></div>
      </ModelCard>

      <ModelCard icon="⌘" title={english ? "Task Execution Models" : "任务执行模型"} subtitle={english ? "Only controls choices when creating Tasks; chat, Agents, and defaults are unaffected." : "仅控制创建任务时的模型候选，不影响聊天、Agent 或默认模型选择。"}>
        {snapshot.models.length === 0 ? <div className="ai-model-empty">{english ? "Add a provider and refresh its catalog first." : "请先添加供应商并刷新模型目录。"}</div> : (
          <div className="ai-model-list">{snapshot.models.map((model) => {
            const item = draft.modelDrafts[model.id];
            if (!item) return null;
            return <ModelRow english={english} key={model.id} model={model} value={item} disabled={isMutating} onChange={(patch) => updateModel(model.id, patch)} />;
          })}</div>
        )}
      </ModelCard>

      {providersOpen && <ProviderManager english={english} ownerUserId={ownerUserId} port={port} providers={snapshot.providers} disabled={isMutating} onClose={() => setProvidersOpen(false)} />}
    </div>
  );
}

function DefaultModelRow({ english, label, detail, modelId, thinking, models, onChange }: { english: boolean; label: string; detail: string; modelId: string | null; thinking: string | null; models: AiModelConfig[]; onChange(modelId: string | null, thinking: string | null): void }) {
  const options = selectableModels(models);
  const selected = options.find((model) => model.id === modelId);
  return <div className="ai-model-default-row"><div><strong>{label}</strong><small>{detail}</small></div><div><select aria-label={`${label} ${english ? "model" : "模型"}`} onChange={(event) => { const id = event.target.value || null; const model = options.find((item) => item.id === id); const nextThinking = model?.supportsReasoning ? (model.taskThinkingLevel && model.thinkingLevels.includes(model.taskThinkingLevel) ? model.taskThinkingLevel : model.thinkingLevels[0] ?? null) : null; onChange(id, nextThinking); }} value={modelId ?? ""}><option value="">{english ? "Not configured" : "未配置"}</option>{options.map((model) => <option key={model.id} value={model.id}>{model.name} · {model.modelName}</option>)}</select><select aria-label={`${label} Thinking`} disabled={!selected?.supportsReasoning} onChange={(event) => onChange(modelId, event.target.value || null)} value={thinking ?? ""}><option value="">{selected?.supportsReasoning ? (english ? "Default" : "默认") : (english ? "Not applicable" : "不适用")}</option>{selected?.thinkingLevels.map((level) => <option key={level} value={level}>{thinkingLabel(level, english)}</option>)}</select></div></div>;
}

function ModelRow({ english, model, value, disabled, onChange }: { english: boolean; model: AiModelConfig; value: { taskUsageScenario: string; taskThinkingLevel: string; temperature: string; maxOutputTokens: string; taskEnabled: boolean }; disabled: boolean; onChange(patch: Partial<typeof value>): void }) {
  return <article className={`ai-model-row ${value.taskEnabled ? "" : "is-disabled"}`}><header><span aria-hidden="true">{model.supportsReasoning ? "✦" : "◌"}</span><div><strong>{model.name}</strong><code>{model.promptVendor} · {model.modelName}</code></div><div className="ai-model-badges">{!model.hasApiKey && <em>{english ? "Missing credential" : "缺少凭据"}</em>}{model.supportsImages && <i>{english ? "Images" : "图片"}</i>}</div><label className="ai-model-switch"><input aria-label={`${model.name} ${english ? "Use for Tasks" : "用于任务"}`} checked={value.taskEnabled} disabled={disabled} onChange={(event) => onChange({ taskEnabled: event.target.checked })} type="checkbox" /><span /></label></header><div className="ai-model-fields"><label className="is-wide"><span>{english ? "Task Usage" : "任务用途"}</span><input disabled={!value.taskEnabled || disabled} onChange={(event) => onChange({ taskUsageScenario: event.target.value })} placeholder={english ? "Coding, analysis, visual understanding…" : "例如：代码实现、分析、视觉理解"} value={value.taskUsageScenario} /></label><label><span>Thinking</span><select disabled={!value.taskEnabled || disabled || !model.supportsReasoning} onChange={(event) => onChange({ taskThinkingLevel: event.target.value })} value={value.taskThinkingLevel}><option value="">{english ? "Default" : "默认"}</option>{model.thinkingLevels.map((level) => <option key={level} value={level}>{thinkingLabel(level, english)}</option>)}</select></label><label><span>Temperature</span><input disabled={!value.taskEnabled || disabled} inputMode="decimal" onChange={(event) => onChange({ temperature: event.target.value })} placeholder="0–2" value={value.temperature} /></label><label><span>Max Tokens</span><input disabled={!value.taskEnabled || disabled} inputMode="numeric" onChange={(event) => onChange({ maxOutputTokens: event.target.value })} placeholder={english ? "Optional" : "可选"} value={value.maxOutputTokens} /></label></div></article>;
}

function ProviderManager({ english, port, providers, disabled, onClose }: { english: boolean; ownerUserId: string; port: AiModelSettingsPort; providers: AiModelProvider[]; disabled: boolean; onClose(): void }) {
  const createProvider = useAiModelSettingsStore((state) => state.createProvider);
  const updateProvider = useAiModelSettingsStore((state) => state.updateProvider);
  const refreshProvider = useAiModelSettingsStore((state) => state.refreshProvider);
  const deleteProvider = useAiModelSettingsStore((state) => state.deleteProvider);
  const [editor, setEditor] = useState<AiModelProviderDraft | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<AiModelProvider | null>(null);
  return <div className="ai-model-modal-backdrop"><section aria-label={english ? "Model Providers" : "模型供应商"} aria-modal="true" className="ai-model-modal" role="dialog"><header><div><h2>{editor ? (editor.providerId ? (english ? "Edit Provider" : "修改供应商") : (english ? "Add Provider" : "添加供应商")) : (english ? "Model Providers" : "模型供应商")}</h2><p>{editor ? (english ? "Save provider connection details and model capabilities." : "保存供应商连接信息和模型能力。") : (english ? "Credentials, Base URLs, and model catalogs are managed here." : "供应商负责凭据、Base URL 和模型目录同步。")}</p></div><div>{!editor && <button className="is-primary" disabled={disabled} onClick={() => setEditor(blankProviderDraft())} type="button">＋ {english ? "Add Provider" : "添加供应商"}</button>}<button aria-label={english ? "Close" : "关闭"} onClick={onClose} type="button">×</button></div></header>{editor ? <ProviderEditor english={english} draft={editor} disabled={disabled} onBack={() => setEditor(null)} onSave={async (draft) => { const success = draft.providerId ? await updateProvider(port, draft) : await createProvider(port, draft); if (success) setEditor(null); }} /> : <div className="ai-model-provider-list">{providers.length === 0 ? <div className="ai-model-empty">{english ? "No providers yet. Add one to import models." : "还没有模型供应商，添加后即可导入模型。"}</div> : providers.map((provider) => <article key={provider.id}><div className="ai-model-provider-main"><span aria-hidden="true">▤</span><div><div><strong>{provider.name}</strong><i className={provider.enabled ? "is-good" : ""}>{provider.enabled ? (english ? "Enabled" : "已启用") : (english ? "Disabled" : "已停用")}</i><i className={provider.hasApiKey ? "is-good" : "is-warning"}>{provider.hasApiKey ? (english ? "Credential saved" : "凭据已保存") : (english ? "Missing credential" : "缺少凭据")}</i></div><small>OpenAI Responses · Prompt: {provider.promptVendor}</small><code>{provider.baseUrl}</code></div><em>{provider.importedModelCount} {english ? "models" : "个模型"}</em></div>{provider.lastSyncError && <p className="ai-model-provider-error">! {provider.lastSyncError}</p>}<footer><div><span>{provider.supportsImages ? "✓" : "○"} {english ? "Images" : "图片"}</span><span>{provider.supportsReasoning ? "✓" : "○"} {english ? "Reasoning" : "推理"}</span></div><div><button disabled={disabled} onClick={() => void refreshProvider(port, provider.id)} type="button">↻ {english ? "Refresh Models" : "刷新模型"}</button><button disabled={disabled} onClick={() => setEditor(providerDraft(provider))} type="button">{english ? "Edit" : "修改"}</button><button className="is-danger" disabled={disabled} onClick={() => setDeleteTarget(provider)} type="button">{english ? "Delete" : "删除"}</button></div></footer></article>)}</div>}{deleteTarget && <div className="ai-model-confirm"><section role="alertdialog" aria-modal="true"><span aria-hidden="true">!</span><h3>{english ? "Delete provider?" : "删除供应商？"}</h3><p>{english ? `This also deletes model configurations imported from ${deleteTarget.name}.` : `这会同时删除由 ${deleteTarget.name} 导入的模型配置。`}</p><footer><button onClick={() => setDeleteTarget(null)} type="button">{english ? "Cancel" : "取消"}</button><button className="is-danger" disabled={disabled} onClick={async () => { if (await deleteProvider(port, deleteTarget.id)) setDeleteTarget(null); }} type="button">{english ? "Delete" : "删除"}</button></footer></section></div>}</section></div>;
}

function ProviderEditor({ english, draft, disabled, onBack, onSave }: { english: boolean; draft: AiModelProviderDraft; disabled: boolean; onBack(): void; onSave(draft: AiModelProviderDraft): Promise<void> }) {
  const [value, setValue] = useState(draft);
  const [error, setError] = useState<string | null>(null);
  const patch = (next: Partial<AiModelProviderDraft>) => setValue((current) => ({ ...current, ...next }));
  const submit = () => {
    if (!value.name.trim() || !/^https?:\/\/[^\s]+$/i.test(value.baseUrl.trim())) return setError(english ? "Enter a provider name and a valid Base URL." : "请填写供应商名称和有效的 Base URL。");
    if (!value.providerId && !value.apiKey.trim()) return setError(english ? "An API key is required when adding a provider." : "新增供应商时必须填写 API Key。");
    setError(null); void onSave(value);
  };
  return <div className="ai-model-editor">{error && <div className="ai-model-editor-error">! {error}</div>}<fieldset><legend>{english ? "Connection" : "连接信息"}</legend><label><span>{english ? "Name" : "名称"}</span><input autoFocus onChange={(event) => patch({ name: event.target.value })} placeholder={english ? "OpenAI Production" : "例如：OpenAI Production"} value={value.name} /></label><label><span>{english ? "Prompt template" : "Prompt 模板"}</span><select onChange={(event) => patch({ promptVendor: event.target.value as AiModelProviderDraft["promptVendor"] })} value={value.promptVendor}><option value="gpt">gpt</option><option value="deepseek">deepseek</option><option value="kimi">kimi</option><option value="glm">glm</option></select></label><label><span>Base URL</span><input onChange={(event) => patch({ baseUrl: event.target.value })} value={value.baseUrl} /></label><label><span>API Key</span><input disabled={value.clearApiKey} onChange={(event) => patch({ apiKey: event.target.value })} placeholder={value.providerId ? (english ? "Leave blank to keep existing" : "留空则保留现有密钥") : (english ? "Required" : "必填")} type="password" value={value.apiKey} /></label>{value.providerId && <label className="ai-model-check"><input checked={value.clearApiKey} onChange={(event) => patch({ clearApiKey: event.target.checked, apiKey: "" })} type="checkbox" /><span>{english ? "Delete the API key saved on the server" : "删除服务器中已保存的 API Key"}</span></label>}</fieldset><fieldset><legend>{english ? "Capabilities & Status" : "能力与状态"}</legend><Toggle label={english ? "Enable this provider" : "启用这个供应商"} checked={value.enabled} onChange={(enabled) => patch({ enabled })} /><Toggle label={english ? "Supports image input" : "支持图片输入"} checked={value.supportsImages} onChange={(supportsImages) => patch({ supportsImages })} /><Toggle label={english ? "Supports reasoning models" : "支持推理模型"} checked={value.supportsReasoning} onChange={(supportsReasoning) => patch({ supportsReasoning })} /></fieldset><footer><button onClick={onBack} type="button">{english ? "Back" : "返回"}</button><button className="is-primary" disabled={disabled} onClick={submit} type="button">{value.providerId ? (english ? "Save Changes" : "保存修改") : (english ? "Add Provider" : "添加供应商")}</button></footer></div>;
}

function Toggle({ label, checked, onChange }: { label: string; checked: boolean; onChange(value: boolean): void }) { return <label className="ai-model-toggle"><span>{label}</span><input checked={checked} onChange={(event) => onChange(event.target.checked)} type="checkbox" /><i /></label>; }
function ModelCard({ icon, title, subtitle, children }: { icon: string; title: string; subtitle: string; children: ReactNode }) { return <section className="ai-model-card"><header><span aria-hidden="true">{icon}</span><div><h3>{title}</h3><p>{subtitle}</p></div></header><div className="ai-model-card__body">{children}</div></section>; }
function ModelStatus({ title, detail, loading, action, onAction }: { title: string; detail: string; loading?: boolean; action?: string; onAction?(): void }) { return <div className="ai-model-status"><span aria-hidden="true">{loading ? <i className="mini-spinner" /> : "!"}</span><h2>{title}</h2><p>{detail}</p>{action && <button onClick={onAction} type="button">{action}</button>}</div>; }
function thinkingLabel(value: string, english: boolean) { return value === "none" ? (english ? "Off" : "关闭") : value; }
