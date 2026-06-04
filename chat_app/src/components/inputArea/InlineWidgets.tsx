import React from 'react';
import { cn, formatFileSize } from '../../lib/utils';
import type { AiModelConfig } from '../../types';
import { useApiClient } from '../../lib/api/ApiClientContext';
import type { AiProviderModelOptionResponse } from '../../lib/api/client/types';

interface InputAreaAttachmentsPreviewProps {
  attachments: File[];
  onRemoveAttachment: (index: number) => void;
}

export const InputAreaAttachmentsPreview: React.FC<InputAreaAttachmentsPreviewProps> = ({
  attachments,
  onRemoveAttachment,
}) => {
  if (attachments.length === 0) {
    return null;
  }

  return (
    <div className="mb-3 flex flex-wrap gap-2">
      {attachments.map((file, index) => (
        <div
          key={index}
          className="flex items-center gap-2 bg-muted px-3 py-2 rounded-lg text-sm"
        >
          <svg className="w-4 h-4 text-muted-foreground" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15.172 7l-6.586 6.586a2 2 0 102.828 2.828l6.414-6.586a4 4 0 00-5.656-5.656l-6.415 6.585a6 6 0 108.486 8.486L20.5 13" />
          </svg>
          <span className="truncate max-w-32">{file.name}</span>
          <span className="text-xs text-muted-foreground">({formatFileSize(file.size)})</span>
          <button
            onClick={() => onRemoveAttachment(index)}
            className="text-muted-foreground hover:text-destructive transition-colors"
          >
            <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>
      ))}
    </div>
  );
};

interface InputAreaErrorBannersProps {
  attachError: string | null;
  projectFileError: string | null;
  workspaceError: string | null;
}

export const InputAreaErrorBanners: React.FC<InputAreaErrorBannersProps> = ({
  attachError,
  projectFileError,
  workspaceError,
}) => (
  <>
    {attachError && (
      <div className="-mt-2 mb-3 text-xs text-destructive">{attachError}</div>
    )}
    {projectFileError && (
      <div className="-mt-2 mb-3 text-xs text-destructive">{projectFileError}</div>
    )}
    {workspaceError && (
      <div className="-mt-2 mb-3 text-xs text-destructive">{workspaceError}</div>
    )}
  </>
);

interface InputAreaFloatingModelPickerProps {
  showModelSelector: boolean;
  hasAiOptions: boolean;
  pickerRef: React.RefObject<HTMLDivElement>;
  disabled: boolean;
  currentAiLabel: string;
  effectiveModelName: string | null;
  effectiveThinkingLevel: string | null;
  pickerOpen: boolean;
  setPickerOpen: React.Dispatch<React.SetStateAction<boolean>>;
  enabledModels: AiModelConfig[];
  selectedModelId?: string | null;
  selectedModelName?: string | null;
  selectedThinkingLevel?: string | null;
  onModelChange?: (modelId: string | null) => void;
  onModelNameChange?: (modelName: string | null) => void;
  onThinkingLevelChange?: (level: string | null) => void;
  onModelRuntimeChange?: (selection: {
    selectedModelId?: string | null;
    selectedModelName?: string | null;
    selectedThinkingLevel?: string | null;
  }) => void;
}

const thinkingOptionsForProvider = (provider?: string) => {
  const normalized = (provider || 'gpt').trim().toLowerCase();
  if (normalized === 'deepseek') {
    return [
      { value: '', label: '思考: 默认' },
      { value: 'none', label: '思考: 关闭' },
      { value: 'high', label: '思考: high' },
      { value: 'max', label: '思考: max' },
    ];
  }
  if (normalized === 'kimi' || normalized === 'kimik2' || normalized === 'moonshot') {
    return [
      { value: '', label: '思考: 默认' },
      { value: 'auto', label: '思考: 自动' },
      { value: 'none', label: '思考: 关闭' },
    ];
  }
  return [
    { value: '', label: '思考: 默认' },
    { value: 'none', label: '思考: none' },
    { value: 'minimal', label: '思考: minimal' },
    { value: 'low', label: '思考: low' },
    { value: 'medium', label: '思考: medium' },
    { value: 'high', label: '思考: high' },
    { value: 'xhigh', label: '思考: xhigh' },
  ];
};

