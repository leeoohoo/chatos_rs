import { useCallback, useEffect, useMemo, useState } from 'react';

import type { Session } from '../../types';
import {
  isSameStringArray,
  normalizeIdList,
  normalizeNullableText,
} from '../../lib/domain/sessionSettings';
import {
  mergeSessionRuntimeIntoMetadata,
  readSessionRuntimeFromMetadata,
} from '../../lib/store/helpers/sessionRuntime';

type UpdateSessionFn = (sessionId: string, updates: Partial<Session>) => Promise<void>;

interface UseSessionRuntimeSettingsOptions {
  session: Session | null | undefined;
  updateSession?: UpdateSessionFn;
  defaultMcpEnabled?: boolean;
  defaultEnabledMcpIds?: string[];
  defaultWorkspaceRoot?: string | null;
}

interface UseSessionRuntimeSettingsResult {
  mcpEnabled: boolean;
  enabledMcpIds: string[];
  workspaceRoot: string | null;
  autoCreateTask: boolean;
  setMcpEnabled: (enabled: boolean) => void;
  setEnabledMcpIds: (ids: string[]) => void;
  setWorkspaceRoot: (path: string | null) => void;
  setAutoCreateTask: (enabled: boolean) => void;
}

const EMPTY_MCP_ID_LIST: string[] = [];

