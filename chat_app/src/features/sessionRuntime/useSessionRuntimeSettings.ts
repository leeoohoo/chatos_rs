import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import { useApiClient } from '../../lib/api/ApiClientContext';
import type {
  SessionRuntimeSettingsPayload,
  SessionRuntimeSettingsResponse,
} from '../../lib/api/client/types';
import type { Session } from '../../types';
import {
  isSameStringArray,
  normalizeIdList,
  normalizeNullableText,
} from '../../lib/domain/sessionSettings';
import { readSessionRuntimeFromMetadata } from '../../lib/store/helpers/sessionRuntime';

type UpdateSessionFn = (sessionId: string, updates: Partial<Session>) => Promise<void>;

interface UseSessionRuntimeSettingsOptions {
  session: Session | null | undefined;
  updateSession?: UpdateSessionFn;
  defaultMcpEnabled?: boolean;
  defaultEnabledMcpIds?: string[];
  defaultWorkspaceRoot?: string | null;
}

export interface SessionModelRuntimeSelection {
  selectedModelId?: string | null;
  selectedModelName?: string | null;
  selectedThinkingLevel?: string | null;
}

interface UseSessionRuntimeSettingsResult {
  selectedModelId: string | null;
  selectedModelName: string | null;
  selectedThinkingLevel: string | null;
  mcpEnabled: boolean;
  enabledMcpIds: string[];
  workspaceRoot: string | null;
  autoCreateTask: boolean;
  setSelectedModelId: (modelId: string | null) => void;
  setSelectedModelName: (modelName: string | null) => void;
  setSelectedThinkingLevel: (level: string | null) => void;
  setModelRuntimeSelection: (selection: SessionModelRuntimeSelection) => void;
  setMcpEnabled: (enabled: boolean) => void;
  setEnabledMcpIds: (ids: string[]) => void;
  setWorkspaceRoot: (path: string | null) => void;
  setAutoCreateTask: (enabled: boolean) => void;
}

interface RuntimeSettingsState {
  selectedModelId: string | null;
  selectedModelName: string | null;
  selectedThinkingLevel: string | null;
  mcpEnabled: boolean;
  enabledMcpIds: string[];
  workspaceRoot: string | null;
  autoCreateTask: boolean;
}

const EMPTY_MCP_ID_LIST: string[] = [];

const toRuntimePayload = (state: RuntimeSettingsState): SessionRuntimeSettingsPayload => ({
  selected_model_id: state.selectedModelId,
  selected_model_name: state.selectedModelName,
  selected_thinking_level: state.selectedThinkingLevel,
  mcp_enabled: state.mcpEnabled,
  enabled_mcp_ids: state.enabledMcpIds,
  workspace_root: state.workspaceRoot,
  auto_create_task: state.autoCreateTask,
});

const areRuntimeStatesEqual = (a: RuntimeSettingsState, b: RuntimeSettingsState): boolean => (
  a.selectedModelId === b.selectedModelId
  && a.selectedModelName === b.selectedModelName
  && a.selectedThinkingLevel === b.selectedThinkingLevel
  && a.mcpEnabled === b.mcpEnabled
  && isSameStringArray(a.enabledMcpIds, b.enabledMcpIds)
  && a.workspaceRoot === b.workspaceRoot
  && a.autoCreateTask === b.autoCreateTask
);

const runtimeFromResponse = (
  response: SessionRuntimeSettingsResponse,
  fallback: RuntimeSettingsState,
): RuntimeSettingsState => ({
  selectedModelId: Object.prototype.hasOwnProperty.call(response, 'selected_model_id')
    ? normalizeNullableText(response.selected_model_id)
    : fallback.selectedModelId,
  selectedModelName: Object.prototype.hasOwnProperty.call(response, 'selected_model_name')
    ? normalizeNullableText(response.selected_model_name)
    : fallback.selectedModelName,
  selectedThinkingLevel: Object.prototype.hasOwnProperty.call(response, 'selected_thinking_level')
    ? normalizeNullableText(response.selected_thinking_level)
    : fallback.selectedThinkingLevel,
  mcpEnabled: typeof response.mcp_enabled === 'boolean'
    ? response.mcp_enabled
    : fallback.mcpEnabled,
  enabledMcpIds: normalizeIdList(response.enabled_mcp_ids ?? fallback.enabledMcpIds),
  workspaceRoot: Object.prototype.hasOwnProperty.call(response, 'workspace_root')
    ? normalizeNullableText(response.workspace_root)
    : fallback.workspaceRoot,
  autoCreateTask: typeof response.auto_create_task === 'boolean'
    ? response.auto_create_task
    : fallback.autoCreateTask,
});

