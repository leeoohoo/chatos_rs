import React from 'react';
import { cn, formatFileSize } from '../../lib/utils';
import type { AiModelConfig } from '../../types';

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
  pickerOpen: boolean;
  setPickerOpen: React.Dispatch<React.SetStateAction<boolean>>;
  enabledModels: AiModelConfig[];
  selectedModelId?: string | null;
  onModelChange?: (modelId: string | null) => void;
}

export const InputAreaFloatingModelPicker: React.FC<InputAreaFloatingModelPickerProps> = ({
  showModelSelector,
  hasAiOptions,
  pickerRef,
  disabled,
  currentAiLabel,
  pickerOpen,
  setPickerOpen,
  enabledModels,
  selectedModelId,
  onModelChange,
}) => {
  if (!showModelSelector || !hasAiOptions) {
    return null;
  }

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
        <div className="absolute left-0 bottom-full mb-2 w-64 max-h-64 overflow-auto bg-popover text-popover-foreground border rounded-md shadow-lg">
          {enabledModels.length > 0 && (
            <>
              <div className="px-2 py-1 text-[11px] uppercase tracking-wide text-muted-foreground">模型</div>
              {enabledModels.map((model) => (
                <button
                  key={model.id}
                  className={cn('w-full text-left px-3 py-1.5 text-sm hover:bg-accent hover:text-accent-foreground', selectedModelId === model.id && 'bg-accent/40')}
                  onClick={() => { onModelChange?.(model.id); setPickerOpen(false); }}
                >
                  {model.name} ({model.model_name})
                </button>
              ))}
            </>
          )}
          <div className="border-t" />
          <button
            className="w-full text-left px-3 py-1.5 text-sm text-muted-foreground hover:bg-accent hover:text-accent-foreground"
            onClick={() => { onModelChange?.(null); setPickerOpen(false); }}
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
