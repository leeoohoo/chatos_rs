import type { ProjectRunState } from '../../types';
import type {
  ProjectContactLinkResponse,
  TerminalDispatchResponse,
} from '../api/client/types';
import { asRecord, readValue } from './normalizerUtils';

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
  origin?: 'dispatched' | 'discovered';
  exitCode?: number | null;
  exitReason?: string | null;
}

export interface ProjectRunnerBoundTerminal {
  terminalId: string;
  terminalName: string;
  cwd: string;
  running: boolean;
  busy: boolean;
  status: string;
}

type RunnerProjectContactsClient = {
  listProjectContacts: (
    projectId: string,
    paging?: { limit?: number; offset?: number },
  ) => Promise<ProjectContactLinkResponse[]>;
};

interface ProjectRunnerContactRowsCacheEntry {
  rows: ProjectContactLinkResponse[];
  stale: boolean;
}

const projectRunnerContactRowsInflight = new WeakMap<object, Map<string, Promise<ProjectContactLinkResponse[]>>>();
const projectRunnerContactRowsCache = new WeakMap<object, Map<string, ProjectRunnerContactRowsCacheEntry>>();

const readTrimmedString = (value: unknown): string => (
  typeof value === 'string' ? value.trim() : ''
);

const readProjectRunnerContactId = (value: ProjectContactLinkResponse | unknown): string => {
  const record = asRecord(value) || {};
  return readTrimmedString(readValue(record, 'contact_id') ?? readValue(record, 'contactId'));
};

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

export const syncProjectRunnerContactRows = (
  client: RunnerProjectContactsClient,
  projectId: string,
  rows: ProjectContactLinkResponse[] | null | undefined,
): ProjectContactLinkResponse[] | null => {
  const normalizedProjectId = readTrimmedString(projectId);
  if (!normalizedProjectId) {
    return null;
  }

  return setProjectRunnerContactRowsCacheEntry(client, normalizedProjectId, {
    rows: Array.isArray(rows) ? [...rows] : [],
    stale: false,
  });
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

export const readProjectRunnerBoundTerminal = (
  value: ProjectRunState | null | undefined,
): ProjectRunnerBoundTerminal | null => {
  if (!value?.terminalId) {
    return null;
  }
  return {
    terminalId: value.terminalId,
    terminalName: value.terminalName || value.terminalId,
    cwd: value.cwd || value.terminal?.cwd || '',
    running: Boolean(value.running),
    busy: Boolean(value.busy),
    status: value.status || 'idle',
  };
};
