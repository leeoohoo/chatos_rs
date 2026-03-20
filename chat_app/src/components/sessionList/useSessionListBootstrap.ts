import { useEffect, useRef } from 'react';

interface UseSessionListBootstrapOptions {
  loadProjects: () => Promise<unknown> | unknown;
  loadAgents: () => Promise<unknown> | unknown;
  loadContacts: () => Promise<unknown> | unknown;
  loadTerminals: () => Promise<unknown> | unknown;
  loadRemoteConnections: () => Promise<unknown> | unknown;
  isCollapsed: boolean;
  terminalsExpanded: boolean;
  remoteExpanded: boolean;
}

export const useSessionListBootstrap = ({
  loadProjects,
  loadAgents,
  loadContacts,
  loadTerminals,
  loadRemoteConnections,
  isCollapsed,
  terminalsExpanded,
  remoteExpanded,
}: UseSessionListBootstrapOptions): void => {
  const didLoadProjectsRef = useRef(false);
  const didLoadAgentsRef = useRef(false);
  const didLoadContactsRef = useRef(false);
  const didLoadTerminalsRef = useRef(false);
  const didLoadRemoteRef = useRef(false);

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
    if (didLoadTerminalsRef.current) return;
    didLoadTerminalsRef.current = true;
    void loadTerminals();
  }, [loadTerminals]);

  useEffect(() => {
    if (didLoadRemoteRef.current) return;
    didLoadRemoteRef.current = true;
    void loadRemoteConnections();
  }, [loadRemoteConnections]);

  useEffect(() => {
    if (isCollapsed || !terminalsExpanded) return;
    const timer = window.setInterval(() => {
      void loadTerminals();
    }, 10000);
    return () => window.clearInterval(timer);
  }, [isCollapsed, terminalsExpanded, loadTerminals]);

  useEffect(() => {
    if (isCollapsed || !remoteExpanded) return;
    const timer = window.setInterval(() => {
      void loadRemoteConnections();
    }, 12000);
    return () => window.clearInterval(timer);
  }, [isCollapsed, remoteExpanded, loadRemoteConnections]);
};
