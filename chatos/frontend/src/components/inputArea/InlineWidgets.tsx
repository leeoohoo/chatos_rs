// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';
import { useI18n } from '../../i18n/I18nProvider';
import { cn, formatFileSize } from '../../lib/utils';
import type { AiModelConfig } from '../../types';
import { thinkingOptionsForProvider } from '../../lib/modelThinkingOptions';
import type { InputAreaRefObject } from './InputAreaComposerTypes';

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
  pickerRef: InputAreaRefObject<HTMLDivElement>;
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
  const { t } = useI18n();
  const selectedModel = React.useMemo(
    () => (selectedModelId
      ? enabledModels.find((model) => model.id === selectedModelId) || null
      : null),
    [enabledModels, selectedModelId],
  );

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
  };

  const currentModelName = selectedModelName || effectiveModelName || selectedModel?.model_name || '';

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

  const currentThinkingLevel = selectedThinkingLevel || effectiveThinkingLevel || selectedModel?.thinking_level || '';
  const thinkingOptions = thinkingOptionsForProvider(selectedModel?.provider, t);

  return (
    <div className="absolute -top-3 left-3 z-10" ref={pickerRef as React.Ref<HTMLDivElement>}>
      <button
        type="button"
        onClick={() => setPickerOpen((v) => !v)}
        disabled={disabled}
        className={cn(
          'px-2 py-0.5 rounded-full border bg-background text-xs shadow-sm',
          'hover:bg-accent hover:text-accent-foreground transition-colors',
          'disabled:opacity-50 disabled:cursor-not-allowed'
        )}
        title={t('inputArea.model.selectTitle')}
      >
        {currentAiLabel}
        <span className="ml-1">▾</span>
      </button>
      {pickerOpen && (
        <div className="absolute left-0 bottom-full mb-2 w-[360px] max-w-[calc(100vw-2rem)] bg-popover text-popover-foreground border rounded-md shadow-lg">
          <div className="space-y-2 p-3">
            <div>
              <label className="mb-1 block text-[11px] text-muted-foreground">{t('inputArea.model.providerConfig')}</label>
              <select
                value={selectedModelId || ''}
                disabled={disabled}
                onChange={(event) => handleProviderChange(event.target.value)}
                className="w-full rounded-md border bg-background px-2 py-1.5 text-xs text-foreground focus:outline-none focus:ring-1 focus:ring-primary"
              >
                <option value="">{t('inputArea.model.selectConfig')}</option>
                {enabledModels.map((model) => (
                  <option key={model.id} value={model.id}>
                    {`${model.name} (${model.provider || 'gpt'})`}
                  </option>
                ))}
              </select>
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
                {t('inputArea.model.done')}
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
            {t('inputArea.model.clearSelection')}
          </button>
        </div>
      )}
    </div>
  );
};

interface InputAreaSendButtonProps {
  onSend: () => void;
  disabled: boolean;
  canSend: boolean;
  showModelSelector: boolean;
  selectedModelId?: string | null;
}

export const InputAreaSendButton: React.FC<InputAreaSendButtonProps> = ({
  onSend,
  disabled,
  canSend,
  showModelSelector,
  selectedModelId,
}) => {
  const { t } = useI18n();

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
      title={showModelSelector && !selectedModelId ? t('inputArea.send.selectModelMessage') : t('inputArea.send.messageTitle')}
    >
      <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 19l9 2-9-18-9 18 9-2zm0 0v-8" />
      </svg>
    </button>
  );
};
