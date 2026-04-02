import type {
  ChangeEvent,
  ClipboardEvent,
  Dispatch,
  DragEvent,
  KeyboardEvent,
  RefObject,
  SetStateAction,
} from 'react';

import { cn } from '../../lib/utils';
import type {
  AiModelConfig,
  FsEntry,
  Project,
  RemoteConnection,
} from '../../types';
import {
  InputAreaFloatingModelPicker,
  InputAreaSendButton,
} from './InlineWidgets';
import type { SelectableMcpConfig } from './useMcpSelection';
import {
  InputAreaMcpPicker,
  InputAreaProjectFilePicker,
  InputAreaProjectSelector,
  InputAreaRemoteConnectionPicker,
  InputAreaWorkspacePicker,
} from './PickerWidgets';

interface InputAreaComposerProps {
  disabled: boolean;
  isStreaming: boolean;
  isStopping: boolean;
  isGuidingMode: boolean;
  effectiveAllowAttachments: boolean;
  showModelSelector: boolean;
  selectedModelId: string | null;
  onModelChange?: (modelId: string | null) => void;
  availableProjects: Project[];
  selectedProjectId: string | null;
  onProjectChange?: (projectId: string | null) => void;
  showProjectSelector: boolean;
  showWorkspaceRootPicker: boolean;
  currentRemoteConnectionId: string | null;
  availableRemoteConnections: RemoteConnection[];
  onRemoteConnectionChange?: (connectionId: string | null) => void;
  mcpEnabled: boolean;
  onMcpEnabledChange?: (enabled: boolean) => void;
  fixedMcpProfile: boolean;
  reasoningSupported: boolean;
  reasoningEnabled: boolean;
  onReasoningToggle?: (enabled: boolean) => void;
  placeholder: string;
  maxLength: number;
  supportedFileTypes: string[];
  isDragging: boolean;
  pickerRef: RefObject<HTMLDivElement>;
  mcpPickerRef: RefObject<HTMLDivElement>;
  workspacePickerRef: RefObject<HTMLDivElement>;
  projectFilePickerRef: RefObject<HTMLDivElement>;
  fileInputRef: RefObject<HTMLInputElement>;
  textareaRef: RefObject<HTMLTextAreaElement>;
  message: string;
  setPickerOpen: Dispatch<SetStateAction<boolean>>;
  pickerOpen: boolean;
  hasAiOptions: boolean;
  currentAiLabel: string;
  enabledModels: AiModelConfig[];
  projectForFilePicker: Project | null;
  showProjectFilePicker: boolean;
  projectFileAttachingPath: string | null;
  projectFilePickerOpen: boolean;
  handleToggleProjectFilePicker: () => void | Promise<void>;
  projectFilePathLabel: string;
  projectFileFilter: string;
  setProjectFileFilter: (value: string) => void;
  projectFileBusy: boolean;
  projectFileKeywordActive: boolean;
  projectFileParent: string | null;
  loadProjectFileEntries: (path?: string | null) => void | Promise<void>;
  displayedProjectFileEntries: FsEntry[];
  handleAttachProjectFile: (entry: FsEntry) => void | Promise<void>;
  toRelativeProjectPath: (path: string) => string;
  projectFileSearchTruncated: boolean;
  normalizedWorkspaceRoot: string | null;
  workspaceRootDisplayName: string;
  workspacePickerOpen: boolean;
  workspacePath: string | null;
  workspaceParent: string | null;
  workspaceLoading: boolean;
  workspaceEntries: FsEntry[];
  workspaceRoots: FsEntry[];
  handleToggleWorkspacePicker: () => void | Promise<void>;
  loadWorkspaceDirectories: (path?: string | null) => void | Promise<void>;
  handleSelectWorkspaceRoot: (path: string | null) => void;
  mcpPickerOpen: boolean;
  handleToggleMcpPicker: () => void | Promise<void>;
  isAllMcpSelected: boolean;
  selectableMcpIds: string[];
  selectedMcpCount: number;
  mcpConfigsLoading: boolean;
  mcpConfigsError: string | null;
  availableMcpConfigs: SelectableMcpConfig[];
  builtinMcpConfigs: SelectableMcpConfig[];
  customMcpConfigs: SelectableMcpConfig[];
  hasDirectoryContext: boolean;
  hasRemoteContext: boolean;
  isProjectRequiredMcpId: (mcpId: string) => boolean;
  isRemoteRequiredMcpId: (mcpId: string) => boolean;
  sanitizedEnabledMcpIds: string[];
  loadAvailableMcpConfigs: () => void | Promise<void>;
  handleSelectAllMcp: () => void;
  handleToggleMcpSelection: (mcpId: string) => void;
  handleInputChange: (event: ChangeEvent<HTMLTextAreaElement>) => void;
  handleKeyDown: (event: KeyboardEvent<HTMLTextAreaElement>) => void;
  handlePaste: (event: ClipboardEvent<HTMLTextAreaElement>) => void;
  onStop?: () => void;
  handleSend: () => void;
  canSend: boolean;
  handleDragOver: (event: DragEvent<HTMLDivElement>) => void;
  handleDragLeave: (event: DragEvent<HTMLDivElement>) => void;
  handleDrop: (event: DragEvent<HTMLDivElement>) => void;
  handleFileSelect: (event: ChangeEvent<HTMLInputElement>) => void;
}

