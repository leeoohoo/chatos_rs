import type {
  GitBranchesResult,
  GitClientInfo,
  GitStatusResult,
  GitSummary,
} from '../../../types';
import type { ProjectGitApiClient } from './projectGitTypes';

interface GitClientInfoCacheEntry {
  clientInfo: GitClientInfo;
}

interface GitSummaryCacheEntry {
  summary: GitSummary;
  stale: boolean;
}

interface GitDetailsCacheEntry {
  branches: GitBranchesResult;
  status: GitStatusResult;
  stale: boolean;
}

interface ProjectGitClientCacheState {
  clientInfo: GitClientInfoCacheEntry | null;
  clientInfoInflight: Promise<GitClientInfo> | null;
  summaryCache: Map<string, GitSummaryCacheEntry>;
  summaryInflight: Map<string, Promise<GitSummary>>;
  detailsCache: Map<string, GitDetailsCacheEntry>;
  detailsInflight: Map<string, Promise<{ branches: GitBranchesResult; status: GitStatusResult }>>;
}

const projectGitCaches = new WeakMap<ProjectGitApiClient, ProjectGitClientCacheState>();

const normalizeProjectRoot = (projectRoot: string): string => String(projectRoot || '').trim();

const getOrCreateProjectGitCacheState = (
  client: ProjectGitApiClient,
): ProjectGitClientCacheState => {
  const existing = projectGitCaches.get(client);
  if (existing) {
    return existing;
  }
  const next: ProjectGitClientCacheState = {
    clientInfo: null,
    clientInfoInflight: null,
    summaryCache: new Map(),
    summaryInflight: new Map(),
    detailsCache: new Map(),
    detailsInflight: new Map(),
  };
  projectGitCaches.set(client, next);
  return next;
};

export const peekGitClientInfoCacheEntry = (
  client: ProjectGitApiClient,
): GitClientInfoCacheEntry | null => {
  return getOrCreateProjectGitCacheState(client).clientInfo;
};

export const setGitClientInfoCacheEntry = (
  client: ProjectGitApiClient,
  clientInfo: GitClientInfo,
): void => {
  getOrCreateProjectGitCacheState(client).clientInfo = { clientInfo };
};

export const getGitClientInfoInflight = (
  client: ProjectGitApiClient,
): Promise<GitClientInfo> | null => {
  return getOrCreateProjectGitCacheState(client).clientInfoInflight;
};

export const setGitClientInfoInflight = (
  client: ProjectGitApiClient,
  inflight: Promise<GitClientInfo> | null,
): void => {
  getOrCreateProjectGitCacheState(client).clientInfoInflight = inflight;
};

export const peekGitSummaryCacheEntry = (
  client: ProjectGitApiClient,
  projectRoot: string,
): GitSummaryCacheEntry | null => {
  const normalizedProjectRoot = normalizeProjectRoot(projectRoot);
  if (!normalizedProjectRoot) {
    return null;
  }
  return getOrCreateProjectGitCacheState(client).summaryCache.get(normalizedProjectRoot) || null;
};

export const setGitSummaryCacheEntry = (
  client: ProjectGitApiClient,
  projectRoot: string,
  summary: GitSummary,
): void => {
  const normalizedProjectRoot = normalizeProjectRoot(projectRoot);
  if (!normalizedProjectRoot) {
    return;
  }
  getOrCreateProjectGitCacheState(client).summaryCache.set(normalizedProjectRoot, {
    summary,
    stale: false,
  });
};

export const markGitSummaryCacheStale = (
  client: ProjectGitApiClient,
  projectRoot: string,
): void => {
  const normalizedProjectRoot = normalizeProjectRoot(projectRoot);
  if (!normalizedProjectRoot) {
    return;
  }
  const cacheState = getOrCreateProjectGitCacheState(client);
  const cached = cacheState.summaryCache.get(normalizedProjectRoot);
  if (!cached) {
    return;
  }
  cacheState.summaryCache.set(normalizedProjectRoot, {
    ...cached,
    stale: true,
  });
};

export const getGitSummaryInflight = (
  client: ProjectGitApiClient,
  projectRoot: string,
): Promise<GitSummary> | null => {
  const normalizedProjectRoot = normalizeProjectRoot(projectRoot);
  if (!normalizedProjectRoot) {
    return null;
  }
  return getOrCreateProjectGitCacheState(client).summaryInflight.get(normalizedProjectRoot) || null;
};

export const setGitSummaryInflight = (
  client: ProjectGitApiClient,
  projectRoot: string,
  inflight: Promise<GitSummary> | null,
): void => {
  const normalizedProjectRoot = normalizeProjectRoot(projectRoot);
  if (!normalizedProjectRoot) {
    return;
  }
  const cacheState = getOrCreateProjectGitCacheState(client);
  if (inflight) {
    cacheState.summaryInflight.set(normalizedProjectRoot, inflight);
    return;
  }
  cacheState.summaryInflight.delete(normalizedProjectRoot);
};

export const peekGitDetailsCacheEntry = (
  client: ProjectGitApiClient,
  projectRoot: string,
): GitDetailsCacheEntry | null => {
  const normalizedProjectRoot = normalizeProjectRoot(projectRoot);
  if (!normalizedProjectRoot) {
    return null;
  }
  return getOrCreateProjectGitCacheState(client).detailsCache.get(normalizedProjectRoot) || null;
};

export const setGitDetailsCacheEntry = (
  client: ProjectGitApiClient,
  projectRoot: string,
  details: { branches: GitBranchesResult; status: GitStatusResult },
): void => {
  const normalizedProjectRoot = normalizeProjectRoot(projectRoot);
  if (!normalizedProjectRoot) {
    return;
  }
  getOrCreateProjectGitCacheState(client).detailsCache.set(normalizedProjectRoot, {
    ...details,
    stale: false,
  });
};

export const markGitDetailsCacheStale = (
  client: ProjectGitApiClient,
  projectRoot: string,
): void => {
  const normalizedProjectRoot = normalizeProjectRoot(projectRoot);
  if (!normalizedProjectRoot) {
    return;
  }
  const cacheState = getOrCreateProjectGitCacheState(client);
  const cached = cacheState.detailsCache.get(normalizedProjectRoot);
  if (!cached) {
    return;
  }
  cacheState.detailsCache.set(normalizedProjectRoot, {
    ...cached,
    stale: true,
  });
};

export const getGitDetailsInflight = (
  client: ProjectGitApiClient,
  projectRoot: string,
): Promise<{ branches: GitBranchesResult; status: GitStatusResult }> | null => {
  const normalizedProjectRoot = normalizeProjectRoot(projectRoot);
  if (!normalizedProjectRoot) {
    return null;
  }
  return getOrCreateProjectGitCacheState(client).detailsInflight.get(normalizedProjectRoot) || null;
};

export const setGitDetailsInflight = (
  client: ProjectGitApiClient,
  projectRoot: string,
  inflight: Promise<{ branches: GitBranchesResult; status: GitStatusResult }> | null,
): void => {
  const normalizedProjectRoot = normalizeProjectRoot(projectRoot);
  if (!normalizedProjectRoot) {
    return;
  }
  const cacheState = getOrCreateProjectGitCacheState(client);
  if (inflight) {
    cacheState.detailsInflight.set(normalizedProjectRoot, inflight);
    return;
  }
  cacheState.detailsInflight.delete(normalizedProjectRoot);
};