const runtimeFromSessionMetadata = (
  session: Session | null | undefined,
  defaults: {
    mcpEnabled: boolean;
    enabledMcpIds: string[];
    workspaceRoot: string | null;
  },
): RuntimeSettingsState => {
  const runtime = readSessionRuntimeFromMetadata(session?.metadata);
  return {
    selectedModelId: normalizeNullableText(runtime?.selectedModelId ?? null),
    selectedModelName: normalizeNullableText(runtime?.selectedModelName ?? null),
    selectedThinkingLevel: normalizeNullableText(runtime?.selectedThinkingLevel ?? null),
    mcpEnabled: runtime?.mcpEnabled ?? defaults.mcpEnabled,
    enabledMcpIds: normalizeIdList(runtime?.enabledMcpIds ?? defaults.enabledMcpIds),
    workspaceRoot: normalizeNullableText(runtime?.workspaceRoot ?? defaults.workspaceRoot),
    autoCreateTask: runtime?.autoCreateTask === true,
  };
};

export const useSessionRuntimeSettings = ({
  session,
  defaultMcpEnabled = true,
  defaultEnabledMcpIds = EMPTY_MCP_ID_LIST,
  defaultWorkspaceRoot = null,
}: UseSessionRuntimeSettingsOptions): UseSessionRuntimeSettingsResult => {
  const client = useApiClient();
  const normalizedDefaultMcpIds = useMemo(
    () => normalizeIdList(defaultEnabledMcpIds),
    [defaultEnabledMcpIds],
  );
  const normalizedDefaultWorkspaceRoot = useMemo(
    () => normalizeNullableText(defaultWorkspaceRoot),
    [defaultWorkspaceRoot],
  );
  const defaults = useMemo(() => ({
    mcpEnabled: defaultMcpEnabled,
    enabledMcpIds: normalizedDefaultMcpIds,
    workspaceRoot: normalizedDefaultWorkspaceRoot,
  }), [defaultMcpEnabled, normalizedDefaultMcpIds, normalizedDefaultWorkspaceRoot]);

  const initialRuntime = useMemo(
    () => runtimeFromSessionMetadata(session, defaults),
    [],
  );
  const runtimeRef = useRef<RuntimeSettingsState>(initialRuntime);
  const persistChainRef = useRef<Promise<unknown>>(Promise.resolve());
  const [runtimeState, setRuntimeState] = useState<RuntimeSettingsState>(initialRuntime);

  const applyRuntimeState = useCallback((next: RuntimeSettingsState) => {
    runtimeRef.current = next;
    setRuntimeState((prev) => (areRuntimeStatesEqual(prev, next) ? prev : next));
  }, []);

  useEffect(() => {
    const sessionId = typeof session?.id === 'string' ? session.id.trim() : '';
    const fallback = runtimeFromSessionMetadata(session, defaults);
    applyRuntimeState(fallback);
    if (!sessionId) {
      return;
    }

    let cancelled = false;
    void client.getConversationRuntimeSettings(sessionId)
      .then((response) => {
        if (cancelled) {
          return;
        }
        applyRuntimeState(runtimeFromResponse(response, fallback));
      })
      .catch((error) => {
        if (!cancelled) {
          console.error('Failed to load session runtime settings:', error);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [
    applyRuntimeState,
    client,
    defaults,
    session?.id,
    session?.metadata,
  ]);

  const persistRuntimePatch = useCallback((patch: Partial<RuntimeSettingsState>) => {
    const sessionId = typeof session?.id === 'string' ? session.id.trim() : '';
    const current = runtimeRef.current;
    const next: RuntimeSettingsState = {
      selectedModelId: Object.prototype.hasOwnProperty.call(patch, 'selectedModelId')
        ? normalizeNullableText(patch.selectedModelId)
        : current.selectedModelId,
      selectedModelName: Object.prototype.hasOwnProperty.call(patch, 'selectedModelName')
        ? normalizeNullableText(patch.selectedModelName)
        : current.selectedModelName,
      selectedThinkingLevel: Object.prototype.hasOwnProperty.call(patch, 'selectedThinkingLevel')
        ? normalizeNullableText(patch.selectedThinkingLevel)
        : current.selectedThinkingLevel,
      mcpEnabled: typeof patch.mcpEnabled === 'boolean'
        ? patch.mcpEnabled
        : current.mcpEnabled,
      enabledMcpIds: patch.enabledMcpIds
        ? normalizeIdList(patch.enabledMcpIds)
        : current.enabledMcpIds,
      workspaceRoot: Object.prototype.hasOwnProperty.call(patch, 'workspaceRoot')
        ? normalizeNullableText(patch.workspaceRoot)
        : current.workspaceRoot,
      autoCreateTask: typeof patch.autoCreateTask === 'boolean'
        ? patch.autoCreateTask
        : current.autoCreateTask,
    };

    if (areRuntimeStatesEqual(current, next)) {
      return;
    }

    applyRuntimeState(next);
    if (!sessionId) {
      return;
    }

    const payload = toRuntimePayload(next);
    persistChainRef.current = persistChainRef.current
      .catch(() => undefined)
      .then(() => client.updateConversationRuntimeSettings(sessionId, payload))
      .catch((error) => {
        console.error('Failed to persist session runtime settings:', error);
      });
  }, [applyRuntimeState, client, session?.id]);

  const setSelectedModelId = useCallback((modelId: string | null) => {
    persistRuntimePatch({ selectedModelId: modelId });
  }, [persistRuntimePatch]);

  const setSelectedModelName = useCallback((modelName: string | null) => {
    persistRuntimePatch({ selectedModelName: modelName });
  }, [persistRuntimePatch]);

  const setSelectedThinkingLevel = useCallback((level: string | null) => {
    persistRuntimePatch({ selectedThinkingLevel: level });
  }, [persistRuntimePatch]);

  const setModelRuntimeSelection = useCallback((selection: SessionModelRuntimeSelection) => {
    persistRuntimePatch({
      selectedModelId: Object.prototype.hasOwnProperty.call(selection, 'selectedModelId')
        ? selection.selectedModelId ?? null
        : runtimeRef.current.selectedModelId,
      selectedModelName: Object.prototype.hasOwnProperty.call(selection, 'selectedModelName')
        ? selection.selectedModelName ?? null
        : runtimeRef.current.selectedModelName,
      selectedThinkingLevel: Object.prototype.hasOwnProperty.call(selection, 'selectedThinkingLevel')
        ? selection.selectedThinkingLevel ?? null
        : runtimeRef.current.selectedThinkingLevel,
    });
  }, [persistRuntimePatch]);

  const setMcpEnabled = useCallback((enabled: boolean) => {
    persistRuntimePatch({ mcpEnabled: enabled });
  }, [persistRuntimePatch]);

  const setEnabledMcpIds = useCallback((ids: string[]) => {
    persistRuntimePatch({ enabledMcpIds: ids });
  }, [persistRuntimePatch]);

  const setWorkspaceRoot = useCallback((path: string | null) => {
    persistRuntimePatch({ workspaceRoot: path });
  }, [persistRuntimePatch]);

  const setAutoCreateTask = useCallback((enabled: boolean) => {
    persistRuntimePatch({ autoCreateTask: enabled });
  }, [persistRuntimePatch]);

  return {
    selectedModelId: runtimeState.selectedModelId,
    selectedModelName: runtimeState.selectedModelName,
    selectedThinkingLevel: runtimeState.selectedThinkingLevel,
    mcpEnabled: runtimeState.mcpEnabled,
    enabledMcpIds: runtimeState.enabledMcpIds,
    workspaceRoot: runtimeState.workspaceRoot,
    autoCreateTask: runtimeState.autoCreateTask,
    setSelectedModelId,
    setSelectedModelName,
    setSelectedThinkingLevel,
    setModelRuntimeSelection,
    setMcpEnabled,
    setEnabledMcpIds,
    setWorkspaceRoot,
    setAutoCreateTask,
  };
};