export default function InputAreaComposer({
  disabled,
  isStreaming,
  isStopping,
  isGuidingMode,
  effectiveAllowAttachments,
  showModelSelector,
  selectedModelId,
  onModelChange,
  availableProjects,
  selectedProjectId,
  onProjectChange,
  showProjectSelector,
  showWorkspaceRootPicker,
  currentRemoteConnectionId,
  availableRemoteConnections,
  onRemoteConnectionChange,
  mcpEnabled,
  onMcpEnabledChange,
  fixedMcpProfile,
  reasoningSupported,
  reasoningEnabled,
  onReasoningToggle,
  placeholder,
  maxLength,
  supportedFileTypes,
  isDragging,
  pickerRef,
  mcpPickerRef,
  workspacePickerRef,
  projectFilePickerRef,
  fileInputRef,
  textareaRef,
  message,
  setPickerOpen,
  pickerOpen,
  hasAiOptions,
  currentAiLabel,
  enabledModels,
  projectForFilePicker,
  showProjectFilePicker,
  projectFileAttachingPath,
  projectFilePickerOpen,
  handleToggleProjectFilePicker,
  projectFilePathLabel,
  projectFileFilter,
  setProjectFileFilter,
  projectFileBusy,
  projectFileKeywordActive,
  projectFileParent,
  loadProjectFileEntries,
  displayedProjectFileEntries,
  handleAttachProjectFile,
  toRelativeProjectPath,
  projectFileSearchTruncated,
  normalizedWorkspaceRoot,
  workspaceRootDisplayName,
  workspacePickerOpen,
  workspacePath,
  workspaceParent,
  workspaceLoading,
  workspaceEntries,
  workspaceRoots,
  handleToggleWorkspacePicker,
  loadWorkspaceDirectories,
  handleSelectWorkspaceRoot,
  mcpPickerOpen,
  handleToggleMcpPicker,
  isAllMcpSelected,
  selectableMcpIds,
  selectedMcpCount,
  mcpConfigsLoading,
  mcpConfigsError,
  availableMcpConfigs,
  builtinMcpConfigs,
  customMcpConfigs,
  hasDirectoryContext,
  hasRemoteContext,
  isProjectRequiredMcpId,
  isRemoteRequiredMcpId,
  sanitizedEnabledMcpIds,
  loadAvailableMcpConfigs,
  handleSelectAllMcp,
  handleToggleMcpSelection,
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
}: InputAreaComposerProps) {
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

      <InputAreaRemoteConnectionPicker
        availableRemoteConnections={availableRemoteConnections}
        currentRemoteConnectionId={currentRemoteConnectionId}
        onRemoteConnectionChange={onRemoteConnectionChange}
        disabled={disabled}
        isStreaming={isStreaming}
        isStopping={isStopping}
      />

      {fixedMcpProfile ? (
        <div
          className="flex-shrink-0 px-2 py-1 text-xs rounded-md bg-muted text-muted-foreground"
          title="联系人聊天固定使用：查看、任务、ui_prompter"
        >
          固定 MCP
        </div>
      ) : (
        <InputAreaMcpPicker
          mcpPickerRef={mcpPickerRef}
          mcpEnabled={mcpEnabled}
          onMcpEnabledChange={onMcpEnabledChange}
          disabled={disabled}
          isStreaming={isStreaming}
          isStopping={isStopping}
          onToggleMcpPicker={() => { void handleToggleMcpPicker(); }}
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
          hasRemoteContext={hasRemoteContext}
          isProjectRequiredMcpId={isProjectRequiredMcpId}
          isRemoteRequiredMcpId={isRemoteRequiredMcpId}
          sanitizedEnabledMcpIds={sanitizedEnabledMcpIds}
          onRefreshMcpConfigs={() => { void loadAvailableMcpConfigs(); }}
          onSelectAllMcp={handleSelectAllMcp}
          onToggleMcpSelection={handleToggleMcpSelection}
        />
      )}

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
            (disabled || isStreaming || isStopping) && 'opacity-50 cursor-not-allowed',
          )}
          title={reasoningEnabled ? '推理已开启' : '推理已关闭'}
        >
          推理 {reasoningEnabled ? '开' : '关'}
        </button>
      )}

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
