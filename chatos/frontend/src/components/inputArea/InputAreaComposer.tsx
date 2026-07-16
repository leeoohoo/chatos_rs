// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { Ref } from 'react';

import { cn } from '../../lib/utils';
import { InputAreaSendButton, InputAreaStopButton } from './InlineWidgets';
import { InputAreaComposerControls } from './InputAreaComposerControls';
import type { InputAreaComposerProps } from './InputAreaComposerTypes';

export default function InputAreaComposer(props: InputAreaComposerProps) {
  const {
  disabled,
  onStop,
  isRunning,
  isStopping,
  effectiveAllowAttachments,
  showModelSelector,
  selectedModelId,
  placeholder,
  maxLength,
  supportedFileTypes,
  isDragging,
  fileInputRef,
  textareaRef,
  message,
  handleInputChange,
  handleKeyDown,
  handlePaste,
  handleSend,
  canSend,
  handleDragOver,
  handleDragLeave,
  handleDrop,
  handleFileSelect,
  } = props;

  return (
    <div
      className={cn(
        'relative flex items-end gap-3 p-3 border rounded-lg transition-colors',
        'focus-within:border-primary',
        isDragging && 'border-primary bg-primary/5',
        disabled && 'opacity-50 cursor-not-allowed',
      )}
      onDragOver={handleDragOver}
      onDragLeave={handleDragLeave}
      onDrop={handleDrop}
    >
      <InputAreaComposerControls {...props} />

      <textarea
        ref={textareaRef as Ref<HTMLTextAreaElement>}
        value={message}
        onChange={handleInputChange}
        onKeyDown={handleKeyDown}
        onPaste={handlePaste}
        placeholder={placeholder}
        disabled={disabled}
        className={cn(
          'flex-1 resize-none bg-transparent border-none outline-none',
          'placeholder:text-muted-foreground',
          'disabled:cursor-not-allowed',
        )}
        rows={1}
        style={{ minHeight: '24px', maxHeight: '200px' }}
      />

      <div className="flex-shrink-0 text-[11px] sm:text-xs text-muted-foreground tabular-nums">
        {message.length}/{maxLength}
      </div>

      {isRunning && onStop && (
        <InputAreaStopButton
          onStop={onStop}
          disabled={isStopping}
        />
      )}
      <InputAreaSendButton
        onSend={handleSend}
        disabled={disabled}
        canSend={canSend}
        showModelSelector={showModelSelector}
        selectedModelId={selectedModelId}
      />

      {effectiveAllowAttachments && (
        <input
          ref={fileInputRef as Ref<HTMLInputElement>}
          type="file"
          multiple
          accept={supportedFileTypes.join(',')}
          onChange={handleFileSelect}
          className="hidden"
        />
      )}
    </div>
  );
}
