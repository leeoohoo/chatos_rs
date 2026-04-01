import type { AgentPageTranslate } from './types';
import {
  useAgentsPageData,
  type AgentsPageDataResult,
} from './useAgentsPageData';
import {
  useAgentsPageInspectors,
  type AgentsPageInspectorsResult,
} from './useAgentsPageInspectors';

interface UseAgentsPageControllerOptions {
  filterUserId?: string;
  currentUserId: string;
  isAdmin: boolean;
  t: AgentPageTranslate;
}

export type AgentsPageControllerResult =
  & AgentsPageDataResult
  & AgentsPageInspectorsResult;

export function useAgentsPageController({
  filterUserId,
  currentUserId,
  isAdmin,
  t,
}: UseAgentsPageControllerOptions): AgentsPageControllerResult {
  const data = useAgentsPageData({
    filterUserId,
    currentUserId,
    isAdmin,
    t,
  });

  const inspectors = useAgentsPageInspectors({
    scopeUserId: data.scopeUserId,
    t,
    pluginCatalog: data.pluginCatalog,
    onError: (message) => {
      data.setError(message);
    },
  });

  return {
    ...data,
    ...inspectors,
  };
}
