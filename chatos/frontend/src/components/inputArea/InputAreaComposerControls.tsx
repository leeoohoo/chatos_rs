// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useI18n } from '../../i18n/I18nProvider';
import { cn } from '../../lib/utils';
import { InputAreaFloatingModelPicker } from './InlineWidgets';
import type { InputAreaComposerProps } from './InputAreaComposerTypes';
import {
  InputAreaProjectFilePicker,
  InputAreaProjectSelector,
  InputAreaRemoteConnectionPicker,
  InputAreaWorkspacePicker,
} from './PickerWidgets';

export function InputAreaComposerControls({
  disabled,
  effectiveAllowAttachments,
  showModelSelector,
  selectedModelId,
  selectedModelName,
  selectedThinkingLevel,
  onModelChange,
  onModelNameChange,
  onThinkingLevelChange,
  onModelRuntimeChange,
  availableProjects,
  selectedProjectId,
  onProjectChange,
  showProjectSelector,
  showWorkspaceRootPicker,
  currentRemoteConnectionId,
  availableRemoteConnections,
  onRemoteConnectionChange,
  reasoningSupported,
  reasoningEnabled,
  onReasoningToggle,
  planModeAvailable,
  planModeEnabled,
  onPlanModeToggle,
  pickerRef,
  workspacePickerRef,
  projectFilePickerRef,
  fileInputRef,
  setPickerOpen,
  pickerOpen,
  hasAiOptions,
  currentAiLabel,
  effectiveModelName,
  effectiveThinkingLevel,
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
}: InputAreaComposerProps) {
  const { t } = useI18n();
  const onText = t('composer.toggle.on');
  const offText = t('composer.toggle.off');

  return (
    <>
      <InputAreaFloatingModelPicker
        showModelSelector={showModelSelector}
        hasAiOptions={hasAiOptions}
        pickerRef={pickerRef}
        disabled={disabled}
        currentAiLabel={currentAiLabel}
        effectiveModelName={effectiveModelName}
        effectiveThinkingLevel={effectiveThinkingLevel}
        pickerOpen={pickerOpen}
        setPickerOpen={setPickerOpen}
        enabledModels={enabledModels}
        selectedModelId={selectedModelId}
        selectedModelName={selectedModelName}
        selectedThinkingLevel={selectedThinkingLevel}
        onModelChange={onModelChange}
        onModelNameChange={onModelNameChange}
        onThinkingLevelChange={onThinkingLevelChange}
        onModelRuntimeChange={onModelRuntimeChange}
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
        projectName={projectForFilePicker?.name || t('composer.currentProject')}
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
        isStreaming={false}
        isStopping={false}
      />

      <InputAreaWorkspacePicker
        showWorkspaceRootPicker={showWorkspaceRootPicker}
        workspacePickerRef={workspacePickerRef}
        disabled={disabled}
        isStreaming={false}
        isStopping={false}
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
        isStreaming={false}
        isStopping={false}
      />

      {planModeAvailable && (
        <button
          type="button"
          onClick={() => onPlanModeToggle?.(!planModeEnabled)}
          disabled={disabled}
          className={cn(
            'flex-shrink-0 px-2 py-1 text-xs rounded-md transition-colors',
            planModeEnabled
              ? 'bg-primary text-primary-foreground hover:bg-primary/90'
              : 'bg-muted text-muted-foreground hover:text-foreground',
            disabled && 'opacity-50 cursor-not-allowed',
          )}
          title={planModeEnabled ? t('composer.planMode.onTitle') : t('composer.planMode.offTitle')}
        >
          {t('composer.planMode.label', { state: planModeEnabled ? onText : offText })}
        </button>
      )}

      {reasoningSupported && (
        <button
          type="button"
          onClick={() => onReasoningToggle?.(!reasoningEnabled)}
          disabled={disabled}
          className={cn(
            'flex-shrink-0 px-2 py-1 text-xs rounded-md transition-colors',
            reasoningEnabled
              ? 'bg-primary text-primary-foreground hover:bg-primary/90'
              : 'bg-muted text-muted-foreground hover:text-foreground',
            disabled && 'opacity-50 cursor-not-allowed',
          )}
          title={reasoningEnabled ? t('composer.reasoning.onTitle') : t('composer.reasoning.offTitle')}
        >
          {t('composer.reasoning.label', { state: reasoningEnabled ? onText : offText })}
        </button>
      )}
    </>
  );
}
