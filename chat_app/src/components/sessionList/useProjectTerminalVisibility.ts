import { useCallback, useEffect, useRef, useState } from 'react';

import type ApiClient from '../../lib/api/client';
import { normalizeProjectRunEnvironment } from '../../lib/domain/projectExplorer';
import { useProjectRunRealtime } from '../../lib/realtime/useProjectRunRealtime';
import type { Project } from '../../types';
import { resolveProjectRunnerRealtimeCatalogAction } from '../projectExplorer/runState/projectRunnerCatalogState';

interface UseProjectTerminalVisibilityOptions {
  client: ApiClient;
  project: Project | null;
}

interface ProjectTerminalVisibilityState {
  resolved: boolean;
  enabled: boolean;
}

const DEFAULT_STATE: ProjectTerminalVisibilityState = {
  resolved: true,
  enabled: true,
};

export const useProjectTerminalVisibility = ({
  client,
  project,
}: UseProjectTerminalVisibilityOptions) => {
  const [state, setState] = useState<ProjectTerminalVisibilityState>(DEFAULT_STATE);
  const requestVersionRef = useRef(0);
  const activeProjectIdRef = useRef<string | null>(null);

  const refresh = useCallback(async () => {
    const projectId = project?.id || null;
    if (!projectId) {
      activeProjectIdRef.current = null;
      requestVersionRef.current += 1;
      setState(DEFAULT_STATE);
      return;
    }

    activeProjectIdRef.current = projectId;
    const requestVersion = ++requestVersionRef.current;
    setState({
      resolved: false,
      enabled: false,
    });

    try {
      const raw = await client.getProjectRunEnvironment(projectId);
      if (
        requestVersionRef.current !== requestVersion
        || activeProjectIdRef.current !== projectId
      ) {
        return;
      }
      const normalized = normalizeProjectRunEnvironment(raw);
      setState({
        resolved: true,
        enabled: normalized.terminalUiEnabled,
      });
    } catch {
      if (
        requestVersionRef.current !== requestVersion
        || activeProjectIdRef.current !== projectId
      ) {
        return;
      }
      setState(DEFAULT_STATE);
    }
  }, [client, project?.id]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useProjectRunRealtime({
    enabled: Boolean(project?.id),
    projectId: project?.id || null,
    onCatalogUpdated: async (payload) => {
      if (resolveProjectRunnerRealtimeCatalogAction(payload) !== 'reload_environment') {
        return;
      }
      await refresh();
    },
  });

  return {
    terminalUiResolved: state.resolved,
    terminalUiEnabled: state.enabled,
    showTerminalSection: project?.id ? state.resolved && state.enabled : true,
  };
};
