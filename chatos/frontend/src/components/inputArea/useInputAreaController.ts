// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ChangeEvent,
  type KeyboardEvent,
  type SyntheticEvent,
} from 'react';

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
import {
  findPluginMentionAtCursor,
  replacePluginMention,
} from './pluginMentions';
import { useTaskPluginPicker } from './useTaskPluginPicker';
import type { TaskRunnerSelectablePluginResponse } from '../../lib/api/client/types';

type UseInputAreaControllerParams = Pick<
  InputAreaProps,
  | 'onSend'
  | 'conversationId'
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
  | 'planModeEnabled'
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
  conversationId = null,
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
  planModeEnabled = false,
}: UseInputAreaControllerParams) {
  const { t } = useI18n();
  const effectiveAllowAttachments = allowAttachments;

  const [pickerOpen, setPickerOpen] = useState(false);
  const [pluginMentionSuggestionIndex, setPluginMentionSuggestionIndex] = useState(0);
  const [messageCursor, setMessageCursor] = useState(0);
  const [dismissedPluginMentionKey, setDismissedPluginMentionKey] = useState<string | null>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const pluginMentionDiscoveryRequestedRef = useRef(false);
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
    effectiveSelectedModelId,
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
  const pluginPicker = useTaskPluginPicker({
    client,
    conversationId,
    project: selectedRuntimeProject,
    projectId: selectedProjectId,
    disabled,
    planMode: planModeEnabled,
  });

  useEffect(() => {
    if (
      !selectedModelId
      || !selectedModel
      || !effectiveSelectedModelId
      || effectiveSelectedModelId === selectedModelId
    ) {
      return;
    }
    handleModelRuntimeChange({
      selectedModelId: effectiveSelectedModelId,
      selectedModelName: selectedModel.model_name || null,
      selectedThinkingLevel: selectedModel.thinking_level || null,
    });
  }, [
    effectiveSelectedModelId,
    handleModelRuntimeChange,
    selectedModel,
    selectedModelId,
  ]);

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
    if (showModelSelector && !effectiveSelectedModelId) {
      void alert({
        title: t('inputArea.send.selectModelTitle'),
        message: t('inputArea.send.selectModelMessage'),
        type: 'warning',
      });
      return true;
    }
    return false;
  }, [alert, effectiveSelectedModelId, showModelSelector, t]);

  const {
    message,
    textareaRef,
    handleInputChange: handleDraftInputChange,
    setMessageValue,
    handleKeyDown: handleDraftKeyDown,
    handleSend,
    canSend,
  } = useInputAreaMessageDraft({
    attachments,
    clearAttachments,
    disabled,
    effectiveAllowAttachments,
    maxLength,
    onSend,
    requireModelSelection,
    selectedPluginKeys: pluginPicker.selectedPlugins.map((plugin) => plugin.plugin_key),
    clearSelectedPlugins: pluginPicker.clearSelectedPlugins,
    selectedProjectId,
    selectedRuntimeProject,
  });

  const handleInputChange = useCallback((event: ChangeEvent<HTMLTextAreaElement>) => {
    setMessageCursor(event.currentTarget.selectionStart ?? event.currentTarget.value.length);
    handleDraftInputChange(event);
  }, [handleDraftInputChange]);

  const handleTextareaSelect = useCallback((event: SyntheticEvent<HTMLTextAreaElement>) => {
    setMessageCursor(event.currentTarget.selectionStart ?? event.currentTarget.value.length);
  }, []);

  const pluginMention = useMemo(
    () => findPluginMentionAtCursor(message, messageCursor),
    [message, messageCursor],
  );
  const pluginMentionKey = pluginMention ? `${message}\u0000${messageCursor}` : null;
  const pluginMentionSuggestions = useMemo(() => (
    pluginMention
      ? pluginPicker.pluginSuggestions(pluginMention.query).slice(0, 24)
      : []
  ), [pluginMention, pluginPicker]);
  const pluginMentionSuggestionsOpen = Boolean(
    pluginPicker.enabled
    && pluginMention
    && !pluginPicker.open
    && dismissedPluginMentionKey !== pluginMentionKey
  );

  useEffect(() => {
    if (!pluginMention || !pluginPicker.enabled) {
      pluginMentionDiscoveryRequestedRef.current = false;
      return;
    }
    if (pluginMentionDiscoveryRequestedRef.current) {
      return;
    }
    pluginMentionDiscoveryRequestedRef.current = true;
    void pluginPicker.loadPicker();
  }, [pluginMention, pluginPicker.enabled, pluginPicker.loadPicker]);

  useEffect(() => {
    setPluginMentionSuggestionIndex((current) => (
      pluginMentionSuggestions.length === 0
        ? 0
        : Math.min(current, pluginMentionSuggestions.length - 1)
    ));
  }, [pluginMention?.query, pluginMentionSuggestions.length]);

  const selectPluginMentionSuggestion = useCallback((
    plugin: TaskRunnerSelectablePluginResponse,
  ) => {
    if (!pluginMention) {
      return;
    }
    const replaced = replacePluginMention(message, pluginMention, plugin.plugin_key);
    if (replaced.message.length > maxLength) {
      return;
    }
    pluginPicker.selectPlugin(plugin.id);
    setMessageValue(replaced.message);
    setMessageCursor(replaced.cursor);
    setDismissedPluginMentionKey(null);
    window.requestAnimationFrame(() => {
      const textarea = textareaRef.current;
      textarea?.focus();
      textarea?.setSelectionRange(replaced.cursor, replaced.cursor);
    });
  }, [maxLength, message, pluginMention, pluginPicker, setMessageValue, textareaRef]);

  const handleKeyDown = useCallback((event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (pluginMentionSuggestionsOpen) {
      if (event.key === 'ArrowDown' && pluginMentionSuggestions.length > 0) {
        event.preventDefault();
        setPluginMentionSuggestionIndex((current) => (
          (current + 1) % pluginMentionSuggestions.length
        ));
        return;
      }
      if (event.key === 'ArrowUp' && pluginMentionSuggestions.length > 0) {
        event.preventDefault();
        setPluginMentionSuggestionIndex((current) => (
          (current - 1 + pluginMentionSuggestions.length) % pluginMentionSuggestions.length
        ));
        return;
      }
      if (event.key === 'Escape') {
        event.preventDefault();
        setDismissedPluginMentionKey(pluginMentionKey);
        return;
      }
      if (
        ((event.key === 'Enter' && !event.shiftKey) || event.key === 'Tab')
        && pluginMentionSuggestions.length > 0
      ) {
        event.preventDefault();
        selectPluginMentionSuggestion(
          pluginMentionSuggestions[Math.min(
            pluginMentionSuggestionIndex,
            pluginMentionSuggestions.length - 1,
          )],
        );
        return;
      }
    }
    handleDraftKeyDown(event);
  }, [
    handleDraftKeyDown,
    pluginMentionKey,
    pluginMentionSuggestionIndex,
    pluginMentionSuggestions,
    pluginMentionSuggestionsOpen,
    selectPluginMentionSuggestion,
  ]);

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
    pluginPicker,
    pluginMentionSuggestions,
    pluginMentionSuggestionsOpen,
    pluginMentionSuggestionIndex,
    setPluginMentionSuggestionIndex,
    selectPluginMentionSuggestion,
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
    effectiveSelectedModelId,
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
    handleTextareaSelect,
    handleKeyDown,
    handleSend,
    canSend,
  };
}
