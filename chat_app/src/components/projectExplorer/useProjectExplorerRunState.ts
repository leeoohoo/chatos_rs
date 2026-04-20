import { useCallback, useEffect, useMemo, useState } from 'react';

import type ApiClient from '../../lib/api/client';
import type {
  FsEntry,
  Project,
  ProjectRunTarget,
} from '../../types';
import { buildSingleFileRunProfile } from './runProfiles';
import { normalizeEntry } from './utils';

const RUNNER_SCRIPT_DIR = '.chatos';
const RUNNER_SCRIPT_FILE = 'project_runner.sh';
const RUNNER_SCRIPT_REL_PATH = `${RUNNER_SCRIPT_DIR}/${RUNNER_SCRIPT_FILE}`;
const RUNNER_START_COMMAND = `bash ./${RUNNER_SCRIPT_REL_PATH} start`;
const RUNNER_STOP_COMMAND = `bash ./${RUNNER_SCRIPT_REL_PATH} stop`;
const RUNNER_RESTART_COMMAND = `bash ./${RUNNER_SCRIPT_REL_PATH} restart`;
const FS_PATH_NOT_FOUND_ERROR = '路径不存在';

interface UseProjectExplorerRunStateParams {
  client: ApiClient;
  project: Project | null;
  selectedEntry: FsEntry | null;
  selectedPath: string | null;
  getParentPath: (path: string | null | undefined) => string;
  setActionError: (value: string | null) => void;
  setActionLoading: (value: boolean) => void;
  setActionMessage: (value: string | null) => void;
}

export interface ProjectRunnerMember {
  contactId: string;
  agentId: string;
  name: string;
}

export interface ProjectRunnerActiveTerminal {
  terminalId: string;
  terminalName: string;
  cwd: string;
  command: string;
  dispatchedAt: number;
}

const readTrimmedString = (value: unknown): string => (typeof value === 'string' ? value.trim() : '');

const normalizeProjectMembers = (value: unknown): ProjectRunnerMember[] => {
  const deduped = new Map<string, ProjectRunnerMember>();
  for (const row of Array.isArray(value) ? value : []) {
    const contactId = readTrimmedString((row as Record<string, unknown>)?.contact_id)
      || readTrimmedString((row as Record<string, unknown>)?.contactId);
    const agentId = readTrimmedString((row as Record<string, unknown>)?.agent_id)
      || readTrimmedString((row as Record<string, unknown>)?.agentId);
    const name = readTrimmedString((row as Record<string, unknown>)?.agent_name_snapshot)
      || readTrimmedString((row as Record<string, unknown>)?.agentNameSnapshot)
      || contactId;
    if (!contactId || !agentId) {
      continue;
    }
    deduped.set(contactId, {
      contactId,
      agentId,
      name: name || contactId,
    });
  }
  return Array.from(deduped.values());
};

const normalizeRootPath = (value: string): string => value.trim().replace(/[\\/]+$/, '');

const readErrorMessage = (error: unknown): string => (
  error instanceof Error ? error.message : '检查启动脚本失败'
);

const isPathMissingError = (error: unknown): boolean => (
  readErrorMessage(error).includes(FS_PATH_NOT_FOUND_ERROR)
);

const hasRunnerScript = async (client: ApiClient, rootPath: string): Promise<boolean> => {
  const safeRoot = normalizeRootPath(rootPath);
  if (!safeRoot) {
    return false;
  }
  const rootList = await client.listFsEntries(safeRoot);
  const rootEntries = Array.isArray(rootList?.entries) ? rootList.entries.map(normalizeEntry) : [];
  const runnerDirEntry = rootEntries.find((entry) => entry.isDir && entry.name === RUNNER_SCRIPT_DIR) || null;
  if (!runnerDirEntry?.path) {
    return false;
  }
  const runnerDirPath = runnerDirEntry.path;
  try {
    const runnerList = await client.listFsEntries(runnerDirPath);
    const runnerEntries = Array.isArray(runnerList?.entries) ? runnerList.entries.map(normalizeEntry) : [];
    return runnerEntries.some((entry) => !entry.isDir && entry.name === RUNNER_SCRIPT_FILE);
  } catch {
    return false;
  }
};

