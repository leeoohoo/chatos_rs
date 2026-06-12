import { useCallback, useEffect, useRef, useState } from 'react';

import { useI18n } from '../../i18n/I18nProvider';
import { useDialogService } from '../ui/DialogProvider';
import { useApiClient } from '../../lib/api/ApiClientContext';
import type { InputAreaProps } from '../../types';
import { useAttachmentsInput } from './useAttachmentsInput';
import { useDismissiblePopover } from './useDismissiblePopover';
import { useProjectFilePicker } from './useProjectFilePicker';
import { useWorkspaceDirectoryPicker } from './useWorkspaceDirectoryPicker';
import { useInputAreaContextModel } from './useInputAreaContextModel';
import { useInputAreaMessageDraft } from './useInputAreaMessageDraft';

type UseInputAreaControllerParams = Pick<
  InputAreaProps,
  | 'onSend'
  | 'disabled'
  | 'maxLength'
  | 'allowAttachments'
  | 'supportedFileTypes'
  | 'showModelSelector'
  | 'selectedModelId'
  | 'selectedModelName'
  | 'selectedThinkingLevel'
  | 'onModelChange'
  | 'onModelNameChange'
  | 'onThinkingLevelChange'
  | 'onModelRuntimeChange'
  | 'availableModels'
  | 'availableProjects'
  | 'selectedProjectId'
  | 'showProjectFileButton'
  | 'showWorkspaceRootPicker'
  | 'workspaceRoot'
  | 'onWorkspaceRootChange'
  | 'currentRemoteConnectionId'
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
  disabled = false,
  maxLength = 4000,
  allowAttachments = false,
  supportedFileTypes = DEFAULT_SUPPORTED_FILE_TYPES,
  showModelSelector = false,
  selectedModelId = null,
  selectedModelName = null,
  selectedThinkingLevel = null,
  onModelChange,
  onModelNameChange,
  onThinkingLevelChange,
  onModelRuntimeChange,
  availableModels = [],
  availableProjects = [],
  selectedProjectId = null,
  showProjectFileButton = true,
  showWorkspaceRootPicker = false,
  workspaceRoot = null,
  onWorkspaceRootChange,
  currentRemoteConnectionId = null,
}: UseInputAreaControllerParams) {
  const { t } = useI18n();
  const effectiveAllowAttachments = allowAttachments;

  const [pickerOpen, setPickerOpen] = useState(false);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const client = useApiClient();
  const { alert } = useDialogService();
  const normalizeNullableText = useCallback((value: string | null | undefined) => {
    const normalized = typeof value === 'string' ? value.trim() : '';
    return normalized.length > 0 ? normalized : null;
  }, []);
  const [localSelectedModelName, setLocalSelectedModelName] = useState<string | null>(
    () => normalizeNullableText(selectedModelName),
  );
  const [localSelectedThinkingLevel, setLocalSelectedThinkingLevel] = useState<string | null>(
    () => normalizeNullableText(selectedThinkingLevel),
  );

  useEffect(() => {
    setLocalSelectedModelName(normalizeNullableText(selectedModelName));
    setLocalSelectedThinkingLevel(normalizeNullableText(selectedThinkingLevel));
  }, [normalizeNullableText, selectedModelName, selectedThinkingLevel]);

  const handleModelRuntimeChange = useCallback((selection: {
    selectedModelId?: string | null;
    selectedModelName?: string | null;
    selectedThinkingLevel?: string | null;
  }) => {
    const hasModelId = Object.prototype.hasOwnProperty.call(selection, 'selectedModelId');
    const hasModelName = Object.prototype.hasOwnProperty.call(selection, 'selectedModelName');
    const hasThinkingLevel = Object.prototype.hasOwnProperty.call(selection, 'selectedThinkingLevel');
    const nextModelId = hasModelId
      ? normalizeNullableText(selection.selectedModelId)
      : selectedModelId;
    const nextModelName = hasModelName
      ? normalizeNullableText(selection.selectedModelName)
      : localSelectedModelName;
    const nextThinkingLevel = hasThinkingLevel
      ? normalizeNullableText(selection.selectedThinkingLevel)
      : localSelectedThinkingLevel;

    setLocalSelectedModelName((prev) => (prev === nextModelName ? prev : nextModelName));
    setLocalSelectedThinkingLevel((prev) => (
      prev === nextThinkingLevel ? prev : nextThinkingLevel
    ));

    if (onModelRuntimeChange) {
      onModelRuntimeChange({
        selectedModelId: nextModelId,
        selectedModelName: nextModelName,
        selectedThinkingLevel: nextThinkingLevel,
      });
      return;
    }
    if (hasModelId) {
      onModelChange?.(nextModelId);
    }
    if (hasModelName) {
      onModelNameChange?.(nextModelName);
    }
    if (hasThinkingLevel) {
      onThinkingLevelChange?.(nextThinkingLevel);
    }
  }, [
    localSelectedModelName,
    localSelectedThinkingLevel,
    normalizeNullableText,
    onModelChange,
    onModelNameChange,
    onModelRuntimeChange,
    onThinkingLevelChange,
    selectedModelId,
  ]);

  const handleModelNameChange = useCallback((modelName: string | null) => {
    const normalized = normalizeNullableText(modelName);
    setLocalSelectedModelName((prev) => (prev === normalized ? prev : normalized));
    handleModelRuntimeChange({
      selectedModelName: normalized,
    });
  }, [handleModelRuntimeChange, normalizeNullableText]);

  const handleThinkingLevelChange = useCallback((level: string | null) => {
    const normalized = normalizeNullableText(level);
    setLocalSelectedThinkingLevel((prev) => (prev === normalized ? prev : normalized));
    handleModelRuntimeChange({
      selectedThinkingLevel: normalized,
    });
  }, [handleModelRuntimeChange, normalizeNullableText]);

  const handleModelChange = useCallback((modelId: string | null) => {
    const normalizedModelId = normalizeNullableText(modelId);
    const nextModel = normalizedModelId
      ? (availableModels || []).find((model) => model.id === normalizedModelId) || null
      : null;
    const nextModelName = normalizeNullableText(nextModel?.model_name);
    const nextThinkingLevel = normalizeNullableText(nextModel?.thinking_level);
    setLocalSelectedModelName(nextModelName);
    setLocalSelectedThinkingLevel(nextThinkingLevel);
    handleModelRuntimeChange({
      selectedModelId: normalizedModelId,
      selectedModelName: nextModelName,
      selectedThinkingLevel: nextThinkingLevel,
    });
  }, [
    availableModels,
    handleModelRuntimeChange,
    normalizeNullableText,
  ]);

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
    effectiveModelName,
    effectiveThinkingLevel,
  } = useInputAreaContextModel({
    availableModels,
    availableProjects,
    selectedModelId,
    selectedModelName: localSelectedModelName,
    selectedThinkingLevel: localSelectedThinkingLevel,
    selectedProjectId,
    workspaceRoot,
    isGuidingMode: false,
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
    isStreaming: false,
    isStopping: false,
    normalizedWorkspaceRoot,
    onWorkspaceRootChange,
  });

  const pickerRef = useDismissiblePopover<HTMLDivElement>(pickerOpen, () => setPickerOpen(false));
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
        title: t('inputArea.send.selectModelTitle'),
        message: t('inputArea.send.selectModelMessage'),
        type: 'warning',
      });
      return true;
    }
    return false;
  }, [alert, selectedModelId, showModelSelector, t]);

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
    maxLength,
    normalizedWorkspaceRoot,
    onSend,
    requireModelSelection,
    selectedModelId,
    selectedRuntimeProject,
    effectiveModelName,
    effectiveThinkingLevel,
  });

  return {
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
    selectedModel,
    enabledModels,
    selectedModelName: localSelectedModelName,
    selectedThinkingLevel: localSelectedThinkingLevel,
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
  };
}
