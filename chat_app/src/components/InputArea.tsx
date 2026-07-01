// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

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
  disabled = false,
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
  planModeAvailable = false,
  planModeEnabled = false,
  onPlanModeToggle,
  showModelSelector = false,
  selectedModelId = null,
  selectedModelName = null,
  selectedThinkingLevel = null,
  availableModels = [],
  onModelChange,
  onModelNameChange,
  onThinkingLevelChange,
  onModelRuntimeChange,
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
}) => {
  const {
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
    enabledModels,
    selectedModelName: resolvedSelectedModelName,
    selectedThinkingLevel: resolvedSelectedThinkingLevel,
    handleModelChange,
    handleModelNameChange,
    handleThinkingLevelChange,
    handleModelRuntimeChange,
    hasAiOptions,
    projectForFilePicker,
    showProjectFilePicker,
    workspaceRootDisplayName,
    currentAiLabel,
    effectiveModelName,
    effectiveThinkingLevel,
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
    disabled,
    maxLength,
    allowAttachments,
    supportedFileTypes,
    showModelSelector,
    selectedModelId,
    selectedModelName,
    selectedThinkingLevel,
    onModelChange,
    onModelNameChange,
    onThinkingLevelChange,
    onModelRuntimeChange,
    availableModels,
    planModeAvailable,
    planModeEnabled,
    availableProjects,
    selectedProjectId,
    showProjectFileButton,
    showWorkspaceRootPicker,
    workspaceRoot,
    onWorkspaceRootChange,
    currentRemoteConnectionId,
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

      <div className="relative">
        <InputAreaComposer
          disabled={disabled}
          effectiveAllowAttachments={effectiveAllowAttachments}
          showModelSelector={showModelSelector}
          selectedModelId={selectedModelId}
          selectedModelName={resolvedSelectedModelName}
          selectedThinkingLevel={resolvedSelectedThinkingLevel}
          onModelChange={handleModelChange}
          onModelNameChange={handleModelNameChange}
          onThinkingLevelChange={handleThinkingLevelChange}
          onModelRuntimeChange={handleModelRuntimeChange}
          availableProjects={availableProjects}
          selectedProjectId={selectedProjectId}
          onProjectChange={onProjectChange}
          showProjectSelector={showProjectSelector}
          showWorkspaceRootPicker={showWorkspaceRootPicker}
          currentRemoteConnectionId={currentRemoteConnectionId}
          availableRemoteConnections={availableRemoteConnections}
          onRemoteConnectionChange={onRemoteConnectionChange}
          reasoningSupported={reasoningSupported}
          reasoningEnabled={reasoningEnabled}
          onReasoningToggle={onReasoningToggle}
          planModeAvailable={planModeAvailable}
          planModeEnabled={planModeEnabled}
          onPlanModeToggle={onPlanModeToggle}
          placeholder={placeholder}
          maxLength={maxLength}
          supportedFileTypes={supportedFileTypes}
          isDragging={isDragging}
          pickerRef={pickerRef}
          workspacePickerRef={workspacePickerRef}
          projectFilePickerRef={projectFilePickerRef}
          fileInputRef={fileInputRef}
          textareaRef={textareaRef}
          message={message}
          setPickerOpen={setPickerOpen}
          pickerOpen={pickerOpen}
          hasAiOptions={hasAiOptions}
          currentAiLabel={currentAiLabel}
          effectiveModelName={effectiveModelName}
          effectiveThinkingLevel={effectiveThinkingLevel}
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
          handleInputChange={handleInputChange}
          handleKeyDown={handleKeyDown}
          handlePaste={handlePaste}
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
    </div>
  );
};

export default InputArea;
