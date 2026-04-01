import { useCallback } from 'react';

import type { Project, RemoteConnection, Session, Terminal } from '../../types';
import { resolveRemoteConnectionErrorFeedback } from '../../lib/api/remoteConnectionErrors';
import { readSessionRuntimeFromMetadata } from '../../lib/store/helpers/sessionRuntime';
import type { ConfirmDialogOptions } from '../../hooks/useConfirmDialog';
import { getSessionStatus } from './helpers';
import type { ContactItem } from './types';

interface UseSessionListDeleteActionsParams {
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
  loadContactsAction: () => Promise<any>;
  clearCachedSessionIdsForContact: (contactId: string) => string[];
  showConfirmDialog: (options: ConfirmDialogOptions) => void;
}

export const useSessionListDeleteActions = ({
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
  showConfirmDialog,
}: UseSessionListDeleteActionsParams) => {
  const handleArchiveProject = useCallback(async (projectId: string) => {
    const project = projects.find((p: Project) => p.id === projectId);
    showConfirmDialog({
      title: '归档确认',
      message: `确定要归档项目 "${project?.name || 'Untitled'}" 吗？归档后将从项目列表移除。`,
      confirmText: '归档',
      cancelText: '取消',
      type: 'danger',
      onConfirm: async () => {
        try {
          await deleteProject(projectId);
        } catch (error) {
          console.error('Failed to archive project:', error);
        }
      }
    });
  }, [deleteProject, projects, showConfirmDialog]);

  const handleDeleteTerminal = useCallback(async (terminalId: string) => {
    const terminal = terminals.find((t: Terminal) => t.id === terminalId);
    showConfirmDialog({
      title: '删除确认',
      message: `确定要删除终端 "${terminal?.name || 'Untitled'}" 吗？此操作无法撤销。`,
      confirmText: '删除',
      cancelText: '取消',
      type: 'danger',
      onConfirm: async () => {
        try {
          await deleteTerminal(terminalId);
        } catch (error) {
          console.error('Failed to delete terminal:', error);
        }
      }
    });
  }, [deleteTerminal, showConfirmDialog, terminals]);

  const handleDeleteRemoteConnection = useCallback(async (connectionId: string) => {
    const connection = remoteConnections.find((item: RemoteConnection) => item.id === connectionId);
    showConfirmDialog({
      title: '删除确认',
      message: `确定要删除远端连接 "${connection?.name || 'Untitled'}" 吗？此操作无法撤销。`,
      confirmText: '删除',
      cancelText: '取消',
      type: 'danger',
      onConfirm: async () => {
        try {
          await deleteRemoteConnection(connectionId);
        } catch (error) {
          const feedback = resolveRemoteConnectionErrorFeedback(error, '删除远端连接失败');
          showConfirmDialog({
            title: '删除失败',
            message: feedback.message,
            description: feedback.message,
            detailsTitle: '建议操作',
            detailsLines: feedback.action ? [feedback.action] : undefined,
            confirmText: '知道了',
            cancelText: '关闭',
            type: 'info',
          });
        }
      }
    });
  }, [deleteRemoteConnection, remoteConnections, showConfirmDialog]);

  const handleDeleteSession = useCallback(async (sessionId: string) => {
    const session = displaySessions.find((s: Session) => s.id === sessionId);
    if (!session || getSessionStatus(session) !== 'active') {
      return;
    }
    const runtime = readSessionRuntimeFromMetadata(session.metadata);
    const contactAgentId = runtime?.contactAgentId || null;
    showConfirmDialog({
      title: '删除联系人',
      message: `确定要删除联系人 "${session.title || 'Untitled'}" 吗？`,
      confirmText: '删除',
      cancelText: '取消',
      type: 'danger',
      onConfirm: async () => {
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
      }
    });
  }, [
    clearCachedSessionIdsForContact,
    contacts,
    currentSession?.id,
    deleteContactAction,
    deleteSession,
    displaySessions,
    loadContactsAction,
    showConfirmDialog,
  ]);

  return {
    handleArchiveProject,
    handleDeleteTerminal,
    handleDeleteRemoteConnection,
    handleDeleteSession,
  };
};
