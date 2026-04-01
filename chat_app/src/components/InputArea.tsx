import React from 'react';

import type { InputAreaProps } from '../types';
import {
  MAX_ATTACHMENTS,
  MAX_FILE_BYTES,
  MAX_TOTAL_BYTES,
} from './inputArea/fileUtils';
import {
  InputAreaAttachmentsPreview,
  InputAreaErrorBanners,
} from './inputArea/InlineWidgets';
import InputAreaComposer from './inputArea/InputAreaComposer';
import InputAreaDragOverlay from './inputArea/InputAreaDragOverlay';
import { useInputAreaController } from './inputArea/useInputAreaController';

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
    'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
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
  currentRemoteConnectionId = null,
  availableRemoteConnections = [],
  onRemoteConnectionChange,
  showWorkspaceRootPicker = false,
  mcpEnabled = true,
  enabledMcpIds = [],
  onMcpEnabledChange,
  onEnabledMcpIdsChange,
}) => {
  const {
    isGuidingMode,
    effectiveAllowAttachments,
    message,
    setPickerOpen,
    textareaRef,
    fileInputRef,
    attachments,
    attachError,
    isDragging,
    handlePaste,
    handleFileSelect,
    removeAttachment,
    handleDragOver,
    handleDragLeave,
    handleDrop,
    pickerRef,
    mcpPickerRef,
    workspacePickerRef,
    projectFilePickerRef,
    pickerOpen,
    normalizedWorkspaceRoot,
    workspacePickerOpen,
    workspacePath,
    workspaceParent,
    workspaceEntries,
    workspaceRoots,
    workspaceLoading,
    workspaceError,
    loadWorkspaceDirectories,
    handleToggleWorkspacePicker,
    handleSelectWorkspaceRoot,
    hasDirectoryContext,
    hasRemoteContext,
    mcpPickerOpen,
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
    isRemoteRequiredMcpId,
    loadAvailableMcpConfigs,
    handleToggleMcpPicker,
    handleSelectAllMcp,
    handleToggleMcpSelection,
    enabledModels,
    hasAiOptions,
    projectForFilePicker,
    showProjectFilePicker,
    workspaceRootDisplayName,
    currentAiLabel,
    projectFilePickerOpen,
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
    handleInputChange,
    handleKeyDown,
    handleSend,
    canSend,
  } = useInputAreaController({
    onSend,
    onGuide,
    disabled,
    isStreaming,
    isStopping,
    maxLength,
    allowAttachments,
    supportedFileTypes,
    showModelSelector,
    selectedModelId,
    availableModels,
    availableProjects,
    selectedProjectId,
    showProjectFileButton,
    showWorkspaceRootPicker,
    workspaceRoot,
    onWorkspaceRootChange,
    currentRemoteConnectionId,
    mcpEnabled,
    enabledMcpIds,
    onMcpEnabledChange,
    onEnabledMcpIdsChange,
  });

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

      <InputAreaComposer
        disabled={disabled}
        isStreaming={isStreaming}
        isStopping={isStopping}
        isGuidingMode={isGuidingMode}
        effectiveAllowAttachments={effectiveAllowAttachments}
        showModelSelector={showModelSelector}
        selectedModelId={selectedModelId}
        onModelChange={onModelChange}
        availableProjects={availableProjects}
        selectedProjectId={selectedProjectId}
        onProjectChange={onProjectChange}
        showProjectSelector={showProjectSelector}
        showWorkspaceRootPicker={showWorkspaceRootPicker}
        currentRemoteConnectionId={currentRemoteConnectionId}
        availableRemoteConnections={availableRemoteConnections}
        onRemoteConnectionChange={onRemoteConnectionChange}
        mcpEnabled={mcpEnabled}
        onMcpEnabledChange={onMcpEnabledChange}
        reasoningSupported={reasoningSupported}
        reasoningEnabled={reasoningEnabled}
        onReasoningToggle={onReasoningToggle}
        placeholder={placeholder}
        maxLength={maxLength}
        supportedFileTypes={supportedFileTypes}
        isDragging={isDragging}
        pickerRef={pickerRef}
        mcpPickerRef={mcpPickerRef}
        workspacePickerRef={workspacePickerRef}
        projectFilePickerRef={projectFilePickerRef}
        fileInputRef={fileInputRef}
        textareaRef={textareaRef}
        message={message}
        setPickerOpen={setPickerOpen}
        pickerOpen={pickerOpen}
        hasAiOptions={hasAiOptions}
        currentAiLabel={currentAiLabel}
        enabledModels={enabledModels}
        projectForFilePicker={projectForFilePicker}
        showProjectFilePicker={showProjectFilePicker}
        projectFileAttachingPath={projectFileAttachingPath}
        projectFilePickerOpen={projectFilePickerOpen}
        handleToggleProjectFilePicker={handleToggleProjectFilePicker}
        projectFilePathLabel={projectFilePathLabel}
        projectFileFilter={projectFileFilter}
        setProjectFileFilter={setProjectFileFilter}
        projectFileBusy={projectFileBusy}
        projectFileKeywordActive={projectFileKeywordActive}
        projectFileParent={projectFileParent}
        loadProjectFileEntries={loadProjectFileEntries}
        displayedProjectFileEntries={displayedProjectFileEntries}
        handleAttachProjectFile={handleAttachProjectFile}
        toRelativeProjectPath={toRelativeProjectPath}
        projectFileSearchTruncated={projectFileSearchTruncated}
        normalizedWorkspaceRoot={normalizedWorkspaceRoot}
        workspaceRootDisplayName={workspaceRootDisplayName}
        workspacePickerOpen={workspacePickerOpen}
        workspacePath={workspacePath}
        workspaceParent={workspaceParent}
        workspaceLoading={workspaceLoading}
        workspaceEntries={workspaceEntries}
        workspaceRoots={workspaceRoots}
        handleToggleWorkspacePicker={handleToggleWorkspacePicker}
        loadWorkspaceDirectories={loadWorkspaceDirectories}
        handleSelectWorkspaceRoot={handleSelectWorkspaceRoot}
        mcpPickerOpen={mcpPickerOpen}
        handleToggleMcpPicker={handleToggleMcpPicker}
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
        loadAvailableMcpConfigs={loadAvailableMcpConfigs}
        handleSelectAllMcp={handleSelectAllMcp}
        handleToggleMcpSelection={handleToggleMcpSelection}
        handleInputChange={handleInputChange}
        handleKeyDown={handleKeyDown}
        handlePaste={handlePaste}
        onStop={onStop}
        handleSend={handleSend}
        canSend={canSend}
        handleDragOver={handleDragOver}
        handleDragLeave={handleDragLeave}
        handleDrop={handleDrop}
        handleFileSelect={handleFileSelect}
      />

      {isDragging && effectiveAllowAttachments && (
        <InputAreaDragOverlay
          maxFileBytes={MAX_FILE_BYTES}
          maxTotalBytes={MAX_TOTAL_BYTES}
          maxAttachments={MAX_ATTACHMENTS}
        />
      )}
    </div>
  );
};

export default InputArea;
