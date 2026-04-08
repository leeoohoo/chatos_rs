import { useCallback, useEffect, useMemo, useState } from 'react';

import type { Session } from '../../types';
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
  disableSessionMcpSelection?: boolean;
}

interface UseSessionRuntimeSettingsResult {
  mcpEnabled: boolean;
  enabledMcpIds: string[];
  workspaceRoot: string | null;
  setMcpEnabled: (enabled: boolean) => void;
  setEnabledMcpIds: (ids: string[]) => void;
  setWorkspaceRoot: (path: string | null) => void;
}

const EMPTY_MCP_ID_LIST: string[] = [];

const normalizeNullableText = (value: unknown): string | null => {
  if (typeof value !== 'string') {
    return null;
  }
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : null;
};

const normalizeIdList = (value: unknown): string[] => {
  if (!Array.isArray(value)) {
    return [];
  }
  const out: string[] = [];
  for (const item of value) {
    if (typeof item !== 'string') {
      continue;
    }
    const trimmed = item.trim();
    if (!trimmed || out.includes(trimmed)) {
      continue;
    }
    out.push(trimmed);
  }
  return out;
};

const isSameStringArray = (left: string[], right: string[]): boolean => {
  if (left.length !== right.length) {
    return false;
  }
  return left.every((item, index) => item === right[index]);
};

export const useSessionRuntimeSettings = ({
  session,
  updateSession,
  defaultMcpEnabled = true,
  defaultEnabledMcpIds = EMPTY_MCP_ID_LIST,
  defaultWorkspaceRoot = null,
  disableSessionMcpSelection = false,
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

  useEffect(() => {
    const runtime = readSessionRuntimeFromMetadata(session?.metadata);
    const nextEnabled = disableSessionMcpSelection
      ? true
      : (runtime?.mcpEnabled ?? defaultMcpEnabled);
    const nextMcpIds = disableSessionMcpSelection
      ? []
      : normalizeIdList(runtime?.enabledMcpIds ?? normalizedDefaultMcpIds);
    const nextWorkspaceRoot = normalizeNullableText(runtime?.workspaceRoot ?? normalizedDefaultWorkspaceRoot);

    setMcpEnabledState((prev) => (prev === nextEnabled ? prev : nextEnabled));
    setEnabledMcpIdsState((prev) => (isSameStringArray(prev, nextMcpIds) ? prev : nextMcpIds));
    setWorkspaceRootState((prev) => (prev === nextWorkspaceRoot ? prev : nextWorkspaceRoot));
  }, [
    defaultMcpEnabled,
    disableSessionMcpSelection,
    normalizedDefaultMcpIds,
    normalizedDefaultWorkspaceRoot,
    session?.id,
    session?.metadata,
  ]);

  const persistRuntimePatch = useCallback((patch: {
    mcpEnabled?: boolean;
    enabledMcpIds?: string[];
    workspaceRoot?: string | null;
  }) => {
    if (!session?.id || !updateSession) {
      return;
    }
    const runtime = readSessionRuntimeFromMetadata(session.metadata);
    const currentEnabled = disableSessionMcpSelection
      ? true
      : (runtime?.mcpEnabled ?? defaultMcpEnabled);
    const currentMcpIds = disableSessionMcpSelection
      ? []
      : normalizeIdList(runtime?.enabledMcpIds ?? normalizedDefaultMcpIds);
    const currentWorkspaceRoot = runtime?.workspaceRoot ?? normalizedDefaultWorkspaceRoot;

    const nextEnabled = disableSessionMcpSelection
      ? true
      : (typeof patch.mcpEnabled === 'boolean' ? patch.mcpEnabled : currentEnabled);
    const nextMcpIds = disableSessionMcpSelection
      ? []
      : (patch.enabledMcpIds ? normalizeIdList(patch.enabledMcpIds) : currentMcpIds);
    const nextWorkspaceRoot = patch.workspaceRoot !== undefined
      ? normalizeNullableText(patch.workspaceRoot)
      : currentWorkspaceRoot;

    if (
      currentEnabled === nextEnabled
      && isSameStringArray(currentMcpIds, nextMcpIds)
      && currentWorkspaceRoot === nextWorkspaceRoot
    ) {
      return;
    }

    const metadata = mergeSessionRuntimeIntoMetadata(session.metadata, {
      mcpEnabled: nextEnabled,
      enabledMcpIds: nextMcpIds,
      workspaceRoot: nextWorkspaceRoot,
    });
    void updateSession(session.id, { metadata } as Partial<Session>);
  }, [
    defaultMcpEnabled,
    disableSessionMcpSelection,
    normalizedDefaultMcpIds,
    normalizedDefaultWorkspaceRoot,
    session,
    updateSession,
  ]);

  const setMcpEnabled = useCallback((enabled: boolean) => {
    if (disableSessionMcpSelection) {
      return;
    }
    setMcpEnabledState((prev) => (prev === enabled ? prev : enabled));
    persistRuntimePatch({
      mcpEnabled: enabled,
      enabledMcpIds,
      workspaceRoot,
    });
  }, [disableSessionMcpSelection, enabledMcpIds, persistRuntimePatch, workspaceRoot]);

  const setEnabledMcpIds = useCallback((ids: string[]) => {
    if (disableSessionMcpSelection) {
      return;
    }
    const normalized = normalizeIdList(ids);
    setEnabledMcpIdsState((prev) => (isSameStringArray(prev, normalized) ? prev : normalized));
    persistRuntimePatch({
      mcpEnabled,
      enabledMcpIds: normalized,
      workspaceRoot,
    });
  }, [disableSessionMcpSelection, mcpEnabled, persistRuntimePatch, workspaceRoot]);

  const setWorkspaceRoot = useCallback((path: string | null) => {
    const normalized = normalizeNullableText(path);
    setWorkspaceRootState((prev) => (prev === normalized ? prev : normalized));
    persistRuntimePatch({
      mcpEnabled,
      enabledMcpIds,
      workspaceRoot: normalized,
    });
  }, [enabledMcpIds, mcpEnabled, persistRuntimePatch]);

  return {
    mcpEnabled,
    enabledMcpIds,
    workspaceRoot,
    setMcpEnabled,
    setEnabledMcpIds,
    setWorkspaceRoot,
  };
};
