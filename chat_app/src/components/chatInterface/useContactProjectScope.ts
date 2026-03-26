import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  normalizeProjectScopeId,
  resolveSessionProjectScopeId,
} from '../../features/contactSession/sessionResolver';
import type { Session } from '../../types';

interface ContactProjectScopeApiClient {
  getContactProjects: (
    contactId: string,
    params?: { limit?: number; offset?: number },
  ) => Promise<unknown[]>;
}

interface ContactProjectScopeProject {
  id: string;
  name?: string;
}

interface UseContactProjectScopeOptions<TProject extends ContactProjectScopeProject> {
  apiClient: ContactProjectScopeApiClient;
  currentSession: Session | Record<string, any> | null;
  currentContactId: string;
  projects: TProject[];
}

interface UseContactProjectScopeResult<TProject extends ContactProjectScopeProject> {
  composerProjectId: string | null;
  currentProjectIdForMemory: string;
  currentProjectNameForMemory: string;
  composerAvailableProjects: TProject[];
  handleComposerProjectChange: (projectId: string | null) => void;
}

export const useContactProjectScope = <TProject extends ContactProjectScopeProject>({
  apiClient,
  currentSession,
  currentContactId,
  projects,
}: UseContactProjectScopeOptions<TProject>): UseContactProjectScopeResult<TProject> => {
  const [composerProjectId, setComposerProjectId] = useState<string | null>(null);
  const [contactScopedProjectIds, setContactScopedProjectIds] = useState<string[]>([]);
  const contactProjectsLoadSeqRef = useRef(0);

  const currentProjectIdForMemory = useMemo(() => {
    if (!currentSession) {
      return '';
    }
    const fromComposer = normalizeProjectScopeId(composerProjectId);
    if (fromComposer !== '0') {
      return fromComposer;
    }
    const fromSession = resolveSessionProjectScopeId(currentSession as any);
    if (fromSession !== '0') {
      return fromSession;
    }
    return '0';
  }, [composerProjectId, currentSession]);

  const currentProjectNameForMemory = useMemo(() => {
    if (!currentProjectIdForMemory) {
      return '';
    }
    if (currentProjectIdForMemory === '0') {
      return '未选择项目';
    }
    const matched = (projects || []).find((item) => item.id === currentProjectIdForMemory);
    return matched?.name || '';
  }, [currentProjectIdForMemory, projects]);

  const composerAvailableProjects = useMemo(() => {
    if (!currentContactId) {
      return [];
    }
    const allowedIds = new Set(contactScopedProjectIds);
    return (projects || []).filter((item) => allowedIds.has(item.id));
  }, [contactScopedProjectIds, currentContactId, projects]);

  useEffect(() => {
    const sessionProjectId = resolveSessionProjectScopeId(currentSession as any);
    setComposerProjectId(sessionProjectId !== '0' ? sessionProjectId : null);
  }, [currentSession?.id, currentSession?.metadata]);

  useEffect(() => {
    const contactId = currentContactId.trim();
    if (!contactId) {
      setContactScopedProjectIds([]);
      return;
    }

    const loadSeq = ++contactProjectsLoadSeqRef.current;
    void apiClient.getContactProjects(contactId, { limit: 1000, offset: 0 })
      .then((rows) => {
        if (loadSeq !== contactProjectsLoadSeqRef.current) {
          return;
        }
        const ids = Array.from(new Set(
          (Array.isArray(rows) ? rows : [])
            .map((item: any) => (typeof item?.project_id === 'string' ? item.project_id.trim() : ''))
            .filter((projectId: string) => projectId.length > 0 && projectId !== '0'),
        ));
        setContactScopedProjectIds(ids);
      })
      .catch((error) => {
        if (loadSeq !== contactProjectsLoadSeqRef.current) {
          return;
        }
        console.error('Failed to load contact projects:', error);
        setContactScopedProjectIds([]);
      });
  }, [apiClient, currentContactId]);

  useEffect(() => {
    if (!composerProjectId) {
      return;
    }
    const exists = composerAvailableProjects.some((item) => item.id === composerProjectId);
    if (!exists) {
      setComposerProjectId(null);
    }
  }, [composerAvailableProjects, composerProjectId]);

  const handleComposerProjectChange = useCallback((projectId: string | null) => {
    const normalizedProjectId = typeof projectId === 'string' ? projectId.trim() : '';
    const nextComposerProjectId = normalizedProjectId.length > 0 ? normalizedProjectId : null;
    if (
      nextComposerProjectId
      && !composerAvailableProjects.some((item) => item.id === nextComposerProjectId)
    ) {
      return;
    }
    setComposerProjectId(nextComposerProjectId);
  }, [composerAvailableProjects]);

  return {
    composerProjectId,
    currentProjectIdForMemory,
    currentProjectNameForMemory,
    composerAvailableProjects,
    handleComposerProjectChange,
  };
};
