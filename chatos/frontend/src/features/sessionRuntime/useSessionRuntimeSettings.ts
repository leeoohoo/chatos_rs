// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useCallback, useEffect, useRef, useState } from 'react';

import { useApiClient } from '../../lib/api/ApiClientContext';
import type {
  SessionRuntimeSettingsPayload,
  SessionRuntimeSettingsResponse,
} from '../../lib/api/client/types';
import type { Session } from '../../types';
import {
  normalizeNullableText,
} from '../../lib/domain/sessionSettings';

interface UseSessionRuntimeSettingsOptions {
  session: Session | null | undefined;
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
  remoteConnectionId: string | null;
  workspaceRoot: string | null;
  reasoningEnabled: boolean;
  planModeEnabled: boolean;
  setSelectedModelId: (modelId: string | null) => void;
  setSelectedModelName: (modelName: string | null) => void;
  setSelectedThinkingLevel: (level: string | null) => void;
  setModelRuntimeSelection: (selection: SessionModelRuntimeSelection) => void;
  setRemoteConnectionId: (connectionId: string | null) => void;
  setWorkspaceRoot: (path: string | null) => void;
  setReasoningEnabled: (enabled: boolean) => void;
  setPlanModeEnabled: (enabled: boolean) => void;
  flushRuntimeSettings: (targetSessionId?: string | null) => Promise<void>;
}

interface RuntimeSettingsState {
  selectedModelId: string | null;
  selectedModelName: string | null;
  selectedThinkingLevel: string | null;
  remoteConnectionId: string | null;
  workspaceRoot: string | null;
  reasoningEnabled: boolean;
  planModeEnabled: boolean;
}

const toRuntimePayload = (state: RuntimeSettingsState): SessionRuntimeSettingsPayload => ({
  selected_model_id: state.selectedModelId,
  selected_model_name: state.selectedModelName,
  selected_thinking_level: state.selectedThinkingLevel,
  remote_connection_id: state.remoteConnectionId,
  workspace_root: state.workspaceRoot,
  reasoning_enabled: state.reasoningEnabled,
  plan_mode_enabled: state.planModeEnabled,
});

const areRuntimeStatesEqual = (a: RuntimeSettingsState, b: RuntimeSettingsState): boolean => (
  a.selectedModelId === b.selectedModelId
  && a.selectedModelName === b.selectedModelName
  && a.selectedThinkingLevel === b.selectedThinkingLevel
  && a.remoteConnectionId === b.remoteConnectionId
  && a.workspaceRoot === b.workspaceRoot
  && a.reasoningEnabled === b.reasoningEnabled
  && a.planModeEnabled === b.planModeEnabled
);

const emptyRuntimeState = (): RuntimeSettingsState => ({
  selectedModelId: null,
  selectedModelName: null,
  selectedThinkingLevel: null,
  remoteConnectionId: null,
  workspaceRoot: null,
  reasoningEnabled: false,
  planModeEnabled: false,
});

const runtimeFromResponse = (
  response: SessionRuntimeSettingsResponse,
): RuntimeSettingsState => ({
  selectedModelId: Object.prototype.hasOwnProperty.call(response, 'selected_model_id')
    ? normalizeNullableText(response.selected_model_id)
    : null,
  selectedModelName: Object.prototype.hasOwnProperty.call(response, 'selected_model_name')
    ? normalizeNullableText(response.selected_model_name)
    : null,
  selectedThinkingLevel: Object.prototype.hasOwnProperty.call(response, 'selected_thinking_level')
    ? normalizeNullableText(response.selected_thinking_level)
    : null,
  remoteConnectionId: Object.prototype.hasOwnProperty.call(response, 'remote_connection_id')
    ? normalizeNullableText(response.remote_connection_id)
    : null,
  workspaceRoot: Object.prototype.hasOwnProperty.call(response, 'workspace_root')
    ? normalizeNullableText(response.workspace_root)
    : null,
  reasoningEnabled: Object.prototype.hasOwnProperty.call(response, 'reasoning_enabled')
    ? response.reasoning_enabled === true
    : false,
  planModeEnabled: Object.prototype.hasOwnProperty.call(response, 'plan_mode_enabled')
    ? response.plan_mode_enabled === true
    : false,
});

