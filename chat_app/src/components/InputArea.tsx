import React, { useState, useRef, useCallback, useEffect, useMemo } from 'react';
import { apiClient as globalApiClient } from '../lib/api/client';
import { useChatApiClientFromContext } from '../lib/store/ChatStoreContext';
import { cn } from '../lib/utils';
import type { InputAreaProps } from '../types';
import {
  formatFileSize,
  MAX_ATTACHMENTS,
  MAX_FILE_BYTES,
  MAX_TOTAL_BYTES,
} from './inputArea/fileUtils';
import { useMcpSelection } from './inputArea/useMcpSelection';
import { useDismissiblePopover } from './inputArea/useDismissiblePopover';
import { useWorkspaceDirectoryPicker } from './inputArea/useWorkspaceDirectoryPicker';
import { useProjectFilePicker } from './inputArea/useProjectFilePicker';
import { useAttachmentsInput } from './inputArea/useAttachmentsInput';
import {
  InputAreaAttachmentsPreview,
  InputAreaErrorBanners,
  InputAreaFloatingModelPicker,
  InputAreaSendButton,
} from './inputArea/InlineWidgets';
import {
  InputAreaMcpPicker,
  InputAreaProjectFilePicker,
  InputAreaProjectSelector,
  InputAreaWorkspacePicker,
} from './inputArea/PickerWidgets';

