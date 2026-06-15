import { useEffect, useRef } from 'react';

interface UseSessionListBootstrapOptions {
  loadSessions: (options?: { silent?: boolean }) => Promise<unknown> | unknown;
  loadProjects: () => Promise<unknown> | unknown;
  loadAgents: () => Promise<unknown> | unknown;
  loadContacts: () => Promise<unknown> | unknown;
  loadTerminals: (options?: { force?: boolean }) => Promise<unknown> | unknown;
  loadRemoteConnections: () => Promise<unknown> | unknown;
  isCollapsed: boolean;
  terminalsEnabled: boolean;
  terminalsExpanded: boolean;
  remoteExpanded: boolean;
}

export const useSessionListBootstrap = ({
  loadSessions,
  loadProjects,
  loadAgents,
  loadContacts,
  loadTerminals,
  loadRemoteConnections,
  isCollapsed,
  terminalsEnabled,
  terminalsExpanded,
  remoteExpanded,
}: UseSessionListBootstrapOptions): void => {
  const didLoadSessionsRef = useRef(false);
  const didLoadProjectsRef = useRef(false);
  const didLoadAgentsRef = useRef(false);
  const didLoadContactsRef = useRef(false);
  const didLoadTerminalsRef = useRef(false);
  const didLoadRemoteRef = useRef(false);

  useEffect(() => {
    if (didLoadSessionsRef.current) return;
    didLoadSessionsRef.current = true;
    void Promise.resolve(loadSessions({ silent: true })).catch((error) => {
      console.error('Failed to load sessions:', error);
    });
  }, [loadSessions]);

  useEffect(() => {
    if (didLoadProjectsRef.current) return;
    didLoadProjectsRef.current = true;
    void loadProjects();
  }, [loadProjects]);

  useEffect(() => {
    if (didLoadAgentsRef.current) return;
    didLoadAgentsRef.current = true;
    void loadAgents();
  }, [loadAgents]);

  useEffect(() => {
    if (didLoadContactsRef.current) return;
    didLoadContactsRef.current = true;
    void Promise.resolve(loadContacts()).catch((error) => {
      console.error('Failed to load contacts:', error);
    });
  }, [loadContacts]);

  useEffect(() => {
    if (!terminalsEnabled) return;
    if (didLoadTerminalsRef.current) return;
    didLoadTerminalsRef.current = true;
    void loadTerminals();
  }, [loadTerminals, terminalsEnabled]);

  useEffect(() => {
    if (didLoadRemoteRef.current) return;
    didLoadRemoteRef.current = true;
    void loadRemoteConnections();
  }, [loadRemoteConnections]);

  void isCollapsed;
  void terminalsEnabled;
  void terminalsExpanded;
  void remoteExpanded;
};
