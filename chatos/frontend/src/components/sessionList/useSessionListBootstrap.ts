// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useEffect, useRef } from 'react';

interface UseSessionListBootstrapOptions {
  loadSessions: (options?: { silent?: boolean }) => Promise<unknown> | unknown;
  loadProjects: (options?: { force?: boolean; throwOnError?: boolean }) => Promise<unknown> | unknown;
  loadAgents: () => Promise<unknown> | unknown;
  loadContacts: (options?: { force?: boolean; throwOnError?: boolean }) => Promise<unknown> | unknown;
  loadTerminals: (options?: { force?: boolean }) => Promise<unknown> | unknown;
  loadRemoteConnections: () => Promise<unknown> | unknown;
  isCollapsed: boolean;
  terminalsEnabled: boolean;
  remoteEnabled: boolean;
  terminalsExpanded: boolean;
  remoteExpanded: boolean;
}

const BOOTSTRAP_RETRY_DELAYS_MS = [
  0,
  250,
  750,
  1_500,
  3_000,
  5_000,
  5_000,
  5_000,
  5_000,
  5_000,
  5_000,
  5_000,
  5_000,
] as const;

const waitForRetry = (delayMs: number): Promise<void> => (
  delayMs > 0
    ? new Promise((resolve) => window.setTimeout(resolve, delayMs))
    : Promise.resolve()
);

const loadBootstrapResource = async (
  load: () => Promise<unknown> | unknown,
): Promise<void> => {
  let lastError: unknown = null;
  for (const delayMs of BOOTSTRAP_RETRY_DELAYS_MS) {
    await waitForRetry(delayMs);
    try {
      await Promise.resolve(load());
      return;
    } catch (error) {
      lastError = error;
    }
  }
  throw lastError instanceof Error ? lastError : new Error('Bootstrap resource load failed');
};

export const useSessionListBootstrap = ({
  loadSessions,
  loadProjects,
  loadAgents,
  loadContacts,
  loadTerminals,
  loadRemoteConnections,
  isCollapsed,
  terminalsEnabled,
  remoteEnabled,
  terminalsExpanded,
  remoteExpanded,
}: UseSessionListBootstrapOptions): void => {
  const didLoadSessionsRef = useRef(false);
  const didLoadProjectsRef = useRef(false);
  const didLoadAgentsRef = useRef(false);
  const didLoadTerminalsRef = useRef(false);
  const didLoadRemoteRef = useRef(false);

  useEffect(() => {
    if (didLoadProjectsRef.current || didLoadSessionsRef.current) return;
    didLoadProjectsRef.current = true;
    didLoadSessionsRef.current = true;
    void (async () => {
      try {
        await loadBootstrapResource(
          () => loadProjects({ force: true, throwOnError: true }),
        );
      } catch (error) {
        console.error('Failed to load projects:', error);
        return;
      }
      try {
        await loadBootstrapResource(
          () => loadContacts({ force: true, throwOnError: true }),
        );
      } catch (error) {
        console.error('Failed to load contacts:', error);
        return;
      }
      try {
        await loadSessions({ silent: true });
      } catch (error) {
        console.error('Failed to load sessions:', error);
      }
    })();
  }, [loadContacts, loadProjects, loadSessions]);

  useEffect(() => {
    if (didLoadAgentsRef.current) return;
    didLoadAgentsRef.current = true;
    void loadAgents();
  }, [loadAgents]);

  useEffect(() => {
    if (!terminalsEnabled) return;
    if (didLoadTerminalsRef.current) return;
    didLoadTerminalsRef.current = true;
    void loadTerminals();
  }, [loadTerminals, terminalsEnabled]);

  useEffect(() => {
    if (!remoteEnabled) return;
    if (didLoadRemoteRef.current) return;
    didLoadRemoteRef.current = true;
    void loadRemoteConnections();
  }, [loadRemoteConnections, remoteEnabled]);

  void isCollapsed;
  void terminalsEnabled;
  void remoteEnabled;
  void terminalsExpanded;
  void remoteExpanded;
};