export const InputAreaFloatingModelPicker: React.FC<InputAreaFloatingModelPickerProps> = ({
  showModelSelector,
  hasAiOptions,
  pickerRef,
  disabled,
  currentAiLabel,
  effectiveModelName,
  effectiveThinkingLevel,
  pickerOpen,
  setPickerOpen,
  enabledModels,
  selectedModelId,
  selectedModelName,
  selectedThinkingLevel,
  onModelChange,
  onModelNameChange,
  onThinkingLevelChange,
  onModelRuntimeChange,
}) => {
  const client = useApiClient();
  const selectedModel = React.useMemo(
    () => (selectedModelId
      ? enabledModels.find((model) => model.id === selectedModelId) || null
      : null),
    [enabledModels, selectedModelId],
  );
  const [modelOptions, setModelOptions] = React.useState<AiProviderModelOptionResponse[]>([]);
  const [modelOptionsConfigId, setModelOptionsConfigId] = React.useState<string | null>(null);
  const [modelOptionsLoading, setModelOptionsLoading] = React.useState(false);
  const [modelOptionsError, setModelOptionsError] = React.useState<string | null>(null);

  const loadModelOptions = React.useCallback(async (refresh = false) => {
    if (!selectedModelId) {
      setModelOptions([]);
      setModelOptionsConfigId(null);
      setModelOptionsError(null);
      return;
    }
    setModelOptionsLoading(true);
    try {
      const response = await client.getAiProviderModels(selectedModelId, { refresh });
      setModelOptions(Array.isArray(response.models) ? response.models : []);
      setModelOptionsConfigId(selectedModelId);
      setModelOptionsError(response.error || null);
    } catch (error) {
      setModelOptions([]);
      setModelOptionsConfigId(selectedModelId);
      setModelOptionsError(error instanceof Error ? error.message : '模型列表获取失败');
    } finally {
      setModelOptionsLoading(false);
    }
  }, [client, selectedModelId]);

  React.useEffect(() => {
    if (!pickerOpen || !selectedModelId || modelOptionsConfigId === selectedModelId) {
      return;
    }
    void loadModelOptions(false);
  }, [loadModelOptions, modelOptionsConfigId, pickerOpen, selectedModelId]);

  if (!showModelSelector || !hasAiOptions) {
    return null;
  }

  const handleProviderChange = (modelId: string) => {
    const nextModel = enabledModels.find((model) => model.id === modelId) || null;
    const nextModelName = nextModel?.model_name || null;
    const nextThinkingLevel = nextModel?.thinking_level || null;
    if (onModelRuntimeChange) {
      onModelRuntimeChange({
        selectedModelId: nextModel?.id || null,
        selectedModelName: nextModelName,
        selectedThinkingLevel: nextThinkingLevel,
      });
    } else {
      onModelChange?.(nextModel?.id || null);
      onModelNameChange?.(nextModelName);
      onThinkingLevelChange?.(nextThinkingLevel);
    }
    setModelOptions([]);
    setModelOptionsConfigId(null);
    setModelOptionsError(null);
  };

  const handleModelNameChange = (modelName: string | null) => {
    const nextValue = modelName || '';
    if (onModelRuntimeChange) {
      onModelRuntimeChange({
        selectedModelId: selectedModelId || null,
        selectedModelName: nextValue || null,
        selectedThinkingLevel: currentThinkingLevel || null,
      });
    } else {
      onModelNameChange?.(nextValue || null);
    }
  };

  const handleThinkingLevelChange = (level: string | null) => {
    const nextValue = level || '';
    if (onModelRuntimeChange) {
      onModelRuntimeChange({
        selectedModelId: selectedModelId || null,
        selectedModelName: currentModelName || null,
        selectedThinkingLevel: nextValue || null,
      });
    } else {
      onThinkingLevelChange?.(nextValue || null);
    }
  };

  const currentModelName = selectedModelName || effectiveModelName || selectedModel?.model_name || '';
  const currentThinkingLevel = selectedThinkingLevel || effectiveThinkingLevel || selectedModel?.thinking_level || '';
  const thinkingOptions = thinkingOptionsForProvider(selectedModel?.provider);

  return (
    <div className="absolute -top-3 left-3 z-10" ref={pickerRef}>
      <button
        type="button"
        onClick={() => setPickerOpen((v) => !v)}
        disabled={disabled}
        className={cn(
          'px-2 py-0.5 rounded-full border bg-background text-xs shadow-sm',
          'hover:bg-accent hover:text-accent-foreground transition-colors',
          'disabled:opacity-50 disabled:cursor-not-allowed'
        )}
        title="选择模型"
      >
        {currentAiLabel}
        <span className="ml-1">▾</span>
      </button>
      {pickerOpen && (
        <div className="absolute left-0 bottom-full mb-2 w-[360px] max-w-[calc(100vw-2rem)] bg-popover text-popover-foreground border rounded-md shadow-lg">
          <div className="space-y-2 p-3">
            <div>
              <label className="mb-1 block text-[11px] text-muted-foreground">供应商配置</label>
              <select
                value={selectedModelId || ''}
                disabled={disabled}
                onChange={(event) => handleProviderChange(event.target.value)}
                className="w-full rounded-md border bg-background px-2 py-1.5 text-xs text-foreground focus:outline-none focus:ring-1 focus:ring-primary"
              >
                <option value="">选择配置</option>
                {enabledModels.map((model) => (
                  <option key={model.id} value={model.id}>
                    {`${model.name} (${model.provider || 'gpt'})`}
                  </option>
                ))}
              </select>
            </div>

            <div>
              <div className="mb-1 flex items-center justify-between gap-2">
                <label className="block text-[11px] text-muted-foreground">模型名称</label>
                <button
                  type="button"
                  onClick={() => { void loadModelOptions(true); }}
                  disabled={disabled || !selectedModelId || modelOptionsLoading}
                  className="rounded border px-1.5 py-0.5 text-[11px] text-muted-foreground hover:bg-accent disabled:opacity-50"
                >
                  {modelOptionsLoading ? '刷新中' : '刷新'}
                </button>
              </div>
              <input
                type="text"
                value={currentModelName}
                disabled={disabled || !selectedModelId}
                onChange={(event) => handleModelNameChange(event.target.value || null)}
                placeholder={selectedModel ? selectedModel.model_name : '先选择供应商配置'}
                className="w-full rounded-md border bg-background px-2 py-1.5 text-xs text-foreground focus:outline-none focus:ring-1 focus:ring-primary disabled:opacity-50"
              />
              {modelOptionsError ? (
                <div className="mt-1 text-[11px] text-amber-600">{modelOptionsError}</div>
              ) : null}
              {modelOptions.length > 0 ? (
                <div className="mt-2 max-h-36 overflow-auto rounded-md border bg-background">
                  {modelOptions.slice(0, 80).map((model) => (
                    <button
                      key={model.id}
                      type="button"
                      className={cn(
                        'flex w-full items-center justify-between gap-2 px-2 py-1.5 text-left text-xs hover:bg-accent',
                        currentModelName === model.id && 'bg-accent/50',
                      )}
                      onMouseDown={(event) => event.preventDefault()}
                      onClick={() => handleModelNameChange(model.id)}
                    >
                      <span className="truncate">{model.id}</span>
                      {model.supports_reasoning ? (
                        <span className="shrink-0 rounded bg-primary/10 px-1.5 py-0.5 text-[10px] text-primary">
                          reasoning
                        </span>
                      ) : null}
                    </button>
                  ))}
                </div>
              ) : null}
            </div>

            <div className="grid grid-cols-[1fr_auto] items-center gap-2">
              <select
                value={currentThinkingLevel}
                disabled={disabled || !selectedModelId}
                onChange={(event) => handleThinkingLevelChange(event.target.value || null)}
                className="min-w-0 rounded-md border bg-background px-2 py-1.5 text-xs text-foreground focus:outline-none focus:ring-1 focus:ring-primary disabled:opacity-50"
              >
                {thinkingOptions.map((option) => (
                  <option key={option.value || 'default'} value={option.value}>
                    {option.label}
                  </option>
                ))}
              </select>
              <button
                type="button"
                className="rounded-md px-2 py-1.5 text-xs text-muted-foreground hover:bg-accent"
                onClick={() => setPickerOpen(false)}
              >
                完成
              </button>
            </div>
          </div>
          <div className="border-t" />
          <button
            className="w-full text-left px-3 py-1.5 text-sm text-muted-foreground hover:bg-accent hover:text-accent-foreground"
            onClick={() => {
              if (onModelRuntimeChange) {
                onModelRuntimeChange({
                  selectedModelId: null,
                  selectedModelName: null,
                  selectedThinkingLevel: null,
                });
              } else {
                onModelChange?.(null);
                onModelNameChange?.(null);
                onThinkingLevelChange?.(null);
              }
              setPickerOpen(false);
            }}
          >
            清除选择
          </button>
        </div>
      )}
    </div>
  );
};

