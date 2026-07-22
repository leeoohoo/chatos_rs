// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useCallback, useMemo } from 'react';

import { useI18n } from '../../i18n/I18nProvider';
import { getUserVisiblePath } from '../../lib/domain/filesystem';
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

const normalizedModelIdentityPart = (value: string | null | undefined): string => (
  typeof value === 'string' ? value.trim().toLocaleLowerCase() : ''
);

const chatModelIdentity = (model: AiModelConfig): string => (
  [model.provider, model.model_name, model.name]
    .map(normalizedModelIdentityPart)
    .join('\u0000')
);

const modelTimestamp = (value: Date): number => {
  const timestamp = value.getTime();
  return Number.isFinite(timestamp) ? timestamp : 0;
};

const shouldPreferChatModel = (
  candidate: AiModelConfig,
  current: AiModelConfig,
): boolean => {
  if (candidate.has_api_key !== current.has_api_key) {
    return candidate.has_api_key;
  }
  if (candidate.enabled !== current.enabled) {
    return candidate.enabled;
  }
  const candidateHasBaseUrl = Boolean(candidate.base_url.trim());
  const currentHasBaseUrl = Boolean(current.base_url.trim());
  if (candidateHasBaseUrl !== currentHasBaseUrl) {
    return candidateHasBaseUrl;
  }
  const candidateUpdatedAt = modelTimestamp(candidate.updatedAt);
  const currentUpdatedAt = modelTimestamp(current.updatedAt);
  if (candidateUpdatedAt !== currentUpdatedAt) {
    return candidateUpdatedAt > currentUpdatedAt;
  }
  const candidateCreatedAt = modelTimestamp(candidate.createdAt);
  const currentCreatedAt = modelTimestamp(current.createdAt);
  if (candidateCreatedAt !== currentCreatedAt) {
    return candidateCreatedAt > currentCreatedAt;
  }
  return candidate.id.localeCompare(current.id) > 0;
};

export const selectChatModelOptions = (models: AiModelConfig[]): AiModelConfig[] => {
  const preferredByIdentity = new Map<string, { index: number; model: AiModelConfig }>();
  (models || []).forEach((model, index) => {
    const identity = chatModelIdentity(model);
    const current = preferredByIdentity.get(identity);
    if (!current) {
      preferredByIdentity.set(identity, { index, model });
      return;
    }
    if (shouldPreferChatModel(model, current.model)) {
      preferredByIdentity.set(identity, { index: current.index, model });
    }
  });

  return Array.from(preferredByIdentity.values())
    .sort((left, right) => left.index - right.index)
    .map((item) => item.model)
    .filter((model) => model.enabled && model.has_api_key && model.model_name.trim());
};

export const resolveChatModelSelection = (
  models: AiModelConfig[],
  options: AiModelConfig[],
  selectedModelId: string | null,
): AiModelConfig | null => {
  if (!selectedModelId) {
    return null;
  }
  const direct = options.find((model) => model.id === selectedModelId);
  if (direct) {
    return direct;
  }
  const selected = (models || []).find((model) => model.id === selectedModelId);
  if (!selected) {
    return null;
  }
  const identity = chatModelIdentity(selected);
  return options.find((model) => chatModelIdentity(model) === identity) || null;
};

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

  const enabledModels = useMemo(
    () => selectChatModelOptions(availableModels || []),
    [availableModels],
  );
  const selectedModel = useMemo<AiModelConfig | null>(
    () => resolveChatModelSelection(availableModels || [], enabledModels, selectedModelId),
    [availableModels, enabledModels, selectedModelId],
  );
  const effectiveSelectedModelId = selectedModel?.id || null;
  const effectiveModelName = useMemo(() => {
    const explicit = typeof selectedModelName === 'string' ? selectedModelName.trim() : '';
    return explicit || selectedModel?.model_name || null;
  }, [selectedModel?.model_name, selectedModelName]);
  const effectiveThinkingLevel = useMemo(() => {
    const explicit = typeof selectedThinkingLevel === 'string' ? selectedThinkingLevel.trim() : '';
    return explicit || selectedModel?.thinking_level || null;
  }, [selectedModel?.thinking_level, selectedThinkingLevel]);

  const hasAiOptions = enabledModels.length > 0;
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

    const normalized = getUserVisiblePath(normalizePath(normalizedWorkspaceRoot));
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
    effectiveSelectedModelId,
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
