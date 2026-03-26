import { useMemo } from 'react';

import { readSessionRuntimeFromMetadata } from '../../lib/store/helpers/sessionRuntime';

interface UseSessionHeaderMetaParams {
  currentSession: any | null;
  contacts: any[];
  activePanel: string;
  currentProject: any;
  currentTerminal: any;
  currentRemoteConnection: any;
}

export const useSessionHeaderMeta = ({
  currentSession,
  contacts,
  activePanel,
  currentProject,
  currentTerminal,
  currentRemoteConnection,
}: UseSessionHeaderMetaParams) => {
  const currentContactName = useMemo(() => {
    if (!currentSession) {
      return '';
    }
    const runtime = readSessionRuntimeFromMetadata((currentSession as any).metadata);
    const contactId = typeof runtime?.contactId === 'string' ? runtime.contactId.trim() : '';
    const contactAgentId = typeof runtime?.contactAgentId === 'string' ? runtime.contactAgentId.trim() : '';
    if (!contactId && !contactAgentId) {
      return '';
    }
    const matched = (contacts || []).find((item: any) => {
      if (contactId && typeof item?.id === 'string' && item.id === contactId) {
        return true;
      }
      if (contactAgentId && typeof item?.agentId === 'string' && item.agentId === contactAgentId) {
        return true;
      }
      return false;
    });
    return matched?.name || '';
  }, [contacts, currentSession]);

  const currentContactId = useMemo(() => {
    if (!currentSession) {
      return '';
    }
    const runtime = readSessionRuntimeFromMetadata((currentSession as any).metadata);
    const directContactId = typeof runtime?.contactId === 'string' ? runtime.contactId.trim() : '';
    if (directContactId) {
      return directContactId;
    }
    const contactAgentId = typeof runtime?.contactAgentId === 'string' ? runtime.contactAgentId.trim() : '';
    if (!contactAgentId) {
      return '';
    }
    const matched = (contacts || []).find((item: any) => item?.agentId === contactAgentId);
    return typeof matched?.id === 'string' ? matched.id : '';
  }, [contacts, currentSession]);

  const headerTitle = activePanel === 'project'
    ? (currentProject?.name || '项目')
    : activePanel === 'terminal'
      ? (currentTerminal?.name || '终端')
      : activePanel === 'remote_terminal' || activePanel === 'remote_sftp'
        ? (currentRemoteConnection?.name || '远端连接')
      : (currentContactName || currentSession?.title || '');

  return {
    currentContactName,
    currentContactId,
    headerTitle,
  };
};
