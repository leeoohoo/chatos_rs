// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type {
  GitBranchesResult,
  GitClientInfo,
  GitStatusResult,
  GitSummary,
} from '../../../types';
import type { ProjectGitApiClient } from './projectGitTypes';

type GitDetailsResult = {
  branches: GitBranchesResult;
  status: GitStatusResult;
};

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
  detailsInflight: Map<string, Promise<GitDetailsResult>>;
}

const projectGitCaches = new WeakMap<ProjectGitApiClient, ProjectGitClientCacheState>();

const normalizeProjectRootKey = (projectRoot: string): string | null => {
  const normalized = String(projectRoot || '').trim();
  return normalized || null;
};

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

const peekProjectRootCacheEntry = <T>(
  cache: Map<string, T>,
  projectRoot: string,
): T | null => {
  const normalizedProjectRoot = normalizeProjectRootKey(projectRoot);
  if (!normalizedProjectRoot) {
    return null;
  }
  return cache.get(normalizedProjectRoot) || null;
};

const setProjectRootCacheEntry = <T>(
  cache: Map<string, T>,
  projectRoot: string,
  value: T,
): void => {
  const normalizedProjectRoot = normalizeProjectRootKey(projectRoot);
  if (!normalizedProjectRoot) {
    return;
  }
  cache.set(normalizedProjectRoot, value);
};

const updateProjectRootCacheEntry = <T>(
  cache: Map<string, T>,
  projectRoot: string,
  updater: (cached: T) => T,
): void => {
  const normalizedProjectRoot = normalizeProjectRootKey(projectRoot);
  if (!normalizedProjectRoot) {
    return;
  }
  const cached = cache.get(normalizedProjectRoot);
  if (!cached) {
    return;
  }
  cache.set(normalizedProjectRoot, updater(cached));
};

const getProjectRootInflight = <T>(
  inflightByRoot: Map<string, Promise<T>>,
  projectRoot: string,
): Promise<T> | null => {
  const normalizedProjectRoot = normalizeProjectRootKey(projectRoot);
  if (!normalizedProjectRoot) {
    return null;
  }
  return inflightByRoot.get(normalizedProjectRoot) || null;
};

const setProjectRootInflight = <T>(
  inflightByRoot: Map<string, Promise<T>>,
  projectRoot: string,
  inflight: Promise<T> | null,
): void => {
  const normalizedProjectRoot = normalizeProjectRootKey(projectRoot);
  if (!normalizedProjectRoot) {
    return;
  }
  if (inflight) {
    inflightByRoot.set(normalizedProjectRoot, inflight);
    return;
  }
  inflightByRoot.delete(normalizedProjectRoot);
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
  return peekProjectRootCacheEntry(
    getOrCreateProjectGitCacheState(client).summaryCache,
    projectRoot,
  );
};

export const setGitSummaryCacheEntry = (
  client: ProjectGitApiClient,
  projectRoot: string,
  summary: GitSummary,
): void => {
  setProjectRootCacheEntry(getOrCreateProjectGitCacheState(client).summaryCache, projectRoot, {
    summary,
    stale: false,
  });
};

export const markGitSummaryCacheStale = (
  client: ProjectGitApiClient,
  projectRoot: string,
): void => {
  updateProjectRootCacheEntry(
    getOrCreateProjectGitCacheState(client).summaryCache,
    projectRoot,
    (cached) => ({
    ...cached,
    stale: true,
    }),
  );
};

export const getGitSummaryInflight = (
  client: ProjectGitApiClient,
  projectRoot: string,
): Promise<GitSummary> | null => {
  return getProjectRootInflight(
    getOrCreateProjectGitCacheState(client).summaryInflight,
    projectRoot,
  );
};

export const setGitSummaryInflight = (
  client: ProjectGitApiClient,
  projectRoot: string,
  inflight: Promise<GitSummary> | null,
): void => {
  setProjectRootInflight(
    getOrCreateProjectGitCacheState(client).summaryInflight,
    projectRoot,
    inflight,
  );
};

export const peekGitDetailsCacheEntry = (
  client: ProjectGitApiClient,
  projectRoot: string,
): GitDetailsCacheEntry | null => {
  return peekProjectRootCacheEntry(
    getOrCreateProjectGitCacheState(client).detailsCache,
    projectRoot,
  );
};

export const setGitDetailsCacheEntry = (
  client: ProjectGitApiClient,
  projectRoot: string,
  details: GitDetailsResult,
): void => {
  setProjectRootCacheEntry(getOrCreateProjectGitCacheState(client).detailsCache, projectRoot, {
    ...details,
    stale: false,
  });
};

export const markGitDetailsCacheStale = (
  client: ProjectGitApiClient,
  projectRoot: string,
): void => {
  updateProjectRootCacheEntry(
    getOrCreateProjectGitCacheState(client).detailsCache,
    projectRoot,
    (cached) => ({
    ...cached,
    stale: true,
    }),
  );
};

export const getGitDetailsInflight = (
  client: ProjectGitApiClient,
  projectRoot: string,
): Promise<GitDetailsResult> | null => {
  return getProjectRootInflight(
    getOrCreateProjectGitCacheState(client).detailsInflight,
    projectRoot,
  );
};

export const setGitDetailsInflight = (
  client: ProjectGitApiClient,
  projectRoot: string,
  inflight: Promise<GitDetailsResult> | null,
): void => {
  setProjectRootInflight(
    getOrCreateProjectGitCacheState(client).detailsInflight,
    projectRoot,
    inflight,
  );
};