export const useProjectExplorerRunState = ({
  client,
  project,
  selectedEntry,
  selectedPath,
  getParentPath,
  setActionError,
  setActionLoading,
  setActionMessage,
}: UseProjectExplorerRunStateParams) => {
  const [projectMembers, setProjectMembers] = useState<ProjectRunnerMember[]>([]);
  const [projectMembersLoading, setProjectMembersLoading] = useState(false);
  const [projectMembersError, setProjectMembersError] = useState<string | null>(null);
  const [runnerScriptExists, setRunnerScriptExists] = useState(false);
  const [runnerScriptChecking, setRunnerScriptChecking] = useState(false);
  const [runnerScriptError, setRunnerScriptError] = useState<string | null>(null);
  const [runnerRootMissing, setRunnerRootMissing] = useState(false);
  const [starting, setStarting] = useState(false);
  const [stopping, setStopping] = useState(false);
  const [restarting, setRestarting] = useState(false);
  const [runnerMessage, setRunnerMessage] = useState<string | null>(null);
  const [runnerError, setRunnerError] = useState<string | null>(null);
  const [activeRun, setActiveRun] = useState<ProjectRunnerActiveTerminal | null>(null);
  const [activeTerminalBusy, setActiveTerminalBusy] = useState(false);
  const [selectedRunTargetId, setSelectedRunTargetId] = useState<string | null>(null);

  const runCwd = useMemo(() => {
    if (!project?.rootPath) {
      return '';
    }
    if (selectedEntry?.isDir) {
      return selectedEntry.path;
    }
    if (selectedEntry && !selectedEntry.isDir) {
      return getParentPath(selectedEntry.path) || project.rootPath;
    }
    if (selectedPath) {
      return getParentPath(selectedPath) || project.rootPath;
    }
    return project.rootPath;
  }, [getParentPath, project?.rootPath, selectedEntry, selectedPath]);

  const handleDispatchTerminalCommand = useCallback(async (payload: { cwd: string; command: string }) => {
    return client.dispatchTerminalCommand({
      cwd: payload.cwd,
      command: payload.command,
      project_id: project?.id,
      create_if_missing: true,
    });
  }, [client, project?.id]);

  const handleInterruptTerminal = useCallback(async (terminalId: string, payload?: { reason?: string }) => {
    return client.interruptTerminal(terminalId, payload);
  }, [client]);

  const handleGetTerminal = useCallback(async (terminalId: string) => {
    return client.getTerminal(terminalId);
  }, [client]);

  const handleListTerminalLogs = useCallback(async (
    terminalId: string,
    params?: { limit?: number; offset?: number; before?: string },
  ) => {
    return client.listTerminalLogs(terminalId, params);
  }, [client]);

  const handleListTerminals = useCallback(async () => {
    return client.listTerminals();
  }, [client]);

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
      setProjectMembers(normalizeProjectMembers(rows));
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
      const exists = await hasRunnerScript(client, project.rootPath);
      setRunnerScriptExists(exists);
      setRunnerRootMissing(false);
    } catch (error) {
      setRunnerScriptExists(false);
      if (isPathMissingError(error)) {
        setRunnerRootMissing(true);
        setRunnerScriptError('项目目录不存在，请检查项目路径');
      } else {
        setRunnerRootMissing(false);
        setRunnerScriptError(readErrorMessage(error));
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

  useEffect(() => {
    setProjectMembers([]);
    setProjectMembersError(null);
    setRunnerScriptExists(false);
    setRunnerScriptError(null);
    setRunnerRootMissing(false);
    setRunnerMessage(null);
    setRunnerError(null);
    setActiveRun(null);
    setActiveTerminalBusy(false);
    setSelectedRunTargetId(null);
    if (!project?.id) {
      return;
    }
    void refreshRunnerState();
  }, [project?.id, refreshRunnerState]);

  const dispatchRunnerCommand = useCallback(async (command: string, label: string) => {
    const rootPath = readTrimmedString(project?.rootPath || '');
    if (!project?.id || !rootPath) {
      throw new Error('项目根目录不存在');
    }
    if (!runnerScriptExists) {
      throw new Error('启动脚本不存在，请先点击“生成启动脚本”');
    }
    const result = await client.dispatchTerminalCommand({
      cwd: rootPath,
      command,
      project_id: project.id,
      create_if_missing: true,
    });
    const terminalId = readTrimmedString((result as { terminal_id?: string | null; terminalId?: string | null })?.terminal_id)
      || readTrimmedString((result as { terminal_id?: string | null; terminalId?: string | null })?.terminalId);
    const terminalName = readTrimmedString(
      (result as { terminal_name?: string | null; terminalName?: string | null })?.terminal_name
      || (result as { terminal_name?: string | null; terminalName?: string | null })?.terminalName
      || terminalId
    );
    if (terminalId) {
      setActiveRun({
        terminalId,
        terminalName: terminalName || terminalId,
        cwd: rootPath,
        command,
        dispatchedAt: Date.now(),
      });
      setActiveTerminalBusy(true);
    }
    setRunnerMessage(
      terminalName
        ? `${label}：已在终端 ${terminalName} 执行`
        : `${label}：命令已派发到终端`
    );
  }, [client, project?.id, project?.rootPath, runnerScriptExists]);

  const handleRunnerStart = useCallback(async () => {
    setStarting(true);
    setRunnerError(null);
    try {
      await dispatchRunnerCommand(RUNNER_START_COMMAND, '启动成功');
    } catch (error) {
      setRunnerError(error instanceof Error ? error.message : '启动失败');
      setRunnerMessage(null);
    } finally {
      setStarting(false);
    }
  }, [dispatchRunnerCommand]);

  const handleRunnerStop = useCallback(async () => {
    setStopping(true);
    setRunnerError(null);
    try {
      await dispatchRunnerCommand(RUNNER_STOP_COMMAND, '停止成功');
    } catch (error) {
      setRunnerError(error instanceof Error ? error.message : '停止失败');
      setRunnerMessage(null);
    } finally {
      setStopping(false);
    }
  }, [dispatchRunnerCommand]);

  const handleRunnerRestart = useCallback(async () => {
    setRestarting(true);
    setRunnerError(null);
    try {
      await dispatchRunnerCommand(RUNNER_RESTART_COMMAND, '重启成功');
    } catch (error) {
      setRunnerError(error instanceof Error ? error.message : '重启失败');
      setRunnerMessage(null);
    } finally {
      setRestarting(false);
    }
  }, [dispatchRunnerCommand]);

  useEffect(() => {
    if (!project?.id) {
      setActiveRun(null);
      setActiveTerminalBusy(false);
      return;
    }
    let disposed = false;
    let timer: ReturnType<typeof setTimeout> | null = null;
    const poll = async () => {
      try {
        const list = await client.listTerminals();
        if (disposed || !Array.isArray(list)) {
          return;
        }
        const related = list
          .filter((item) => {
            const terminalProjectId = readTrimmedString(
              (item as { project_id?: string | null; projectId?: string | null })?.project_id
              || (item as { project_id?: string | null; projectId?: string | null })?.projectId
            );
            const status = readTrimmedString((item as { status?: string | null })?.status);
            return terminalProjectId === project.id && status === 'running';
          })
          .sort((a, b) => {
            const left = new Date(
              (a as { last_active_at?: string | null; lastActiveAt?: string | null })?.last_active_at
              || (a as { last_active_at?: string | null; lastActiveAt?: string | null })?.lastActiveAt
              || 0
            ).getTime();
            const right = new Date(
              (b as { last_active_at?: string | null; lastActiveAt?: string | null })?.last_active_at
              || (b as { last_active_at?: string | null; lastActiveAt?: string | null })?.lastActiveAt
              || 0
            ).getTime();
            return right - left;
          });
        const busy = related.find((item) => Boolean((item as { busy?: boolean })?.busy));
        const chosen = busy || related[0] || null;
        if (!chosen) {
          setActiveTerminalBusy(false);
          return;
        }
        const terminalId = readTrimmedString((chosen as { id?: string | null })?.id);
        const terminalName = readTrimmedString((chosen as { name?: string | null })?.name) || terminalId;
        setActiveTerminalBusy(Boolean((chosen as { busy?: boolean })?.busy));
        if (!terminalId) {
          return;
        }
        setActiveRun((prev) => ({
          terminalId,
          terminalName,
          command: prev?.command || RUNNER_START_COMMAND,
          cwd: readTrimmedString((chosen as { cwd?: string | null })?.cwd) || prev?.cwd || readTrimmedString(project.rootPath || ''),
          dispatchedAt: prev?.dispatchedAt || Date.now(),
        }));
      } catch {
        // ignore polling errors
      } finally {
        if (!disposed) {
          timer = setTimeout(() => {
            void poll();
          }, 2000);
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
  }, [client, project?.id, project?.rootPath]);

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

  useEffect(() => {
    if (!activeRun?.terminalId) {
      setActiveTerminalBusy(false);
      return;
    }
    let disposed = false;
    let timer: ReturnType<typeof setTimeout> | null = null;
    const poll = async () => {
      try {
        const terminal = await client.getTerminal(activeRun.terminalId);
        if (disposed) {
          return;
        }
        setActiveTerminalBusy(Boolean(terminal?.busy));
      } catch {
        if (!disposed) {
          setActiveTerminalBusy(false);
        }
      } finally {
        if (!disposed) {
          timer = setTimeout(() => {
            void poll();
          }, 1500);
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
  }, [activeRun?.terminalId, client]);

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
    return [{
      id: 'project_runner_start',
      label: 'project_runner.sh start',
      kind: 'script',
      cwd: project.rootPath,
      command: RUNNER_START_COMMAND,
      source: 'script',
      confidence: 1,
      isDefault: true,
    }];
  }, [project?.rootPath, runnerScriptExists]);

  useEffect(() => {
    if (!runnerScriptExists || runTargets.length === 0) {
      setSelectedRunTargetId(null);
      return;
    }
    setSelectedRunTargetId((prev) => prev || runTargets[0].id);
  }, [runTargets, runnerScriptExists]);

  const runCatalogLoading = runnerScriptChecking || projectMembersLoading;
  const runCatalogError = runnerScriptError || projectMembersError;

  const handleAnalyzeRunTargets = useCallback(() => {
    void refreshRunnerState();
  }, [refreshRunnerState]);

  const canRunFile = useCallback((entry: FsEntry) => {
    if (entry.isDir) {
      return false;
    }
    return Boolean(buildSingleFileRunProfile(entry.path));
  }, []);

  const handleRunFile = useCallback(async (entry: FsEntry) => {
    const profile = buildSingleFileRunProfile(entry.path);
    if (!profile) {
      setActionError('该文件类型暂不支持直接运行');
      return;
    }
    setActionLoading(true);
    setActionError(null);
    setActionMessage(null);
    try {
      const result = await client.dispatchTerminalCommand({
        cwd: profile.cwd,
        command: profile.command,
        project_id: project?.id,
        create_if_missing: true,
      });
      const terminalName = readTrimmedString(
        (result as { terminal_name?: string | null; terminal_id?: string | null })?.terminal_name
        || (result as { terminal_name?: string | null; terminal_id?: string | null })?.terminal_id
      );
      setActionMessage(terminalName ? `已在终端 ${terminalName} 运行文件` : '已派发运行命令');
    } catch (error) {
      setActionError(error instanceof Error ? error.message : '运行文件失败');
    } finally {
      setActionLoading(false);
    }
  }, [client, project?.id, setActionError, setActionLoading, setActionMessage]);

  return {
    runCwd,
    runStatus,
    runTargets,
    runCatalogLoading,
    runCatalogError,
    selectedRunTargetId,
    setSelectedRunTargetId,
    handleDispatchTerminalCommand,
    handleInterruptTerminal,
    handleGetTerminal,
    handleListTerminalLogs,
    handleListTerminals,
    handleAnalyzeRunTargets,
    canRunFile,
    handleRunFile,
    projectMembers,
    projectMembersLoading,
    projectMembersError,
    runnerScriptExists,
    runnerScriptChecking,
    runnerScriptPath: RUNNER_SCRIPT_REL_PATH,
    runnerStartCommand: RUNNER_START_COMMAND,
    runnerStopCommand: RUNNER_STOP_COMMAND,
    runnerRestartCommand: RUNNER_RESTART_COMMAND,
    starting,
    stopping,
    restarting,
    runnerMessage,
    runnerError,
    activeRun,
    activeTerminalBusy,
    handleRunnerStart,
    handleRunnerStop,
    handleRunnerRestart,
    refreshRunnerState,
  };
};