interface InputAreaSendButtonProps {
  isStreaming: boolean;
  isStopping: boolean;
  onStop?: () => void;
  onSend: () => void;
  disabled: boolean;
  canSend: boolean;
  showModelSelector: boolean;
  selectedModelId?: string | null;
}

export const InputAreaSendButton: React.FC<InputAreaSendButtonProps> = ({
  isStreaming,
  isStopping,
  onStop,
  onSend,
  disabled,
  canSend,
  showModelSelector,
  selectedModelId,
}) => {
  if (isStreaming) {
    return (
      <button
        onClick={() => {
          if (onStop && !isStopping) {
            onStop();
          }
        }}
        disabled={isStopping}
        className={cn(
          'flex-shrink-0 p-2 rounded-md transition-colors',
          isStopping
            ? 'bg-amber-500 text-white'
            : 'bg-red-500 text-white hover:bg-red-600',
          'disabled:opacity-50 disabled:cursor-not-allowed'
        )}
        title={isStopping ? '停止中...' : '停止生成'}
        style={{ backgroundColor: isStopping ? '#f59e0b' : '#ef4444', color: 'white' }}
      >
        {isStopping ? (
          <svg className="w-5 h-5 animate-spin" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 3a9 9 0 109 9" />
          </svg>
        ) : (
          <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 6h12v12H6z" />
          </svg>
        )}
      </button>
    );
  }

  return (
    <button
      onClick={onSend}
      disabled={disabled || !canSend}
      className={cn(
        'flex-shrink-0 p-2 rounded-md transition-colors',
        'disabled:opacity-50 disabled:cursor-not-allowed',
        canSend && !disabled
          ? 'bg-primary text-primary-foreground hover:bg-primary/90'
          : 'text-muted-foreground'
      )}
      title={showModelSelector && !selectedModelId ? '请先选择模型' : 'Send message'}
    >
      <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 19l9 2-9-18-9 18 9-2zm0 0v-8" />
      </svg>
    </button>
  );
};
