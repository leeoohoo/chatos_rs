import { useCallback, useMemo } from 'react';

import { useI18n } from '../../i18n/I18nProvider';
import type { AiModelConfig, Project } from '../../types';

interface UseInputAreaContextModelOptions {
  availableModels: AiModelConfig[];
  availableProjects: Project[];
  selectedModelId: string | null;
  selectedModelName: string | null;
  selectedThinkingLevel: string | null;
  selectedProjectId: string | null;
  workspaceRoot: string | null;
  isGuidingMode: boolean;
  showProjectFileButton: boolean;
}

export const useInputAreaContextModel = ({
  availableModels,
  availableProjects,
  selectedModelId,
  selectedModelName,
  selectedThinkingLevel,
  selectedProjectId,
  workspaceRoot,
  isGuidingMode,
  showProjectFileButton,
}: UseInputAreaContextModelOptions) => {
  const { t } = useI18n();
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
  const effectiveModelName = useMemo(() => {
    const explicit = typeof selectedModelName === 'string' ? selectedModelName.trim() : '';
    return explicit || selectedModel?.model_name || null;
  }, [selectedModel?.model_name, selectedModelName]);
  const effectiveThinkingLevel = useMemo(() => {
    const explicit = typeof selectedThinkingLevel === 'string' ? selectedThinkingLevel.trim() : '';
    return explicit || selectedModel?.thinking_level || null;
  }, [selectedModel?.thinking_level, selectedThinkingLevel]);

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
      return t('inputArea.workspace.empty');
    }

    const normalized = normalizePath(normalizedWorkspaceRoot);
    const segments = normalized.split('/').filter((segment) => segment.length > 0);
    if (segments.length === 0) {
      return normalized;
    }
    return segments[segments.length - 1] || normalized;
  }, [normalizePath, normalizedWorkspaceRoot, t]);

  const currentAiLabel = useMemo(
    () => {
      if (!selectedModel) {
        return t('inputArea.model.selectTitle');
      }
      const modelName = effectiveModelName || selectedModel.model_name;
      if (!modelName) {
        return selectedModel.name;
      }
      if (selectedModel.name.toLocaleLowerCase().includes(modelName.toLocaleLowerCase())) {
        return selectedModel.name;
      }
      return `${selectedModel.name} / ${modelName}`;
    },
    [effectiveModelName, selectedModel, t],
  );

  return {
    normalizePath,
    selectedRuntimeProject,
    normalizedWorkspaceRoot,
    selectedModel,
    enabledModels,
    effectiveModelName,
    effectiveThinkingLevel,
    hasAiOptions,
    projectForFilePicker,
    projectRootForFilePicker,
    showProjectFilePicker,
    workspaceRootDisplayName,
    currentAiLabel,
  };
};
