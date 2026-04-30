import type { ProjectRunTarget, Terminal } from '../../types';
import type {
  ProjectContactLinkResponse,
  TerminalDispatchResponse,
} from '../api/client/types';
import { normalizeFsEntry } from './filesystem';
import { asRecord, readValue } from './normalizerUtils';

export const RUNNER_SCRIPT_DIR = '.chatos';
export const RUNNER_SCRIPT_FILE = 'project_runner.sh';
export const RUNNER_SCRIPT_REL_PATH = `${RUNNER_SCRIPT_DIR}/${RUNNER_SCRIPT_FILE}`;
export const RUNNER_LOG_DIR_REL_PATH = 'project_runner/logs';
export const RUNNER_START_COMMAND = `bash ./${RUNNER_SCRIPT_REL_PATH} start`;
export const RUNNER_STOP_COMMAND = `bash ./${RUNNER_SCRIPT_REL_PATH} stop`;
export const RUNNER_RESTART_COMMAND = `bash ./${RUNNER_SCRIPT_REL_PATH} restart`;
export const RUNNER_GENERATION_MCP_IDS = [
  'builtin_code_maintainer_read',
  'builtin_code_maintainer_write',
  'builtin_terminal_controller',
];

const FS_PATH_NOT_FOUND_ERROR = '路径不存在';

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

export interface ProjectRuntimeTerminalSelection {
  busyTerminal: Terminal | null;
  activeTerminal: Terminal | null;
}

type RunnerFilesystemClient = {
  listFsEntries: (path?: string) => Promise<{ entries?: unknown[] } | null | undefined>;
};

type RunnerProjectContactsClient = {
  listProjectContacts: (
    projectId: string,
    paging?: { limit?: number; offset?: number },
  ) => Promise<ProjectContactLinkResponse[]>;
};

const readTrimmedString = (value: unknown): string => (
  typeof value === 'string' ? value.trim() : ''
);

const readProjectRunnerContactId = (value: ProjectContactLinkResponse | unknown): string => {
  const record = asRecord(value) || {};
  return readTrimmedString(readValue(record, 'contact_id') ?? readValue(record, 'contactId'));
};

interface ProjectRunnerContactRowsCacheEntry {
  rows: ProjectContactLinkResponse[];
  stale: boolean;
}

const projectRunnerContactRowsInflight = new WeakMap<object, Map<string, Promise<ProjectContactLinkResponse[]>>>();
const projectRunnerContactRowsCache = new WeakMap<object, Map<string, ProjectRunnerContactRowsCacheEntry>>();
const projectRunnerScriptInflight = new WeakMap<object, Map<string, Promise<boolean>>>();

const getProjectRunnerContactRowsScopedCache = (
  client: object,
): Map<string, ProjectRunnerContactRowsCacheEntry> | null => (
  projectRunnerContactRowsCache.get(client) || null
);

const ensureProjectRunnerContactRowsScopedCache = (
  client: object,
): Map<string, ProjectRunnerContactRowsCacheEntry> => {
  const existing = getProjectRunnerContactRowsScopedCache(client);
  if (existing) {
    return existing;
  }
  const next = new Map<string, ProjectRunnerContactRowsCacheEntry>();
  projectRunnerContactRowsCache.set(client, next);
  return next;
};

const setProjectRunnerContactRowsCacheEntry = (
  client: object,
  projectId: string,
  entry: ProjectRunnerContactRowsCacheEntry,
): ProjectContactLinkResponse[] => {
  const scopedCache = ensureProjectRunnerContactRowsScopedCache(client);
  scopedCache.set(projectId, entry);
  return entry.rows;
};

const withClientInflight = <Result,>(
  store: WeakMap<object, Map<string, Promise<Result>>>,
  client: object,
  key: string,
  load: () => Promise<Result>,
): Promise<Result> => {
  let scoped = store.get(client);
  if (!scoped) {
    scoped = new Map<string, Promise<Result>>();
    store.set(client, scoped);
  }

  const existing = scoped.get(key);
  if (existing) {
    return existing;
  }

  const task = load().finally(() => {
    const current = store.get(client);
    if (!current) {
      return;
    }
    current.delete(key);
    if (current.size === 0) {
      store.delete(client);
    }
  });

  scoped.set(key, task);
  return task;
};

export const normalizeProjectRunnerRootPath = (value: string): string => (
  value.trim().replace(/[\\/]+$/, '')
);

export const readProjectRunnerErrorMessage = (
  error: unknown,
  fallback: string,
): string => (
  error instanceof Error ? error.message : fallback
);

export const isProjectRunnerPathMissingError = (error: unknown): boolean => (
  readProjectRunnerErrorMessage(error, '').includes(FS_PATH_NOT_FOUND_ERROR)
);

