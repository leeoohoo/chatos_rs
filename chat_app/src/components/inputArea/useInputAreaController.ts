import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import { apiClient as globalApiClient } from '../../lib/api/client';
import { useChatApiClientFromContext } from '../../lib/store/ChatStoreContext';
import type { AiModelConfig, InputAreaProps } from '../../types';
import { useAttachmentsInput } from './useAttachmentsInput';
import { useDismissiblePopover } from './useDismissiblePopover';
import { useMcpSelection } from './useMcpSelection';
import { useProjectFilePicker } from './useProjectFilePicker';
import { useWorkspaceDirectoryPicker } from './useWorkspaceDirectoryPicker';
import type { AgentConfig } from '../../types';

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
  | 'onMcpEnabledChange'
  | 'onEnabledMcpIdsChange'
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
  onMcpEnabledChange,
  onEnabledMcpIdsChange,
}: UseInputAreaControllerParams) {
  const isGuidingMode = isStreaming && !isStopping;
  const effectiveAllowAttachments = allowAttachments && !isGuidingMode;

  const [message, setMessage] = useState('');
  const [pickerOpen, setPickerOpen] = useState(false);
  const [skillsEnabled, setSkillsEnabled] = useState(false);
  const [selectedSkillIds, setSelectedSkillIds] = useState<string[]>([]);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const apiClientFromContext = useChatApiClientFromContext();
  const client = useMemo(() => apiClientFromContext || globalApiClient, [apiClientFromContext]);

  const currentAgentForSkills = useMemo<AgentConfig | null>(
    () => (currentAgent && typeof currentAgent === 'object' ? currentAgent : null),
    [currentAgent],
  );
  const [resolvedAgentForSkills, setResolvedAgentForSkills] = useState<AgentConfig | null>(null);
  const [skillsLoading, setSkillsLoading] = useState(false);

  useEffect(() => {
    let cancelled = false;
    const baseAgent = currentAgentForSkills;
    const agentId = typeof baseAgent?.id === 'string' ? baseAgent.id.trim() : '';
    if (!baseAgent || !agentId) {
      setResolvedAgentForSkills(null);
      setSkillsLoading(false);
      return undefined;
    }
    const hasRuntimeSkills = Array.isArray(baseAgent.runtime_skills)
      && baseAgent.runtime_skills.length > 0;
    const hasInlineSkills = Array.isArray(baseAgent.skills)
      && baseAgent.skills.length > 0;
    if (hasRuntimeSkills || hasInlineSkills) {
      setResolvedAgentForSkills(baseAgent);
      setSkillsLoading(false);
      return undefined;
    }

    setResolvedAgentForSkills(baseAgent);
    setSkillsLoading(true);
    void client.getMemoryAgentRuntimeContext(agentId)
      .then((runtime) => {
        if (cancelled) {
          return;
        }
        setResolvedAgentForSkills({
          ...baseAgent,
          runtime_skills: Array.isArray(runtime?.runtime_skills) ? runtime.runtime_skills as any : [],
          runtime_plugins: Array.isArray(runtime?.runtime_plugins) ? runtime.runtime_plugins as any : [],
          plugin_sources: Array.isArray(runtime?.plugin_sources) ? runtime.plugin_sources as any : baseAgent.plugin_sources,
          skills: Array.isArray(runtime?.skills) ? runtime.skills as any : baseAgent.skills,
          skill_ids: Array.isArray(runtime?.skill_ids) ? runtime.skill_ids as any : baseAgent.skill_ids,
        });
      })
      .catch(() => {
        if (!cancelled) {
          setResolvedAgentForSkills(baseAgent);
        }
      })
      .finally(() => {
        if (!cancelled) {
          setSkillsLoading(false);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [client, currentAgentForSkills]);

  const availableSkillOptions = useMemo(() => {
    const byId = new Map<string, { id: string; name: string; description?: string | null }>();
    const runtimeSkills = Array.isArray(resolvedAgentForSkills?.runtime_skills)
      ? resolvedAgentForSkills.runtime_skills
      : [];
    runtimeSkills.forEach((skill) => {
      const id = typeof skill?.id === 'string' ? skill.id.trim() : '';
      if (!id || byId.has(id)) return;
      const name = typeof skill?.name === 'string' && skill.name.trim().length > 0
        ? skill.name.trim()
        : id;
      const description = typeof skill?.description === 'string' ? skill.description.trim() : '';
      byId.set(id, {
        id,
        name,
        description: description || null,
      });
    });
    const inlineSkills = Array.isArray(resolvedAgentForSkills?.skills)
      ? resolvedAgentForSkills.skills
      : [];
    inlineSkills.forEach((skill) => {
      const id = typeof skill?.id === 'string' ? skill.id.trim() : '';
      if (!id || byId.has(id)) return;
      const name = typeof skill?.name === 'string' && skill.name.trim().length > 0
        ? skill.name.trim()
        : id;
      byId.set(id, { id, name, description: null });
    });
    return Array.from(byId.values());
  }, [resolvedAgentForSkills]);

  useEffect(() => {
    setSelectedSkillIds((prev) => prev.filter((id) => availableSkillOptions.some((item) => item.id === id)));
  }, [availableSkillOptions]);

  useEffect(() => {
    if (!resolvedAgentForSkills) {
      setSkillsEnabled(false);
      setSelectedSkillIds([]);
    }
  }, [resolvedAgentForSkills]);

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
    return (availableProjects || []).find((project) => project.id === selectedProjectId) || null;
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

  const selectedModel = useMemo<AiModelConfig | null>(
    () => (selectedModelId ? (availableModels || []).find((model) => model.id === selectedModelId) || null : null),
    [availableModels, selectedModelId],
  );

  const enabledModels = useMemo(
    () => (availableModels || []).filter((model) => model.enabled),
    [availableModels],
  );

  const hasAiOptions = Boolean(availableModels && availableModels.length > 0);
  const projectForFilePicker = useMemo(
    () => selectedRuntimeProject || null,
    [selectedRuntimeProject],
  );

  const projectRootForFilePicker = useMemo(() => {
    if (!projectForFilePicker?.rootPath) {
      return null;
    }
    return normalizePath(projectForFilePicker.rootPath);
  }, [normalizePath, projectForFilePicker?.rootPath]);

  const showProjectFilePicker = !isGuidingMode
    && showProjectFileButton
    && Boolean(projectRootForFilePicker);

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
    () => (selectedModel ? `Model: ${selectedModel.name}` : '选择模型'),
    [selectedModel],
  );

  const adjustTextareaHeight = useCallback(() => {
    const textarea = textareaRef.current;
    if (!textarea) {
      return;
    }

    textarea.style.height = 'auto';
    const scrollHeight = textarea.scrollHeight;
    textarea.style.height = `${Math.min(scrollHeight, 200)}px`;
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

  const projectFilePickerRef = useDismissiblePopover<HTMLDivElement>(
    projectFilePickerOpen,
    () => setProjectFilePickerOpen(false),
  );

  useEffect(() => {
    if (!isGuidingMode || attachments.length === 0) {
      return;
    }
    clearAttachments();
  }, [attachments.length, clearAttachments, isGuidingMode]);

  const resetComposer = useCallback(() => {
    setMessage('');
    clearAttachments();
    if (textareaRef.current) {
      textareaRef.current.style.height = 'auto';
    }
  }, [clearAttachments]);

  const handleInputChange = useCallback((event: React.ChangeEvent<HTMLTextAreaElement>) => {
    const value = event.target.value;
    if (value.length <= maxLength) {
      setMessage(value);
      adjustTextareaHeight();
    }
  }, [adjustTextareaHeight, maxLength]);

  const handleSend = useCallback(() => {
    const trimmedMessage = message.trim();
    if (!trimmedMessage && (!effectiveAllowAttachments || attachments.length === 0)) {
      return;
    }
    if (disabled) {
      return;
    }

    if (isGuidingMode) {
      if (!trimmedMessage) {
        return;
      }
      onGuide?.(trimmedMessage);
      resetComposer();
      return;
    }

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
      remoteConnectionId: currentRemoteConnectionId,
      projectId: runtimeProjectId,
      projectRoot: runtimeProjectRoot,
      workspaceRoot: runtimeWorkspaceRoot,
      skillsEnabled,
      selectedSkillIds: skillsEnabled ? selectedSkillIds : [],
    });
    resetComposer();
  }, [
    attachments,
    currentRemoteConnectionId,
    disabled,
    effectiveAllowAttachments,
    isGuidingMode,
    mcpEnabled,
    message,
    normalizedWorkspaceRoot,
    onGuide,
    onSend,
    resetComposer,
    sanitizedEnabledMcpIds,
    selectedSkillIds,
    selectedModelId,
    selectedRuntimeProject,
    showModelSelector,
    skillsEnabled,
    resolvedAgentForSkills,
  ]);

  const handleKeyDown = useCallback((event: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.key === 'Enter' && !event.shiftKey) {
      event.preventDefault();
      handleSend();
    }
  }, [handleSend]);

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
    currentAgentForSkills: resolvedAgentForSkills,
    skillsEnabled,
    setSkillsEnabled,
    skillsLoading,
    availableSkillOptions,
    selectedSkillIds,
    handleToggleSelectedSkill: (skillId: string) => {
      const normalized = typeof skillId === 'string' ? skillId.trim() : '';
      if (!normalized) return;
      setSelectedSkillIds((prev) => (
        prev.includes(normalized)
          ? prev.filter((item) => item !== normalized)
          : [...prev, normalized]
      ));
    },
    handleClearSelectedSkills: () => {
      setSelectedSkillIds([]);
    },
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
    canSend: Boolean(message.trim() || (!isGuidingMode && attachments.length > 0)),
  };
}
