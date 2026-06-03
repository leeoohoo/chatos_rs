import { useCallback, useMemo, useRef, useState } from 'react';

import { useDialogService } from '../ui/DialogProvider';
import { useApiClient } from '../../lib/api/ApiClientContext';
import type { InputAreaProps } from '../../types';
import { useAttachmentsInput } from './useAttachmentsInput';
import { useDismissiblePopover } from './useDismissiblePopover';
import { useMcpSelection } from './useMcpSelection';
import { useProjectFilePicker } from './useProjectFilePicker';
import { useWorkspaceDirectoryPicker } from './useWorkspaceDirectoryPicker';
import { useAgentSkillSelection } from './useAgentSkillSelection';
import { useInputAreaContextModel } from './useInputAreaContextModel';
import { useInputAreaMessageDraft } from './useInputAreaMessageDraft';

type UseInputAreaControllerParams = Pick<
  InputAreaProps,
  | 'onSend'
  | 'onGuide'
  | 'disabled'
  | 'isStreaming'
  | 'isStopping'
  | 'maxLength'
  | 'allowAttachments'
  | 'supportedFileTypes'
  | 'showModelSelector'
  | 'selectedModelId'
  | 'availableModels'
  | 'availableProjects'
  | 'selectedProjectId'
  | 'showProjectFileButton'
  | 'showWorkspaceRootPicker'
  | 'workspaceRoot'
  | 'onWorkspaceRootChange'
  | 'currentRemoteConnectionId'
  | 'currentAgent'
  | 'mcpEnabled'
  | 'enabledMcpIds'
  | 'autoCreateTask'
  | 'onMcpEnabledChange'
  | 'onEnabledMcpIdsChange'
  | 'onAutoCreateTaskChange'
>;

const DEFAULT_SUPPORTED_FILE_TYPES = [
  'image/*',
  'text/*',
  'application/json',
  'application/pdf',
  'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
];

export function useInputAreaController({
  onSend,
  onGuide,
  disabled = false,
  isStreaming = false,
  isStopping = false,
  maxLength = 4000,
  allowAttachments = false,
  supportedFileTypes = DEFAULT_SUPPORTED_FILE_TYPES,
  showModelSelector = false,
  selectedModelId = null,
  availableModels = [],
  availableProjects = [],
  selectedProjectId = null,
  showProjectFileButton = true,
  showWorkspaceRootPicker = false,
  workspaceRoot = null,
  onWorkspaceRootChange,
  currentRemoteConnectionId = null,
  currentAgent = null,
  mcpEnabled = true,
  enabledMcpIds = [],
  autoCreateTask = false,
  onMcpEnabledChange,
  onEnabledMcpIdsChange,
}: UseInputAreaControllerParams) {
  const isGuidingMode = isStreaming && !isStopping;
  const effectiveAllowAttachments = allowAttachments;

  const [pickerOpen, setPickerOpen] = useState(false);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const client = useApiClient();
  const { alert } = useDialogService();

  const {
    currentAgentForSkills,
    skillsEnabled,
    setSkillsEnabled,
    skillsLoading,
    availableSkillOptions,
    selectedSkillIds,
    handleToggleSelectedSkill,
    handleClearSelectedSkills,
  } = useAgentSkillSelection({
    client,
    currentAgent,
  });

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

  const {
    selectedRuntimeProject,
    normalizedWorkspaceRoot,
    selectedModel,
    enabledModels,
    hasAiOptions,
    projectForFilePicker,
    projectRootForFilePicker,
    showProjectFilePicker,
    workspaceRootDisplayName,
    currentAiLabel,
  } = useInputAreaContextModel({
    availableModels,
    availableProjects,
    selectedModelId,
    selectedProjectId,
    workspaceRoot,
    isGuidingMode,
    showProjectFileButton,
  });

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
  const hasRemoteContext = Boolean(
    typeof currentRemoteConnectionId === 'string' && currentRemoteConnectionId.trim().length > 0,
  );
  const mcpProjectScopeKey = useMemo(() => {
    const projectId = typeof selectedRuntimeProject?.id === 'string'
      ? selectedRuntimeProject.id.trim()
      : '';
    if (projectId) {
      return `project:${projectId}`;
    }
    if (normalizedWorkspaceRoot) {
      return `workspace:${normalizedWorkspaceRoot}`;
    }
    return null;
  }, [normalizedWorkspaceRoot, selectedRuntimeProject?.id]);

  const {
    mcpPickerOpen,
    setMcpPickerOpen,
    availableMcpConfigs,
    mcpConfigsLoading,
    mcpConfigsError,
    builtinMcpConfigs,
    customMcpConfigs,
    mcpToolsetPresets,
    projectScopeKey,
    hasProjectMcpDefault,
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
    handleApplyMcpToolsetPreset,
    handleSaveProjectMcpDefault,
    handleApplyProjectMcpDefault,
  } = useMcpSelection({
    client,
    mcpEnabled,
    enabledMcpIds,
    projectScopeKey: mcpProjectScopeKey,
    hasDirectoryContext,
    hasRemoteContext,
    disabled,
    isStreaming,
    isStopping,
    onMcpEnabledChange,
    onEnabledMcpIdsChange,
  });

  const pickerRef = useDismissiblePopover<HTMLDivElement>(pickerOpen, () => setPickerOpen(false));
  const mcpPickerRef = useDismissiblePopover<HTMLDivElement>(
    mcpPickerOpen,
    () => setMcpPickerOpen(false),
  );
  const workspacePickerRef = useDismissiblePopover<HTMLDivElement>(
    workspacePickerOpen,
    () => setWorkspacePickerOpen(false),
  );

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

  const projectFilePickerRef = useDismissiblePopover<HTMLDivElement>(
    projectFilePickerOpen,
    () => setProjectFilePickerOpen(false),
  );

  const requireModelSelection = useCallback(() => {
    if (showModelSelector && !selectedModelId) {
      void alert({
        title: '请选择模型',
        message: '请先选择一个模型',
        type: 'warning',
      });
      return true;
    }
    return false;
  }, [alert, selectedModelId, showModelSelector]);

  const {
    message,
    textareaRef,
    handleInputChange,
    handleKeyDown,
    handleSend,
    canSend,
  } = useInputAreaMessageDraft({
    attachments,
    clearAttachments,
    currentRemoteConnectionId,
    disabled,
    effectiveAllowAttachments,
    isGuidingMode,
    mcpEnabled,
    autoCreateTask,
    maxLength,
    normalizedWorkspaceRoot,
    onGuide,
    onSend,
    requireModelSelection,
    sanitizedEnabledMcpIds,
    selectedSkillIds,
    selectedRuntimeProject,
    skillsEnabled,
  });

  return {
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
    selectedRuntimeProject,
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
    mcpToolsetPresets,
    projectScopeKey,
    hasProjectMcpDefault,
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
    handleApplyMcpToolsetPreset,
    handleSaveProjectMcpDefault,
    handleApplyProjectMcpDefault,
    currentAgentForSkills,
    skillsEnabled,
    setSkillsEnabled,
    skillsLoading,
    availableSkillOptions,
    selectedSkillIds,
    handleToggleSelectedSkill,
    handleClearSelectedSkills,
    selectedModel,
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
  };
}