export const normalizeProjectRunnerMembers = (
  value: ProjectContactLinkResponse[] | unknown,
): ProjectRunnerMember[] => {
  const deduped = new Map<string, ProjectRunnerMember>();

  for (const row of Array.isArray(value) ? value : []) {
    const record = asRecord(row) || {};
    const contactId = readTrimmedString(readValue(record, 'contact_id') ?? readValue(record, 'contactId'));
    const agentId = readTrimmedString(readValue(record, 'agent_id') ?? readValue(record, 'agentId'));
    const name = readTrimmedString(
      readValue(record, 'agent_name_snapshot')
      ?? readValue(record, 'agentNameSnapshot')
      ?? contactId,
    );
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

export const loadProjectRunnerMembers = async (
  client: RunnerProjectContactsClient,
  projectId: string,
): Promise<ProjectRunnerMember[]> => {
  const rows = await loadProjectRunnerContactRows(client, projectId);
  return normalizeProjectRunnerMembers(rows);
};

export const loadProjectRunnerContactRows = async (
  client: RunnerProjectContactsClient,
  projectId: string,
): Promise<ProjectContactLinkResponse[]> => {
  const normalizedProjectId = readTrimmedString(projectId);
  if (!normalizedProjectId) {
    return [];
  }

  const cachedRows = getProjectRunnerContactRowsScopedCache(client)?.get(normalizedProjectId);
  if (cachedRows && !cachedRows.stale) {
    return cachedRows.rows;
  }

  return withClientInflight(projectRunnerContactRowsInflight, client, normalizedProjectId, async () => {
    const rows = await client.listProjectContacts(normalizedProjectId, { limit: 500, offset: 0 });
    setProjectRunnerContactRowsCacheEntry(client, normalizedProjectId, {
      rows,
      stale: false,
    });
    return rows;
  });
};

export const getProjectRunnerContactRowsSnapshot = (
  client: RunnerProjectContactsClient,
  projectId: string,
): ProjectContactLinkResponse[] | null => {
  const normalizedProjectId = readTrimmedString(projectId);
  if (!normalizedProjectId) {
    return null;
  }
  return getProjectRunnerContactRowsScopedCache(client)?.get(normalizedProjectId)?.rows || null;
};

export const upsertProjectRunnerContactRow = (
  client: RunnerProjectContactsClient,
  projectId: string,
  row: ProjectContactLinkResponse | null | undefined,
): ProjectContactLinkResponse[] | null => {
  const normalizedProjectId = readTrimmedString(projectId);
  const normalizedContactId = readProjectRunnerContactId(row);
  if (!normalizedProjectId || !normalizedContactId) {
    return null;
  }

  const scopedCache = getProjectRunnerContactRowsScopedCache(client);
  const cached = scopedCache?.get(normalizedProjectId);
  if (!scopedCache || !cached) {
    return null;
  }

  const nextRows = [
    row as ProjectContactLinkResponse,
    ...cached.rows.filter((item) => readProjectRunnerContactId(item) !== normalizedContactId),
  ];

  return setProjectRunnerContactRowsCacheEntry(client, normalizedProjectId, {
    rows: nextRows,
    stale: cached.stale,
  });
};

export const removeProjectRunnerContactRow = (
  client: RunnerProjectContactsClient,
  projectId: string,
  contactId: string,
): ProjectContactLinkResponse[] | null => {
  const normalizedProjectId = readTrimmedString(projectId);
  const normalizedContactId = readTrimmedString(contactId);
  if (!normalizedProjectId || !normalizedContactId) {
    return null;
  }

  const scopedCache = getProjectRunnerContactRowsScopedCache(client);
  const cached = scopedCache?.get(normalizedProjectId);
  if (!scopedCache || !cached) {
    return null;
  }

  const nextRows = cached.rows.filter((item) => readProjectRunnerContactId(item) !== normalizedContactId);

  return setProjectRunnerContactRowsCacheEntry(client, normalizedProjectId, {
    rows: nextRows,
    stale: cached.stale,
  });
};

export const markProjectRunnerContactRowsStale = (
  client: RunnerProjectContactsClient,
  projectId: string,
): void => {
  const normalizedProjectId = readTrimmedString(projectId);
  if (!normalizedProjectId) {
    return;
  }
  const scopedCache = getProjectRunnerContactRowsScopedCache(client);
  const cached = scopedCache?.get(normalizedProjectId);
  if (!scopedCache || !cached) {
    return;
  }
  scopedCache.set(normalizedProjectId, {
    ...cached,
    stale: true,
  });
};

export const hasProjectRunnerScript = async (
  client: RunnerFilesystemClient,
  rootPath: string,
): Promise<boolean> => {
  const safeRoot = normalizeProjectRunnerRootPath(rootPath);
  if (!safeRoot) {
    return false;
  }

  return withClientInflight(projectRunnerScriptInflight, client, safeRoot, async () => {
    const rootList = await client.listFsEntries(safeRoot);
    const rootEntries = Array.isArray(rootList?.entries)
      ? rootList.entries.map((entry) => normalizeFsEntry(entry))
      : [];
    const runnerDirEntry = rootEntries.find((entry) => entry.isDir && entry.name === RUNNER_SCRIPT_DIR) || null;
    if (!runnerDirEntry?.path) {
      return false;
    }

    try {
      const runnerList = await client.listFsEntries(runnerDirEntry.path);
      const runnerEntries = Array.isArray(runnerList?.entries)
        ? runnerList.entries.map((entry) => normalizeFsEntry(entry))
        : [];
      return runnerEntries.some((entry) => !entry.isDir && entry.name === RUNNER_SCRIPT_FILE);
    } catch {
      return false;
    }
  });
};

export const resolveProjectRuntimeTerminal = (
  terminals: Terminal[],
  projectId: string,
): ProjectRuntimeTerminalSelection => {
  const related = terminals
    .filter((terminal) => String(terminal?.projectId || '') === projectId && terminal?.status === 'running')
    .sort((left, right) => {
      const leftTime = new Date(left?.lastActiveAt || 0).getTime();
      const rightTime = new Date(right?.lastActiveAt || 0).getTime();
      return rightTime - leftTime;
    });

  const busyTerminal = related.find((terminal) => Boolean(terminal?.busy)) || null;
  return {
    busyTerminal,
    activeTerminal: busyTerminal || related[0] || null,
  };
};

export const readProjectRunnerDispatchTarget = (
  value: TerminalDispatchResponse | unknown,
): {
  terminalId: string;
  terminalName: string;
} => {
  const record = asRecord(value) || {};
  const terminalId = readTrimmedString(readValue(record, 'terminal_id') ?? readValue(record, 'terminalId'));
  const terminalName = readTrimmedString(
    readValue(record, 'terminal_name')
    ?? readValue(record, 'terminalName')
    ?? terminalId,
  );

  return {
    terminalId,
    terminalName: terminalName || terminalId,
  };
};

export const buildProjectRunnerTarget = (
  rootPath: string,
): ProjectRunTarget => ({
  id: 'project_runner_start',
  label: 'project_runner.sh start',
  kind: 'script',
  cwd: rootPath,
  command: RUNNER_START_COMMAND,
  source: 'script',
  confidence: 1,
  isDefault: true,
});

export const buildProjectRunnerGenerationPrompt = (rootPath: string): string => [
  `你是项目运行脚本生成助手。请在项目根目录 ${rootPath} 下创建文件 ${RUNNER_SCRIPT_REL_PATH}。`,
  '',
  '目标：',
  '1) 生成一个 bash 脚本，支持参数 start / stop / restart。',
  '2) start: 启动当前项目下所有可启动服务（前端、后端、worker 等都包含，能启动的都要启动）。',
  '3) stop: 停止 start 启动的全部进程（优先使用 pid 文件，避免误杀非本脚本启动进程）。',
  '4) restart: 等价于 stop + start。',
  `5) 所有服务日志必须写入 ${rootPath}/${RUNNER_LOG_DIR_REL_PATH}/。`,
  '',
  '强制要求：',
  '1) 先读取项目关键文件（如 package.json / pyproject.toml / Cargo.toml / go.mod / pom.xml 等）再决策。',
  '2) 可使用终端工具做必要探测（如命令是否存在）。',
  '3) 脚本必须可执行（#!/usr/bin/env bash，set -euo pipefail）。',
  `4) 必须创建日志目录 ${rootPath}/${RUNNER_LOG_DIR_REL_PATH}/，并按服务拆分日志文件（例如 frontend.log、backend.log）。`,
  '5) 若无法确定某服务启动命令，要在注释与日志里明确标记该服务待人工补充，但其他可启动服务仍需正常启动。',
  '6) 禁止把后端端口写死为 3997 或其它固定值；每个服务启动前必须检测端口是否可用，不可用时自动选择可用端口。',
  '7) 必须把实际使用端口写入 project_runner/runtime/ports.env，重启时优先复用该文件中的端口配置。',
  '8) stop 只能按本脚本维护的 pid 文件停止，不允许按端口全局 kill，避免误伤其他项目服务。',
  `9) 完成后请回复：脚本已生成: ${RUNNER_SCRIPT_REL_PATH}`,
].join('\n');
