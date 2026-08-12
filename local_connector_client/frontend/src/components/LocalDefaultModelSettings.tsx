// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';
import { Brain } from 'lucide-react';

import type { LocalModelConfig, LocalModelSettings } from '../api';

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
  const enabledModels = models.filter(
    (model) => model.enabled && model.has_api_key && model.model.trim(),
  );
  const selectedModelId = settings.command_approval_model_config_id || '';
  const selectedModel = enabledModels.find((model) => model.id === selectedModelId) || null;
  const selectedModelExists = Boolean(selectedModel);
  const thinkingOptions = thinkingOptionsForProvider(selectedModel?.provider);
  const thinkingValue = thinkingOptions.some(
    (option) => option.value === (settings.command_approval_thinking_level || ''),
  )
    ? settings.command_approval_thinking_level || ''
    : '';

  return (
    <section className="panel">
      <div className="panelHeader">
        <div>
          <h2><Brain size={18} />本机审批 Agent 设置</h2>
          <p>模型来源跟随云端；本机仅配置审批模型及其审批运行参数。</p>
        </div>
        <button
          className="primaryButton compact"
          disabled={disabled || !selectedModelExists}
          onClick={onSave}
        >
          保存审批设置
        </button>
      </div>
      <div className="approvalFormGrid">
        <label>
          模型请求最大重试次数
          <input
            type="number"
            min={0}
            max={10}
            step={1}
            value={settings.model_request_max_retries ?? 5}
            disabled={disabled}
            onChange={(event) => onChange({
              ...settings,
              model_request_max_retries: Math.min(10, Math.max(0, Number(event.target.value) || 0)),
            })}
          />
          <small>网络波动、限流或上游暂时不可用时重试；默认 5 次。</small>
        </label>
      </div>
      <div className="approvalFormGrid">
        <label>
          命令审批模型
          <select
            value={selectedModelId}
            disabled={disabled || !enabledModels.length}
            onChange={(event) => {
              const nextModelId = event.target.value || null;
              const nextModel = enabledModels.find((model) => model.id === nextModelId) || null;
              onChange({
                ...settings,
                command_approval_model_config_id: nextModelId,
                command_approval_thinking_level: normalizeThinkingLevel(
                  nextModel?.provider,
                  settings.command_approval_thinking_level,
                ),
              });
            }}
          >
            <option value="">请选择审批模型</option>
            {enabledModels.map((model) => (
              <option key={model.id} value={model.id}>
                {model.name} · {model.model}
              </option>
            ))}
          </select>
        </label>
        <label>
          审批 Thinking
          <select
            value={thinkingValue}
            disabled={disabled || !selectedModel}
            onChange={(event) => onChange({
              ...settings,
              command_approval_thinking_level: event.target.value || null,
            })}
          >
            {thinkingOptions.map((option) => (
              <option key={option.value || 'default'} value={option.value}>
                {option.label}
              </option>
            ))}
          </select>
          <small>
            {enabledModels.length
              ? '供应商、凭据及其他模型运行参数直接使用云端配置。'
              : '请先在云端添加并启用带凭据的模型。'}
          </small>
        </label>
      </div>
    </section>
  );
}

function normalizeThinkingLevel(provider?: string | null, value?: string | null) {
  const normalized = (value || '').trim();
  if (!normalized) return null;
  return thinkingOptionsForProvider(provider).some((option) => option.value === normalized)
    ? normalized
    : null;
}

function thinkingOptionsForProvider(provider?: string | null) {
  const normalized = (provider || 'gpt').trim().toLowerCase().replace('-', '_');
  if (normalized === 'deepseek') {
    return [
      { value: '', label: '默认' },
      { value: 'none', label: '关闭' },
      { value: 'high', label: 'high' },
      { value: 'max', label: 'max' },
    ];
  }
  if (normalized === 'kimi' || normalized === 'kimik2' || normalized === 'moonshot') {
    return [
      { value: '', label: '默认' },
      { value: 'auto', label: 'auto' },
      { value: 'none', label: '关闭' },
    ];
  }
  if (normalized === 'glm' || normalized === 'zhipu' || normalized === 'zai') {
    return [
      { value: '', label: '默认' },
      { value: 'none', label: 'none' },
      { value: 'low', label: 'low' },
      { value: 'medium', label: 'medium' },
      { value: 'high', label: 'high' },
      { value: 'xhigh', label: 'xhigh' },
    ];
  }
  return [
    { value: '', label: '默认' },
    { value: 'none', label: 'none' },
    { value: 'minimal', label: 'minimal' },
    { value: 'low', label: 'low' },
    { value: 'medium', label: 'medium' },
    { value: 'high', label: 'high' },
    { value: 'xhigh', label: 'xhigh' },
  ];
}
