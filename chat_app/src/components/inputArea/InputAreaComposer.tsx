import { useI18n } from '../../i18n/I18nProvider';
import { cn } from '../../lib/utils';
import { InputAreaSendButton } from './InlineWidgets';
import { InputAreaComposerControls } from './InputAreaComposerControls';
import type { InputAreaComposerProps } from './InputAreaComposerTypes';

export default function InputAreaComposer(props: InputAreaComposerProps) {
  const { t } = useI18n();
  const {
  disabled,
  isStreaming,
  isStopping,
  isGuidingMode,
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
  onStop,
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
        ref={textareaRef}
        value={message}
        onChange={handleInputChange}
        onKeyDown={handleKeyDown}
        onPaste={handlePaste}
        placeholder={isGuidingMode ? t('inputArea.composer.guidingPlaceholder') : placeholder}
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

      {isGuidingMode && (
        <button
          onClick={() => {
            if (onStop && !isStopping) {
              onStop();
            }
          }}
          disabled={isStopping || disabled}
          className={cn(
            'flex-shrink-0 p-2 rounded-md transition-colors',
            isStopping
              ? 'bg-amber-500 text-white'
              : 'bg-red-500 text-white hover:bg-red-600',
            'disabled:opacity-50 disabled:cursor-not-allowed',
          )}
          title={isStopping ? t('inputArea.send.stopping') : t('inputArea.send.stop')}
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
      )}

      <InputAreaSendButton
        isStreaming={!isGuidingMode && isStreaming}
        isStopping={isStopping}
        onStop={onStop}
        onSend={handleSend}
        disabled={disabled || isStopping}
        canSend={canSend}
        showModelSelector={showModelSelector}
        selectedModelId={selectedModelId}
      />

      {effectiveAllowAttachments && (
        <input
          ref={fileInputRef}
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