export const useSessionRuntimeSettings = ({
  session,
  updateSession,
  defaultMcpEnabled = true,
  defaultEnabledMcpIds = EMPTY_MCP_ID_LIST,
  defaultWorkspaceRoot = null,
}: UseSessionRuntimeSettingsOptions): UseSessionRuntimeSettingsResult => {
  const normalizedDefaultMcpIds = useMemo(
    () => normalizeIdList(defaultEnabledMcpIds),
    [defaultEnabledMcpIds],
  );
  const normalizedDefaultWorkspaceRoot = useMemo(
    () => normalizeNullableText(defaultWorkspaceRoot),
    [defaultWorkspaceRoot],
  );

  const [mcpEnabled, setMcpEnabledState] = useState<boolean>(defaultMcpEnabled);
  const [enabledMcpIds, setEnabledMcpIdsState] = useState<string[]>(normalizedDefaultMcpIds);
  const [workspaceRoot, setWorkspaceRootState] = useState<string | null>(normalizedDefaultWorkspaceRoot);
  const [autoCreateTask, setAutoCreateTaskState] = useState<boolean>(false);

  useEffect(() => {
    const runtime = readSessionRuntimeFromMetadata(session?.metadata);
    const nextEnabled = runtime?.mcpEnabled ?? defaultMcpEnabled;
    const nextMcpIds = normalizeIdList(runtime?.enabledMcpIds ?? normalizedDefaultMcpIds);
    const nextWorkspaceRoot = normalizeNullableText(runtime?.workspaceRoot ?? normalizedDefaultWorkspaceRoot);
    const nextAutoCreateTask = runtime?.autoCreateTask === true;

    setMcpEnabledState((prev) => (prev === nextEnabled ? prev : nextEnabled));
    setEnabledMcpIdsState((prev) => (isSameStringArray(prev, nextMcpIds) ? prev : nextMcpIds));
    setWorkspaceRootState((prev) => (prev === nextWorkspaceRoot ? prev : nextWorkspaceRoot));
    setAutoCreateTaskState((prev) => (prev === nextAutoCreateTask ? prev : nextAutoCreateTask));
  }, [
    defaultMcpEnabled,
    normalizedDefaultMcpIds,
    normalizedDefaultWorkspaceRoot,
    session?.id,
    session?.metadata,
  ]);

  const persistRuntimePatch = useCallback((patch: {
    mcpEnabled?: boolean;
    enabledMcpIds?: string[];
    workspaceRoot?: string | null;
    autoCreateTask?: boolean;
  }) => {
    if (!session?.id || !updateSession) {
      return;
    }
    const runtime = readSessionRuntimeFromMetadata(session.metadata);
    const currentEnabled = runtime?.mcpEnabled ?? defaultMcpEnabled;
    const currentMcpIds = normalizeIdList(runtime?.enabledMcpIds ?? normalizedDefaultMcpIds);
    const currentWorkspaceRoot = runtime?.workspaceRoot ?? normalizedDefaultWorkspaceRoot;
    const currentAutoCreateTask = runtime?.autoCreateTask === true;

    const nextEnabled = typeof patch.mcpEnabled === 'boolean' ? patch.mcpEnabled : currentEnabled;
    const nextMcpIds = patch.enabledMcpIds ? normalizeIdList(patch.enabledMcpIds) : currentMcpIds;
    const nextWorkspaceRoot = patch.workspaceRoot !== undefined
      ? normalizeNullableText(patch.workspaceRoot)
      : currentWorkspaceRoot;
    const nextAutoCreateTask = typeof patch.autoCreateTask === 'boolean'
      ? patch.autoCreateTask
      : currentAutoCreateTask;

    if (
      currentEnabled === nextEnabled
      && isSameStringArray(currentMcpIds, nextMcpIds)
      && currentWorkspaceRoot === nextWorkspaceRoot
      && currentAutoCreateTask === nextAutoCreateTask
    ) {
      return;
    }

    const metadata = mergeSessionRuntimeIntoMetadata(session.metadata, {
      mcpEnabled: nextEnabled,
      enabledMcpIds: nextMcpIds,
      workspaceRoot: nextWorkspaceRoot,
      autoCreateTask: nextAutoCreateTask,
    });
    void updateSession(session.id, { metadata } as Partial<Session>);
  }, [
    defaultMcpEnabled,
    normalizedDefaultMcpIds,
    normalizedDefaultWorkspaceRoot,
    session,
    updateSession,
  ]);

  const setMcpEnabled = useCallback((enabled: boolean) => {
    setMcpEnabledState((prev) => (prev === enabled ? prev : enabled));
    persistRuntimePatch({
      mcpEnabled: enabled,
      enabledMcpIds,
      workspaceRoot,
      autoCreateTask,
    });
  }, [autoCreateTask, enabledMcpIds, persistRuntimePatch, workspaceRoot]);

  const setEnabledMcpIds = useCallback((ids: string[]) => {
    const normalized = normalizeIdList(ids);
    setEnabledMcpIdsState((prev) => (isSameStringArray(prev, normalized) ? prev : normalized));
    persistRuntimePatch({
      mcpEnabled,
      enabledMcpIds: normalized,
      workspaceRoot,
      autoCreateTask,
    });
  }, [autoCreateTask, mcpEnabled, persistRuntimePatch, workspaceRoot]);

  const setWorkspaceRoot = useCallback((path: string | null) => {
    const normalized = normalizeNullableText(path);
    setWorkspaceRootState((prev) => (prev === normalized ? prev : normalized));
    persistRuntimePatch({
      mcpEnabled,
      enabledMcpIds,
      workspaceRoot: normalized,
      autoCreateTask,
    });
  }, [autoCreateTask, enabledMcpIds, mcpEnabled, persistRuntimePatch]);

  const setAutoCreateTask = useCallback((enabled: boolean) => {
    setAutoCreateTaskState((prev) => (prev === enabled ? prev : enabled));
    persistRuntimePatch({
      mcpEnabled,
      enabledMcpIds,
      workspaceRoot,
      autoCreateTask: enabled,
    });
  }, [enabledMcpIds, mcpEnabled, persistRuntimePatch, workspaceRoot]);

  return {
    mcpEnabled,
    enabledMcpIds,
    workspaceRoot,
    autoCreateTask,
    setMcpEnabled,
    setEnabledMcpIds,
    setWorkspaceRoot,
    setAutoCreateTask,
  };
};
