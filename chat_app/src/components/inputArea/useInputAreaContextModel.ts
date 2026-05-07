import { useCallback, useMemo } from 'react';

import type { AiModelConfig, Project } from '../../types';

interface UseInputAreaContextModelOptions {
  availableModels: AiModelConfig[];
  availableProjects: Project[];
  selectedModelId: string | null;
  selectedProjectId: string | null;
  workspaceRoot: string | null;
  isGuidingMode: boolean;
  showProjectFileButton: boolean;
}

export const useInputAreaContextModel = ({
  availableModels,
  availableProjects,
  selectedModelId,
  selectedProjectId,
  workspaceRoot,
  isGuidingMode,
  showProjectFileButton,
}: UseInputAreaContextModelOptions) => {
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

  return {
    normalizePath,
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
  };
};
