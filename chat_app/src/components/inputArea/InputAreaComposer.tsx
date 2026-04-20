import type {
  ChangeEvent,
  ClipboardEvent,
  Dispatch,
  DragEvent,
  KeyboardEvent,
  MouseEvent,
  RefObject,
  SetStateAction,
} from 'react';
import { useEffect, useRef, useState } from 'react';

import { cn } from '../../lib/utils';
import type {
  AiModelConfig,
  AgentConfig,
  FsEntry,
  Project,
  RemoteConnection,
} from '../../types';
import {
  InputAreaFloatingModelPicker,
  InputAreaSendButton,
} from './InlineWidgets';
import type { McpToolsetPreset, SelectableMcpConfig } from './useMcpSelection';
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
  currentAgent: AgentConfig | null;
  availableRemoteConnections: RemoteConnection[];
  onRemoteConnectionChange?: (connectionId: string | null) => void;
  mcpEnabled: boolean;
  onMcpEnabledChange?: (enabled: boolean) => void;
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
  mcpToolsetPresets: McpToolsetPreset[];
  projectScopeKey: string | null;
  hasProjectMcpDefault: boolean;
  hasDirectoryContext: boolean;
  hasRemoteContext: boolean;
  isProjectRequiredMcpId: (mcpId: string) => boolean;
  isRemoteRequiredMcpId: (mcpId: string) => boolean;
  sanitizedEnabledMcpIds: string[];
  loadAvailableMcpConfigs: () => void | Promise<void>;
  handleSelectAllMcp: () => void;
  handleToggleMcpSelection: (mcpId: string) => void;
  handleApplyMcpToolsetPreset: (presetId: string) => void;
  handleSaveProjectMcpDefault: () => void;
  handleApplyProjectMcpDefault: () => void;
  skillsEnabled: boolean;
  onSkillsEnabledChange: (enabled: boolean) => void;
  skillsLoading: boolean;
  availableSkillOptions: Array<{ id: string; name: string; description?: string | null }>;
  selectedSkillIds: string[];
  onToggleSelectedSkill: (skillId: string) => void;
  onClearSelectedSkills: () => void;
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
  currentAgent,
  availableRemoteConnections,
  onRemoteConnectionChange,
  mcpEnabled,
  onMcpEnabledChange,
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
  mcpToolsetPresets,
  projectScopeKey,
  hasProjectMcpDefault,
  hasDirectoryContext,
  hasRemoteContext,
  isProjectRequiredMcpId,
  isRemoteRequiredMcpId,
  sanitizedEnabledMcpIds,
  loadAvailableMcpConfigs,
  handleSelectAllMcp,
  handleToggleMcpSelection,
  handleApplyMcpToolsetPreset,
  handleSaveProjectMcpDefault,
  handleApplyProjectMcpDefault,
  skillsEnabled,
  onSkillsEnabledChange,
  skillsLoading,
  availableSkillOptions,
  selectedSkillIds,
  onToggleSelectedSkill,
  onClearSelectedSkills,
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
  const selectedSkillCount = selectedSkillIds.length;
  const skillsToggleDisabled = disabled || isStreaming || isStopping;
  const [skillPickerOpen, setSkillPickerOpen] = useState(false);
  const skillPickerRef = useRef<HTMLDivElement | null>(null);
  const handleSkillsButtonClick = (event: MouseEvent<HTMLButtonElement>) => {
    event.preventDefault();
    if (skillsToggleDisabled) {
      return;
    }
    const nextEnabled = !skillsEnabled;
    onSkillsEnabledChange(nextEnabled);
    setSkillPickerOpen(nextEnabled);
  };
  const handleSkillPickerClick = (event: MouseEvent<HTMLButtonElement>) => {
    event.preventDefault();
    if (disabled || isStreaming || isStopping) {
      return;
    }
    setSkillPickerOpen((prev) => !prev);
  };

  useEffect(() => {
    if (!skillsEnabled) {
      setSkillPickerOpen(false);
    }
  }, [skillsEnabled]);

  useEffect(() => {
    if (!skillPickerOpen) {
      return undefined;
    }
    const handleDocumentClick = (event: globalThis.MouseEvent) => {
      const target = event.target as Node;
      if (skillPickerRef.current && !skillPickerRef.current.contains(target)) {
        setSkillPickerOpen(false);
      }
    };
    document.addEventListener('mousedown', handleDocumentClick);
    return () => {
      document.removeEventListener('mousedown', handleDocumentClick);
    };
  }, [skillPickerOpen]);

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

      <div className="flex items-center gap-2">
        <button
          type="button"
          onClick={handleSkillsButtonClick}
          disabled={skillsToggleDisabled}
          className={cn(
            'flex-shrink-0 px-2 py-1 text-xs rounded-md transition-colors',
            skillsEnabled && !skillsToggleDisabled
              ? 'bg-emerald-600 text-white hover:bg-emerald-700'
              : 'bg-muted text-muted-foreground hover:text-foreground',
            skillsToggleDisabled && 'opacity-50 cursor-not-allowed',
          )}
          title={
            !currentAgent
              ? '启用技能上下文；发送时会按当前会话智能体解析'
              : (skillsEnabled ? '已启用技能上下文' : '未启用技能上下文')
          }
        >
          技能 {skillsEnabled && !skillsToggleDisabled ? '开' : '关'}
        </button>

        {skillsEnabled && (
          <div className="relative" ref={skillPickerRef}>
            <button
              type="button"
              onClick={handleSkillPickerClick}
              disabled={disabled || isStreaming || isStopping}
              className={cn(
                'flex-shrink-0 px-2 py-1 text-xs rounded-md border transition-colors',
                selectedSkillCount > 0
                  ? 'border-emerald-600 bg-emerald-50 text-emerald-700'
                  : 'border-border bg-background text-muted-foreground hover:text-foreground',
                (disabled || isStreaming || isStopping) && 'opacity-50 cursor-not-allowed',
              )}
              title="选择本轮要直接注入全文的技能；不选则使用技能优先概览模式"
            >
              {selectedSkillCount > 0 ? `已选 ${selectedSkillCount}` : '技能选择'}
            </button>

            {skillPickerOpen && (
              <div className="absolute bottom-full left-0 z-50 mb-2 w-72 max-h-80 overflow-y-auto rounded-lg border border-border bg-popover p-2 shadow-lg">
                <div className="px-2 pb-2 text-xs text-muted-foreground">
                  选择具体技能会把技能全文直接带入上下文；不选择则使用技能优先概览模式。
                </div>
                {availableSkillOptions.length > 0 ? (
                  <div className="space-y-1">
                    {availableSkillOptions.map((skill) => {
                      const selected = selectedSkillIds.includes(skill.id);
                      return (
                        <button
                          key={skill.id}
                          type="button"
                          onClick={(event) => {
                            event.preventDefault();
                            onToggleSelectedSkill(skill.id);
                          }}
                          disabled={disabled || isStreaming || isStopping}
                          className={cn(
                            'w-full rounded-md border px-2 py-1.5 text-left text-xs transition-colors',
                            selected
                              ? 'border-emerald-600 bg-emerald-50 text-emerald-700'
                              : 'border-transparent hover:border-border hover:bg-accent',
                            (disabled || isStreaming || isStopping) && 'opacity-50 cursor-not-allowed',
                          )}
                          title={skill.description || skill.name}
                        >
                          <span className="flex items-center gap-2">
                            <span
                              className={cn(
                                'flex h-3.5 w-3.5 items-center justify-center rounded-sm border text-[10px]',
                                selected
                                  ? 'border-emerald-600 bg-emerald-600 text-white'
                                  : 'border-border bg-background',
                              )}
                            >
                              {selected ? 'x' : ''}
                            </span>
                            <span className="min-w-0 flex-1 truncate">{skill.name}</span>
                          </span>
                          {skill.description ? (
                            <span className="mt-0.5 block truncate pl-5 text-[11px] text-muted-foreground">
                              {skill.description}
                            </span>
                          ) : null}
                        </button>
                      );
                    })}
                    {selectedSkillCount > 0 && (
                      <button
                        type="button"
                        onClick={(event) => {
                          event.preventDefault();
                          onClearSelectedSkills();
                        }}
                        disabled={disabled || isStreaming || isStopping}
                        className="mt-2 w-full rounded-md border border-dashed border-border px-2 py-1.5 text-xs text-muted-foreground hover:text-foreground"
                        title="清空已选技能，保留技能优先模式"
                      >
                        清空已选技能
                      </button>
                    )}
                  </div>
                ) : (
                  <div className="rounded-md border border-dashed border-border px-2 py-3 text-xs text-muted-foreground">
                    {skillsLoading
                      ? '正在加载当前智能体的技能...'
                      : '当前智能体暂无可选择技能。不选择具体技能时，发送会使用技能优先概览模式。'}
                  </div>
                )}
              </div>
            )}
          </div>
        )}
      </div>

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
        mcpToolsetPresets={mcpToolsetPresets}
        projectScopeKey={projectScopeKey}
        hasProjectMcpDefault={hasProjectMcpDefault}
        hasDirectoryContext={hasDirectoryContext}
        hasRemoteContext={hasRemoteContext}
        isProjectRequiredMcpId={isProjectRequiredMcpId}
        isRemoteRequiredMcpId={isRemoteRequiredMcpId}
        sanitizedEnabledMcpIds={sanitizedEnabledMcpIds}
        onRefreshMcpConfigs={() => { void loadAvailableMcpConfigs(); }}
        onSelectAllMcp={handleSelectAllMcp}
        onToggleMcpSelection={handleToggleMcpSelection}
        onApplyMcpToolsetPreset={handleApplyMcpToolsetPreset}
        onSaveProjectMcpDefault={handleSaveProjectMcpDefault}
        onApplyProjectMcpDefault={handleApplyProjectMcpDefault}
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
