// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';

import type { LocalModelConfig, LocalModelSettings } from '../api';
import {
  normalizeThinkingLevelForProvider,
  thinkingOptionsForProvider,
  thinkingValueForProvider,
} from '../utils/modelConfigState';

interface Props {
  models: LocalModelConfig[];
  settings: LocalModelSettings;
  disabled: boolean;
  onChange: (settings: LocalModelSettings) => void;
  onSave: () => void;
}

export function LocalDefaultModelSettings({
  models,
  settings,
  disabled,
  onChange,
  onSave,
}: Props) {
  const enabledModels = models.filter((model) => model.enabled && model.model.trim());
  return (
    <section className="panel">
      <div className="panelHeader">
        <div>
          <h2>默认模型</h2>
          <p>这些默认模型和凭据只用于本地项目，并保存在当前客户端。</p>
        </div>
        <button className="primaryButton compact" disabled={disabled} onClick={onSave}>
          保存默认设置
        </button>
      </div>
      <div className="approvalFormGrid">
        <DefaultModelPair
          modelLabel="Memory 总结模型"
          thinkingLabel="Memory Thinking"
          modelId={settings.memory_summary_model_config_id}
          thinkingLevel={settings.memory_summary_thinking_level}
          models={enabledModels}
          onChange={(modelId, thinkingLevel) => onChange({
            ...settings,
            memory_summary_model_config_id: modelId,
            memory_summary_thinking_level: thinkingLevel,
          })}
        />
        <DefaultModelPair
          modelLabel="项目管理 Agent 模型"
          thinkingLabel="Agent Thinking"
          modelId={settings.project_management_agent_model_config_id}
          thinkingLevel={settings.project_management_agent_thinking_level}
          models={enabledModels}
          onChange={(modelId, thinkingLevel) => onChange({
            ...settings,
            project_management_agent_model_config_id: modelId,
            project_management_agent_thinking_level: thinkingLevel,
          })}
        />
        <DefaultModelPair
          modelLabel="环境初始化模型"
          thinkingLabel="环境初始化 Thinking"
          modelId={settings.environment_initialization_model_config_id}
          thinkingLevel={settings.environment_initialization_thinking_level}
          models={enabledModels}
          onChange={(modelId, thinkingLevel) => onChange({
            ...settings,
            environment_initialization_model_config_id: modelId,
            environment_initialization_thinking_level: thinkingLevel,
          })}
        />
        <DefaultModelPair
          modelLabel="命令审批模型"
          thinkingLabel="审批 Thinking"
          modelId={settings.command_approval_model_config_id}
          thinkingLevel={settings.command_approval_thinking_level}
          models={enabledModels}
          emptyLabel="自动选择可用模型"
          fallbackToFirst
          onChange={(modelId, thinkingLevel) => onChange({
            ...settings,
            command_approval_model_config_id: modelId,
            command_approval_thinking_level: thinkingLevel,
          })}
        />
      </div>
    </section>
  );
}

function DefaultModelPair({
  modelLabel,
  thinkingLabel,
  modelId,
  thinkingLevel,
  models,
  emptyLabel = '不指定',
  fallbackToFirst = false,
  onChange,
}: {
  modelLabel: string;
  thinkingLabel: string;
  modelId?: string | null;
  thinkingLevel?: string | null;
  models: LocalModelConfig[];
  emptyLabel?: string;
  fallbackToFirst?: boolean;
  onChange: (modelId: string | null, thinkingLevel: string | null) => void;
}) {
  const modelById = React.useMemo(
    () => new Map(models.map((model) => [model.id, model])),
    [models],
  );
  const selectedModel = modelById.get(modelId || '') || (fallbackToFirst ? models[0] : null) || null;
  return (
    <>
      <label>
        {modelLabel}
        <select
          value={modelId || ''}
          onChange={(event) => {
            const nextModelId = event.target.value || null;
            const nextModel = modelById.get(nextModelId || '') || (fallbackToFirst ? models[0] : null);
            onChange(
              nextModelId,
              normalizeThinkingLevelForProvider(nextModel?.provider, thinkingLevel),
            );
          }}
        >
          <option value="">{emptyLabel}</option>
          {models.map((model) => (
            <option key={model.id} value={model.id}>{model.name} · {model.model}</option>
          ))}
        </select>
      </label>
      <label>
        {thinkingLabel}
        <select
          value={thinkingValueForProvider(selectedModel?.provider, thinkingLevel)}
          disabled={!selectedModel}
          onChange={(event) => onChange(modelId || null, event.target.value || null)}
        >
          {thinkingOptionsForProvider(selectedModel?.provider).map((option) => (
            <option key={option.value || 'default'} value={option.value}>
              {option.label}
            </option>
          ))}
        </select>
      </label>
    </>
  );
}