export const InputArea: React.FC<InputAreaProps> = ({
  onSend,
  onGuide,
  onStop,
  disabled = false,
  isStreaming = false,
  isStopping = false,
  placeholder = 'Type your message...',
  maxLength = 4000,
  allowAttachments = false,
  supportedFileTypes = [
    'image/*',
    'text/*',
    'application/json',
    'application/pdf',
    'application/vnd.openxmlformats-officedocument.wordprocessingml.document'
  ],
  reasoningSupported = false,
  reasoningEnabled = false,
  onReasoningToggle,
  showModelSelector = false,
  selectedModelId = null,
  availableModels = [],
  onModelChange,
  availableProjects = [],
  selectedProjectId = null,
  onProjectChange,
  showProjectSelector = true,
  showProjectFileButton = true,
  workspaceRoot = null,
  onWorkspaceRootChange,
  showWorkspaceRootPicker = false,
  mcpEnabled = true,
  enabledMcpIds = [],
  onMcpEnabledChange,
  onEnabledMcpIdsChange,
}) => {
  const isGuidingMode = isStreaming && !isStopping;
  const effectiveAllowAttachments = allowAttachments && !isGuidingMode;
  const [message, setMessage] = useState('');
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const [pickerOpen, setPickerOpen] = useState(false);
  const {
    attachments,
    attachError,
    isDragging,
    addFiles,
    handlePaste,
    handleFileSelect,
    removeAttachment,
    handleDragOver,
    handleDragLeave,
    handleDrop,
    clearAttachments,
  } = useAttachmentsInput({
    allowAttachments: effectiveAllowAttachments,
    disabled,
    supportedFileTypes,
    fileInputRef,
  });

  const apiClientFromContext = useChatApiClientFromContext();
  const client = useMemo(() => apiClientFromContext || globalApiClient, [apiClientFromContext]);

  const normalizePath = useCallback((value: string) => {
    const normalized = value.replace(/\\/g, '/').replace(/\/+/g, '/');
    if (normalized.length > 1 && normalized.endsWith('/')) {
      return normalized.slice(0, -1);
    }
    return normalized;
  }, []);

  const selectedRuntimeProject = useMemo(() => {
    if (!selectedProjectId) {
      return null;
    }
    return (availableProjects || []).find((p: any) => p.id === selectedProjectId) || null;
  }, [availableProjects, selectedProjectId]);
  const normalizedWorkspaceRoot = useMemo(() => {
    const raw = typeof workspaceRoot === 'string' ? workspaceRoot.trim() : '';
    return raw ? normalizePath(raw) : null;
  }, [normalizePath, workspaceRoot]);
  const {
    workspacePickerOpen,
    setWorkspacePickerOpen,
    workspacePath,
    workspaceParent,
    workspaceEntries,
    workspaceRoots,
    workspaceLoading,
    workspaceError,
    loadWorkspaceDirectories,
    handleToggleWorkspacePicker,
    handleSelectWorkspaceRoot,
  } = useWorkspaceDirectoryPicker({
    client,
    showWorkspaceRootPicker,
    disabled,
    isStreaming,
    isStopping,
    normalizedWorkspaceRoot,
    onWorkspaceRootChange,
  });
  const hasRuntimeProject = Boolean(selectedRuntimeProject?.id && selectedRuntimeProject?.rootPath);
  const hasDirectoryContext = hasRuntimeProject || Boolean(normalizedWorkspaceRoot);
  const {
    mcpPickerOpen,
    setMcpPickerOpen,
    availableMcpConfigs,
    mcpConfigsLoading,
    mcpConfigsError,
    builtinMcpConfigs,
    customMcpConfigs,
    selectableMcpIds,
    sanitizedEnabledMcpIds,
    isAllMcpSelected,
    selectedMcpCount,
    isProjectRequiredMcpId,
    loadAvailableMcpConfigs,
    handleToggleMcpPicker,
    handleSelectAllMcp,
    handleToggleMcpSelection,
  } = useMcpSelection({
    client,
    mcpEnabled,
    enabledMcpIds,
    hasDirectoryContext,
    disabled,
    isStreaming,
    isStopping,
    onMcpEnabledChange,
    onEnabledMcpIdsChange,
  });
  const pickerRef = useDismissiblePopover<HTMLDivElement>(pickerOpen, () => setPickerOpen(false));
  const mcpPickerRef = useDismissiblePopover<HTMLDivElement>(mcpPickerOpen, () => setMcpPickerOpen(false));
  const workspacePickerRef = useDismissiblePopover<HTMLDivElement>(workspacePickerOpen, () => setWorkspacePickerOpen(false));
  const selectedModel = useMemo(
    () => (selectedModelId ? (availableModels || []).find(m => (m as any).id === selectedModelId) : null),
    [availableModels, selectedModelId]
  );
  const enabledModels = useMemo(
    () => (availableModels || []).filter((m: any) => m.enabled),
    [availableModels]
  );
  const hasAiOptions = (availableModels && availableModels.length > 0);
  const projectForFilePicker = useMemo(
    () => selectedRuntimeProject || null,
    [selectedRuntimeProject],
  );
  const projectRootForFilePicker = useMemo(() => {
    if (!projectForFilePicker?.rootPath) return null;
    return normalizePath(projectForFilePicker.rootPath);
  }, [projectForFilePicker?.rootPath, normalizePath]);
  const showProjectFilePicker = !isGuidingMode && showProjectFileButton && Boolean(projectRootForFilePicker);
  const workspaceRootDisplayName = useMemo(() => {
    if (!normalizedWorkspaceRoot) {
      return '未选择';
    }
    const normalized = normalizePath(normalizedWorkspaceRoot);
    const segments = normalized.split('/').filter((segment) => segment.length > 0);
    if (segments.length === 0) {
      return normalized;
    }
    return segments[segments.length - 1] || normalized;
  }, [normalizePath, normalizedWorkspaceRoot]);
  const currentAiLabel = useMemo(
    () => (selectedModel ? `Model: ${(selectedModel as any).name}` : '选择模型'),
    [selectedModel]
  );
  // 自动调整文本框高度
  const adjustTextareaHeight = useCallback(() => {
    const textarea = textareaRef.current;
    if (textarea) {
      textarea.style.height = 'auto';
      const scrollHeight = textarea.scrollHeight;
      const maxHeight = 200; // 最大高度
      textarea.style.height = `${Math.min(scrollHeight, maxHeight)}px`;
    }
  }, []);
  const {
    projectFilePickerOpen,
    setProjectFilePickerOpen,
    projectFileParent,
    projectFileFilter,
    setProjectFileFilter,
    projectFileSearchTruncated,
    projectFileError,
    projectFileAttachingPath,
    projectFilePathLabel,
    projectFileKeywordActive,
    displayedProjectFileEntries,
    projectFileBusy,
    loadProjectFileEntries,
    handleToggleProjectFilePicker,
    handleAttachProjectFile,
    toRelativeProjectPath,
  } = useProjectFilePicker({
    client,
    showProjectFilePicker,
    disabled,
    projectRootForFilePicker,
    addFiles,
  });
  const projectFilePickerRef = useDismissiblePopover<HTMLDivElement>(projectFilePickerOpen, () => setProjectFilePickerOpen(false));

  useEffect(() => {
    if (!isGuidingMode || attachments.length === 0) {
      return;
    }
    clearAttachments();
  }, [attachments.length, clearAttachments, isGuidingMode]);

  // 处理输入变化
  const handleInputChange = (e: React.ChangeEvent<HTMLTextAreaElement>) => {
    const value = e.target.value;
    if (value.length <= maxLength) {
      setMessage(value);
      adjustTextareaHeight();
    }
  };

  // 处理键盘事件
  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  };

  // 发送消息
  const handleSend = () => {
    const trimmedMessage = message.trim();
    if (!trimmedMessage && (!effectiveAllowAttachments || attachments.length === 0)) return;
    if (disabled) return;

    if (isGuidingMode) {
      if (!trimmedMessage) {
        return;
      }
      onGuide?.(trimmedMessage);
      setMessage('');
      clearAttachments();
      if (textareaRef.current) {
        textareaRef.current.style.height = 'auto';
      }
      return;
    }

    // 检查是否选择了模型
    if (showModelSelector && !selectedModelId) {
      alert('请先选择一个模型');
      return;
    }

    const runtimeProjectId = selectedRuntimeProject?.id?.trim() || '0';
    const runtimeProjectRoot = runtimeProjectId === '0'
      ? null
      : (selectedRuntimeProject?.rootPath || null);
    const runtimeWorkspaceRoot = normalizedWorkspaceRoot || null;

    onSend(trimmedMessage, attachments, {
      mcpEnabled,
      enabledMcpIds: sanitizedEnabledMcpIds,
      projectId: runtimeProjectId,
      projectRoot: runtimeProjectRoot,
      workspaceRoot: runtimeWorkspaceRoot,
    });
    setMessage('');
    clearAttachments();
    
    // 重置文本框高度
    if (textareaRef.current) {
      textareaRef.current.style.height = 'auto';
    }
  };

  return (
    <div className="border-t bg-background p-3 sm:p-4">
      <InputAreaAttachmentsPreview
        attachments={attachments}
        onRemoveAttachment={removeAttachment}
      />
      <InputAreaErrorBanners
        attachError={attachError}
        projectFileError={projectFileError}
        workspaceError={workspaceError}
      />

      {/* 输入区域 */}
      <div
        className={cn(
          'relative flex items-end gap-3 p-3 border rounded-lg transition-colors',
          'focus-within:border-primary',
          isDragging && 'border-primary bg-primary/5',
          disabled && 'opacity-50 cursor-not-allowed'
        )}
        onDragOver={handleDragOver}
        onDragLeave={handleDragLeave}
        onDrop={handleDrop}
      >
        <InputAreaFloatingModelPicker
          showModelSelector={showModelSelector}
          hasAiOptions={hasAiOptions}
          pickerRef={pickerRef}
          disabled={disabled}
          currentAiLabel={currentAiLabel}
          pickerOpen={pickerOpen}
          setPickerOpen={setPickerOpen}
          enabledModels={enabledModels}
          selectedModelId={selectedModelId}
          onModelChange={onModelChange}
        />
        {/* 附件按钮 */}
        {effectiveAllowAttachments && (
          <button
            onClick={() => fileInputRef.current?.click()}
            disabled={disabled}
            className="flex-shrink-0 p-2 text-muted-foreground hover:text-foreground transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
            title="Attach files"
          >
            <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15.172 7l-6.586 6.586a2 2 0 102.828 2.828l6.414-6.586a4 4 0 00-5.656-5.656l-6.415 6.585a6 6 0 108.486 8.486L20.5 13" />
            </svg>
          </button>
        )}

        <InputAreaProjectFilePicker
          allowAttachments={effectiveAllowAttachments}
          showProjectFilePicker={showProjectFilePicker}
          pickerRef={projectFilePickerRef}
          disabled={disabled}
          projectFileAttachingPath={projectFileAttachingPath}
          projectFilePickerOpen={projectFilePickerOpen}
          onTogglePicker={() => { void handleToggleProjectFilePicker(); }}
          projectName={projectForFilePicker?.name || '当前项目'}
          projectFilePathLabel={projectFilePathLabel}
          projectFileFilter={projectFileFilter}
          onProjectFileFilterChange={setProjectFileFilter}
          projectFileBusy={projectFileBusy}
          projectFileKeywordActive={projectFileKeywordActive}
          projectFileParent={projectFileParent}
          onLoadProjectFileEntries={(path) => { void loadProjectFileEntries(path); }}
          displayedProjectFileEntries={displayedProjectFileEntries}
          onAttachProjectFile={(entry) => { void handleAttachProjectFile(entry); }}
          toRelativeProjectPath={toRelativeProjectPath}
          projectFileSearchTruncated={projectFileSearchTruncated}
        />

        <InputAreaProjectSelector
          showProjectSelector={showProjectSelector}
          availableProjects={availableProjects}
          selectedProjectId={selectedProjectId}
          onProjectChange={onProjectChange}
          disabled={disabled}
          isStreaming={isStreaming}
          isStopping={isStopping}
        />

        <InputAreaWorkspacePicker
          showWorkspaceRootPicker={showWorkspaceRootPicker}
          workspacePickerRef={workspacePickerRef}
          disabled={disabled}
          isStreaming={isStreaming}
          isStopping={isStopping}
          onToggleWorkspacePicker={() => { void handleToggleWorkspacePicker(); }}
          normalizedWorkspaceRoot={normalizedWorkspaceRoot}
          workspaceRootDisplayName={workspaceRootDisplayName}
          workspacePickerOpen={workspacePickerOpen}
          workspacePath={workspacePath}
          workspaceParent={workspaceParent}
          workspaceLoading={workspaceLoading}
          workspaceEntries={workspaceEntries}
          workspaceRoots={workspaceRoots}
          onLoadWorkspaceDirectories={(path) => { void loadWorkspaceDirectories(path); }}
          onSelectWorkspaceRoot={handleSelectWorkspaceRoot}
        />

        <InputAreaMcpPicker
          mcpPickerRef={mcpPickerRef}
          mcpEnabled={mcpEnabled}
          onMcpEnabledChange={onMcpEnabledChange}
          disabled={disabled}
          isStreaming={isStreaming}
          isStopping={isStopping}
          onToggleMcpPicker={handleToggleMcpPicker}
          mcpPickerOpen={mcpPickerOpen}
          isAllMcpSelected={isAllMcpSelected}
          selectableMcpIds={selectableMcpIds}
          selectedMcpCount={selectedMcpCount}
          mcpConfigsLoading={mcpConfigsLoading}
          mcpConfigsError={mcpConfigsError}
          availableMcpConfigs={availableMcpConfigs}
          builtinMcpConfigs={builtinMcpConfigs}
          customMcpConfigs={customMcpConfigs}
          hasDirectoryContext={hasDirectoryContext}
          isProjectRequiredMcpId={isProjectRequiredMcpId}
          sanitizedEnabledMcpIds={sanitizedEnabledMcpIds}
          onRefreshMcpConfigs={() => { void loadAvailableMcpConfigs(); }}
          onSelectAllMcp={handleSelectAllMcp}
          onToggleMcpSelection={handleToggleMcpSelection}
        />

        {reasoningSupported && (
          <button
            type="button"
            onClick={() => onReasoningToggle?.(!reasoningEnabled)}
            disabled={disabled || isStreaming || isStopping}
            className={cn(
              'flex-shrink-0 px-2 py-1 text-xs rounded-md transition-colors',
              reasoningEnabled
                ? 'bg-primary text-primary-foreground hover:bg-primary/90'
                : 'bg-muted text-muted-foreground hover:text-foreground',
              (disabled || isStreaming || isStopping) && 'opacity-50 cursor-not-allowed'
            )}
            title={reasoningEnabled ? '推理已开启' : '推理已关闭'}
          >
            推理 {reasoningEnabled ? '开' : '关'}
          </button>
        )}

        {/* 移除行内选择器，使用右上角浮动标签 */}

        {/* 文本输入 */}
        <textarea
          ref={textareaRef}
          value={message}
          onChange={handleInputChange}
          onKeyDown={handleKeyDown}
          onPaste={handlePaste}
          placeholder={isGuidingMode ? '执行中，可发送引导（不会打断当前执行）...' : placeholder}
          disabled={disabled}
          className={cn(
            'flex-1 resize-none bg-transparent border-none outline-none',
            'placeholder:text-muted-foreground',
            'disabled:cursor-not-allowed'
          )}
          rows={1}
          style={{ minHeight: '24px', maxHeight: '200px' }}
        />

        {/* 右侧不再放选择器，避免靠近发送按钮 */}

        {/* 字符计数 */}
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
        )}

        <InputAreaSendButton
          isStreaming={!isGuidingMode && isStreaming}
          isStopping={isStopping}
          onStop={onStop}
          onSend={handleSend}
          disabled={disabled || isStopping}
          canSend={Boolean(message.trim() || (!isGuidingMode && attachments.length > 0))}
          showModelSelector={showModelSelector}
          selectedModelId={selectedModelId}
        />

        {/* 隐藏的文件输入 */}
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

      {/* 拖拽提示 */}
      {isDragging && effectiveAllowAttachments && (
        <div className="absolute inset-0 bg-primary/10 border-2 border-dashed border-primary rounded-lg flex items-center justify-center">
          <div className="text-center">
            <svg className="w-8 h-8 mx-auto text-primary mb-2" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M7 16a4 4 0 01-.88-7.903A5 5 0 1115.9 6L16 6a5 5 0 011 9.9M15 13l-3-3m0 0l-3 3m3-3v12" />
            </svg>
            <p className="text-sm font-medium text-primary">Drop files here to attach</p>
            <p className="text-[11px] text-muted-foreground mt-1">单文件≤{formatFileSize(MAX_FILE_BYTES)}，总计≤{formatFileSize(MAX_TOTAL_BYTES)}，最多 {MAX_ATTACHMENTS} 个</p>
          </div>
        </div>
      )}
    </div>
  );
};

export default InputArea;
