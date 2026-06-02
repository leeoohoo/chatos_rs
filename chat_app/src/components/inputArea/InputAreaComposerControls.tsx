import { cn } from '../../lib/utils';
import { InputAreaFloatingModelPicker } from './InlineWidgets';
import type { InputAreaComposerProps } from './InputAreaComposerTypes';
import {
  InputAreaMcpPicker,
  InputAreaProjectFilePicker,
  InputAreaProjectSelector,
  InputAreaRemoteConnectionPicker,
  InputAreaSkillPicker,
  InputAreaWorkspacePicker,
} from './PickerWidgets';

export function InputAreaComposerControls({
  disabled,
  isStreaming,
  isStopping,
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
  autoCreateTask,
  onMcpEnabledChange,
  onAutoCreateTaskChange,
  reasoningSupported,
  reasoningEnabled,
  onReasoningToggle,
  pickerRef,
  mcpPickerRef,
  workspacePickerRef,
  projectFilePickerRef,
  fileInputRef,
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
}: InputAreaComposerProps) {
  return (
    <>
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

      <InputAreaSkillPicker
        currentAgent={currentAgent}
        disabled={disabled}
        isStreaming={isStreaming}
        isStopping={isStopping}
        skillsEnabled={skillsEnabled}
        onSkillsEnabledChange={onSkillsEnabledChange}
        skillsLoading={skillsLoading}
        availableSkillOptions={availableSkillOptions}
        selectedSkillIds={selectedSkillIds}
        onToggleSelectedSkill={onToggleSelectedSkill}
        onClearSelectedSkills={onClearSelectedSkills}
      />

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
        onRefreshMcpConfigs={() => { void loadAvailableMcpConfigs({ forceRefresh: true }); }}
        onSelectAllMcp={handleSelectAllMcp}
        onToggleMcpSelection={handleToggleMcpSelection}
        onApplyMcpToolsetPreset={handleApplyMcpToolsetPreset}
        onSaveProjectMcpDefault={handleSaveProjectMcpDefault}
        onApplyProjectMcpDefault={handleApplyProjectMcpDefault}
      />

      <button
        type="button"
        onClick={() => onAutoCreateTaskChange?.(!autoCreateTask)}
        disabled={disabled || isStreaming || isStopping}
        className={cn(
          'flex-shrink-0 px-2 py-1 text-xs rounded-md transition-colors',
          autoCreateTask
            ? 'bg-emerald-600 text-white hover:bg-emerald-700'
            : 'bg-muted text-muted-foreground hover:text-foreground',
          (disabled || isStreaming || isStopping) && 'opacity-50 cursor-not-allowed',
        )}
        title={autoCreateTask ? '自动建任务已开启' : '自动建任务已关闭'}
      >
        任务自动建 {autoCreateTask ? '开' : '关'}
      </button>

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
    </>
  );
}
