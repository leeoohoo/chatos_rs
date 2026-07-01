// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useCallback } from 'react';

import type { TranslateFn } from '../../i18n/I18nProvider';
import type { Project, RemoteConnection, Session, Terminal } from '../../types';
import { resolveRemoteConnectionErrorFeedback } from '../../lib/api/remoteConnectionErrors';
import { readSessionRuntimeFromMetadata } from '../../lib/store/helpers/sessionRuntime';
import type { DialogAlertOptions, DialogConfirmOptions } from '../ui/DialogProvider';
import { getSessionStatus, translateSessionListMessage } from './helpers';
import type { ContactItem } from './types';

interface UseSessionListDeleteActionsParams {
  t?: TranslateFn;
  projects: Project[];
  terminals: Terminal[];
  remoteConnections: RemoteConnection[];
  displaySessions: Session[];
  contacts: ContactItem[];
  currentSession: Session | null;
  deleteProject: (id: string) => Promise<void>;
  deleteTerminal: (id: string) => Promise<void>;
  deleteRemoteConnection: (id: string) => Promise<void>;
  deleteSession: (id: string) => Promise<void>;
  deleteContactAction: (id: string) => Promise<void>;
  loadContactsAction: () => Promise<unknown>;
  clearCachedSessionIdsForContact: (contactId: string) => string[];
  confirmDialog: (options: DialogConfirmOptions) => Promise<boolean>;
  alertDialog: (options: DialogAlertOptions) => Promise<void>;
}

export const useSessionListDeleteActions = ({
  t,
  projects,
  terminals,
  remoteConnections,
  displaySessions,
  contacts,
  currentSession,
  deleteProject,
  deleteTerminal,
  deleteRemoteConnection,
  deleteSession,
  deleteContactAction,
  loadContactsAction,
  clearCachedSessionIdsForContact,
  confirmDialog,
  alertDialog,
}: UseSessionListDeleteActionsParams) => {
  const handleArchiveProject = useCallback(async (projectId: string) => {
    const project = projects.find((p: Project) => p.id === projectId);
    const projectName = project?.name || 'Untitled';
    const confirmed = await confirmDialog({
      title: translateSessionListMessage(t, 'sessionList.confirm.archiveProjectTitle'),
      message: translateSessionListMessage(t, 'sessionList.confirm.archiveProjectMessage', { name: projectName }),
      confirmText: translateSessionListMessage(t, 'common.archive'),
      cancelText: translateSessionListMessage(t, 'common.cancel'),
      type: 'danger',
    });
    if (!confirmed) {
      return;
    }
    try {
      await deleteProject(projectId);
    } catch (error) {
      console.error('Failed to archive project:', error);
    }
  }, [confirmDialog, deleteProject, projects, t]);

  const handleDeleteTerminal = useCallback(async (terminalId: string) => {
    const terminal = terminals.find((t: Terminal) => t.id === terminalId);
    const terminalName = terminal?.name || 'Untitled';
    const confirmed = await confirmDialog({
      title: translateSessionListMessage(t, 'sessionList.confirm.deleteTitle'),
      message: translateSessionListMessage(t, 'sessionList.confirm.deleteTerminalMessage', { name: terminalName }),
      confirmText: translateSessionListMessage(t, 'common.delete'),
      cancelText: translateSessionListMessage(t, 'common.cancel'),
      type: 'danger',
    });
    if (!confirmed) {
      return;
    }
    try {
      await deleteTerminal(terminalId);
    } catch (error) {
      console.error('Failed to delete terminal:', error);
      const message = error instanceof Error ? error.message : translateSessionListMessage(t, 'sessionList.resource.error.deleteTerminalFailed');
      await alertDialog({
        title: translateSessionListMessage(t, 'sessionList.confirm.deleteFailedTitle'),
        message,
        confirmText: translateSessionListMessage(t, 'common.gotIt'),
        type: 'info',
      });
    }
  }, [alertDialog, confirmDialog, deleteTerminal, terminals, t]);

  const handleDeleteRemoteConnection = useCallback(async (connectionId: string) => {
    const connection = remoteConnections.find((item: RemoteConnection) => item.id === connectionId);
    const connectionName = connection?.name || 'Untitled';
    const confirmed = await confirmDialog({
      title: translateSessionListMessage(t, 'sessionList.confirm.deleteTitle'),
      message: translateSessionListMessage(t, 'sessionList.confirm.deleteRemoteMessage', { name: connectionName }),
      confirmText: translateSessionListMessage(t, 'common.delete'),
      cancelText: translateSessionListMessage(t, 'common.cancel'),
      type: 'danger',
    });
    if (!confirmed) {
      return;
    }
    try {
      await deleteRemoteConnection(connectionId);
    } catch (error) {
      const feedback = resolveRemoteConnectionErrorFeedback(
        error,
        translateSessionListMessage(t, 'remoteConnection.error.deleteFailed'),
      );
      await alertDialog({
        title: translateSessionListMessage(t, 'sessionList.confirm.deleteFailedTitle'),
        message: feedback.message,
        description: feedback.action || undefined,
        confirmText: translateSessionListMessage(t, 'common.gotIt'),
        type: 'info',
      });
    }
  }, [alertDialog, confirmDialog, deleteRemoteConnection, remoteConnections, t]);

  const handleDeleteSession = useCallback(async (sessionId: string) => {
    const session = displaySessions.find((s: Session) => s.id === sessionId);
    if (!session || getSessionStatus(session) !== 'active') {
      return;
    }
    const runtime = readSessionRuntimeFromMetadata(session.metadata);
    const contactAgentId = runtime?.contactAgentId || null;
    const sessionName = session.title || 'Untitled';
    const confirmed = await confirmDialog({
      title: translateSessionListMessage(t, 'sessionList.confirm.deleteContactTitle'),
      message: translateSessionListMessage(t, 'sessionList.confirm.deleteContactMessage', { name: sessionName }),
      confirmText: translateSessionListMessage(t, 'common.delete'),
      cancelText: translateSessionListMessage(t, 'common.cancel'),
      type: 'danger',
    });
    if (!confirmed) {
      return;
    }
    try {
      let resolvedContactId = runtime?.contactId || null;
      if (!resolvedContactId && contactAgentId) {
        const matched = contacts.find((item) => item.agentId === contactAgentId) || null;
        resolvedContactId = matched?.id || null;
      }
      if (resolvedContactId) {
        await deleteContactAction(resolvedContactId);
        const cachedSessionIds = clearCachedSessionIdsForContact(resolvedContactId);
        for (const cachedSessionId of cachedSessionIds) {
          if (currentSession?.id === cachedSessionId) {
            await deleteSession(cachedSessionId);
          }
        }
      }
      if (!sessionId.startsWith('contact-placeholder:')) {
        await deleteSession(sessionId);
      }
      if (resolvedContactId) {
        await loadContactsAction();
      }
    } catch (error) {
      console.error('Failed to delete session:', error);
    }
  }, [
    confirmDialog,
    clearCachedSessionIdsForContact,
    contacts,
    currentSession?.id,
    deleteContactAction,
    deleteSession,
    displaySessions,
    loadContactsAction,
    t,
  ]);

  return {
    handleArchiveProject,
    handleDeleteTerminal,
    handleDeleteRemoteConnection,
    handleDeleteSession,
  };
};