export const useSessionRuntimeSettings = ({
  session,
}: UseSessionRuntimeSettingsOptions): UseSessionRuntimeSettingsResult => {
  const client = useApiClient();
  const initialRuntime = emptyRuntimeState();
  const runtimeRef = useRef<RuntimeSettingsState>(initialRuntime);
  const activeSessionIdRef = useRef('');
  const preparedSessionRuntimeRef = useRef<{
    sessionId: string;
    state: RuntimeSettingsState;
  } | null>(null);
  const unboundRuntimeDirtyRef = useRef(false);
  const runtimeMutationSeqRef = useRef(0);
  const persistChainRef = useRef<Promise<unknown>>(Promise.resolve());
  const persistErrorRef = useRef<unknown>(null);
  const [runtimeState, setRuntimeState] = useState<RuntimeSettingsState>(initialRuntime);

  const applyRuntimeState = useCallback((next: RuntimeSettingsState) => {
    runtimeRef.current = next;
    setRuntimeState((prev) => (areRuntimeStatesEqual(prev, next) ? prev : next));
  }, []);

  useEffect(() => {
    const sessionId = typeof session?.id === 'string' ? session.id.trim() : '';
    const previousSessionId = activeSessionIdRef.current;
    activeSessionIdRef.current = sessionId;
    const preparedRuntime = preparedSessionRuntimeRef.current;
    if (sessionId && preparedRuntime?.sessionId === sessionId) {
      preparedSessionRuntimeRef.current = null;
      unboundRuntimeDirtyRef.current = false;
      applyRuntimeState(preparedRuntime.state);
      return;
    }
    preparedSessionRuntimeRef.current = null;
    if (sessionId && !previousSessionId && unboundRuntimeDirtyRef.current) {
      return;
    }
    unboundRuntimeDirtyRef.current = false;
    applyRuntimeState(emptyRuntimeState());
    if (!sessionId) {
      return;
    }

    let cancelled = false;
    const loadMutationSeq = runtimeMutationSeqRef.current;
    void client.getConversationRuntimeSettings(sessionId)
      .then((response) => {
        if (
          cancelled
          || activeSessionIdRef.current !== sessionId
          || runtimeMutationSeqRef.current !== loadMutationSeq
        ) {
          return;
        }
        applyRuntimeState(runtimeFromResponse(response));
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
    session?.id,
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
      remoteConnectionId: Object.prototype.hasOwnProperty.call(patch, 'remoteConnectionId')
        ? normalizeNullableText(patch.remoteConnectionId)
        : current.remoteConnectionId,
      workspaceRoot: Object.prototype.hasOwnProperty.call(patch, 'workspaceRoot')
        ? normalizeNullableText(patch.workspaceRoot)
        : current.workspaceRoot,
      reasoningEnabled: Object.prototype.hasOwnProperty.call(patch, 'reasoningEnabled')
        ? patch.reasoningEnabled === true
        : current.reasoningEnabled,
      planModeEnabled: Object.prototype.hasOwnProperty.call(patch, 'planModeEnabled')
        ? patch.planModeEnabled === true
        : current.planModeEnabled,
    };

    if (areRuntimeStatesEqual(current, next)) {
      return;
    }

    applyRuntimeState(next);
    if (!sessionId) {
      unboundRuntimeDirtyRef.current = true;
      return;
    }

    const payload = toRuntimePayload(next);
    unboundRuntimeDirtyRef.current = false;
    runtimeMutationSeqRef.current += 1;
    persistErrorRef.current = null;
    persistChainRef.current = persistChainRef.current
      .catch(() => undefined)
      .then(() => client.updateConversationRuntimeSettings(sessionId, payload))
      .then((response) => {
        persistErrorRef.current = null;
        applyRuntimeState(runtimeFromResponse(response));
      })
      .catch((error) => {
        persistErrorRef.current = error;
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

  const setRemoteConnectionId = useCallback((connectionId: string | null) => {
    persistRuntimePatch({ remoteConnectionId: connectionId });
  }, [persistRuntimePatch]);

  const setWorkspaceRoot = useCallback((path: string | null) => {
    persistRuntimePatch({ workspaceRoot: path });
  }, [persistRuntimePatch]);

  const setReasoningEnabled = useCallback((enabled: boolean) => {
    persistRuntimePatch({ reasoningEnabled: enabled });
  }, [persistRuntimePatch]);

  const setPlanModeEnabled = useCallback((enabled: boolean) => {
    persistRuntimePatch({ planModeEnabled: enabled });
  }, [persistRuntimePatch]);

  const flushRuntimeSettings = useCallback(async (targetSessionId?: string | null) => {
    const normalizedTargetSessionId = typeof targetSessionId === 'string'
      ? targetSessionId.trim()
      : '';
    const activeSessionId = activeSessionIdRef.current;
    if (
      normalizedTargetSessionId
      && (
        normalizedTargetSessionId !== activeSessionId
        || unboundRuntimeDirtyRef.current
      )
    ) {
      const payload = toRuntimePayload(runtimeRef.current);
      runtimeMutationSeqRef.current += 1;
      persistErrorRef.current = null;
      persistChainRef.current = persistChainRef.current
        .catch(() => undefined)
        .then(() => client.updateConversationRuntimeSettings(normalizedTargetSessionId, payload))
        .then((response) => {
          persistErrorRef.current = null;
          unboundRuntimeDirtyRef.current = false;
          const preparedState = runtimeFromResponse(response);
          preparedSessionRuntimeRef.current = {
            sessionId: normalizedTargetSessionId,
            state: preparedState,
          };
          if (
            !activeSessionIdRef.current
            || activeSessionIdRef.current === normalizedTargetSessionId
          ) {
            applyRuntimeState(preparedState);
          }
        })
        .catch((error) => {
          persistErrorRef.current = error;
          console.error('Failed to persist session runtime settings:', error);
        });
    }
    await persistChainRef.current.catch(() => undefined);
    if (persistErrorRef.current) {
      throw persistErrorRef.current instanceof Error
        ? persistErrorRef.current
        : new Error('Failed to persist session runtime settings');
    }
  }, [applyRuntimeState, client]);

  return {
    selectedModelId: runtimeState.selectedModelId,
    selectedModelName: runtimeState.selectedModelName,
    selectedThinkingLevel: runtimeState.selectedThinkingLevel,
    remoteConnectionId: runtimeState.remoteConnectionId,
    workspaceRoot: runtimeState.workspaceRoot,
    reasoningEnabled: runtimeState.reasoningEnabled,
    planModeEnabled: runtimeState.planModeEnabled,
    setSelectedModelId,
    setSelectedModelName,
    setSelectedThinkingLevel,
    setModelRuntimeSelection,
    setRemoteConnectionId,
    setWorkspaceRoot,
    setReasoningEnabled,
    setPlanModeEnabled,
    flushRuntimeSettings,
  };
};
