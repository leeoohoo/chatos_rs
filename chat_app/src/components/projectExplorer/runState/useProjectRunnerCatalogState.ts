import { useCallback, useEffect, useMemo, useState } from 'react';

import type ApiClient from '../../../lib/api/client';
import type { Project, ProjectRunTarget } from '../../../types';
import {
  buildProjectRunnerTarget,
  hasProjectRunnerScript,
  isProjectRunnerPathMissingError,
  normalizeProjectRunnerMembers,
  readProjectRunnerErrorMessage,
  RUNNER_SCRIPT_REL_PATH,
} from '../../../lib/domain/projectRunner';
import type { ProjectRunnerMember } from '../../../lib/domain/projectRunner';

interface UseProjectRunnerCatalogStateOptions {
  client: ApiClient;
  project: Project | null;
}

export const useProjectRunnerCatalogState = ({
  client,
  project,
}: UseProjectRunnerCatalogStateOptions) => {
  const [projectMembers, setProjectMembers] = useState<ProjectRunnerMember[]>([]);
  const [projectMembersLoading, setProjectMembersLoading] = useState(false);
  const [projectMembersError, setProjectMembersError] = useState<string | null>(null);
  const [runnerScriptExists, setRunnerScriptExists] = useState(false);
  const [runnerScriptChecking, setRunnerScriptChecking] = useState(false);
  const [runnerScriptError, setRunnerScriptError] = useState<string | null>(null);
  const [runnerRootMissing, setRunnerRootMissing] = useState(false);
  const [selectedRunTargetId, setSelectedRunTargetId] = useState<string | null>(null);

  const loadProjectMembers = useCallback(async () => {
    if (!project?.id) {
      setProjectMembers([]);
      setProjectMembersLoading(false);
      setProjectMembersError(null);
      return;
    }

    setProjectMembersLoading(true);
    setProjectMembersError(null);
    try {
      const rows = await client.listProjectContacts(project.id, { limit: 500, offset: 0 });
      setProjectMembers(normalizeProjectRunnerMembers(rows));
    } catch (error) {
      setProjectMembers([]);
      setProjectMembersError(error instanceof Error ? error.message : '加载项目成员失败');
    } finally {
      setProjectMembersLoading(false);
    }
  }, [client, project?.id]);

  const loadRunnerScriptState = useCallback(async () => {
    if (!project?.rootPath) {
      setRunnerScriptExists(false);
      setRunnerScriptChecking(false);
      setRunnerScriptError(null);
      setRunnerRootMissing(false);
      return;
    }

    setRunnerScriptChecking(true);
    setRunnerScriptError(null);
    try {
      const exists = await hasProjectRunnerScript(client, project.rootPath);
      setRunnerScriptExists(exists);
      setRunnerRootMissing(false);
    } catch (error) {
      setRunnerScriptExists(false);
      if (isProjectRunnerPathMissingError(error)) {
        setRunnerRootMissing(true);
        setRunnerScriptError('项目目录不存在，请检查项目路径');
      } else {
        setRunnerRootMissing(false);
        setRunnerScriptError(readProjectRunnerErrorMessage(error, '检查启动脚本失败'));
      }
    } finally {
      setRunnerScriptChecking(false);
    }
  }, [client, project?.rootPath]);

  const refreshRunnerState = useCallback(async () => {
    await Promise.all([
      loadProjectMembers(),
      loadRunnerScriptState(),
    ]);
  }, [loadProjectMembers, loadRunnerScriptState]);

  const resetRunnerCatalogState = useCallback(() => {
    setProjectMembers([]);
    setProjectMembersError(null);
    setRunnerScriptExists(false);
    setRunnerScriptError(null);
    setRunnerRootMissing(false);
    setSelectedRunTargetId(null);
  }, []);

  useEffect(() => {
    if (!project?.id || runnerScriptExists || runnerRootMissing) {
      return;
    }
    let disposed = false;
    let timer: ReturnType<typeof setTimeout> | null = null;
    const poll = async () => {
      try {
        await loadRunnerScriptState();
      } finally {
        if (!disposed) {
          timer = setTimeout(() => {
            void poll();
          }, 2500);
        }
      }
    };
    void poll();
    return () => {
      disposed = true;
      if (timer) {
        clearTimeout(timer);
      }
    };
  }, [loadRunnerScriptState, project?.id, runnerRootMissing, runnerScriptExists]);

  const runStatus = useMemo(() => {
    if (!project?.id) {
      return 'idle';
    }
    if (runnerRootMissing) {
      return 'missing_root';
    }
    if (runnerScriptChecking || projectMembersLoading) {
      return 'loading';
    }
    if (runnerScriptError || projectMembersError) {
      return 'error';
    }
    if (runnerScriptExists) {
      return 'ready';
    }
    if (projectMembers.length === 0) {
      return 'no_member';
    }
    return 'script_missing';
  }, [
    project?.id,
    projectMembers.length,
    projectMembersError,
    projectMembersLoading,
    runnerRootMissing,
    runnerScriptChecking,
    runnerScriptError,
    runnerScriptExists,
  ]);

  const runTargets = useMemo<ProjectRunTarget[]>(() => {
    if (!project?.rootPath || !runnerScriptExists) {
      return [];
    }
    return [buildProjectRunnerTarget(project.rootPath)];
  }, [project?.rootPath, runnerScriptExists]);

  useEffect(() => {
    if (!runnerScriptExists || runTargets.length === 0) {
      setSelectedRunTargetId(null);
      return;
    }
    setSelectedRunTargetId((prev) => prev || runTargets[0].id);
  }, [runTargets, runnerScriptExists]);

  return {
    projectMembers,
    projectMembersLoading,
    projectMembersError,
    runnerScriptExists,
    runnerScriptChecking,
    runnerScriptPath: RUNNER_SCRIPT_REL_PATH,
    runnerRootMissing,
    runStatus,
    runTargets,
    runCatalogLoading: runnerScriptChecking || projectMembersLoading,
    runCatalogError: runnerScriptError || projectMembersError,
    selectedRunTargetId,
    setSelectedRunTargetId,
    loadProjectMembers,
    loadRunnerScriptState,
    refreshRunnerState,
    resetRunnerCatalogState,
  };
};
