import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import { normalizeProjectScopeId } from '../../../features/contactSession/sessionResolver';
import type { ProjectContactLinkResponse } from '../../../lib/api/client/types';
import type { ContactRecord } from '../../../lib/store/types';
import {
  normalizeProjectContactLinks,
  normalizeProjectMemberContacts,
} from '../../../lib/domain/projectMembers';
import {
  getProjectRunnerContactRowsSnapshot,
  loadProjectRunnerContactRows,
  markProjectRunnerContactRowsStale,
  removeProjectRunnerContactRow,
  upsertProjectRunnerContactRow,
} from '../../../lib/domain/projectRunner';
import { useProjectRunRealtime } from '../../../lib/realtime/useProjectRunRealtime';
import { useDialogService } from '../../ui/DialogProvider';
import type { ContactItem, ProjectContactLink } from './types';

interface ProjectMembersApiClient {
  listProjectContacts: (
    projectId: string,
    paging?: { limit?: number; offset?: number },
  ) => Promise<ProjectContactLinkResponse[]>;
  addProjectContact: (
    projectId: string,
    payload: { contact_id: string },
  ) => Promise<ProjectContactLinkResponse>;
  removeProjectContact: (projectId: string, contactId: string) => Promise<{ success?: boolean }>;
}

interface UseProjectMembersManagerOptions {
  apiClient: ProjectMembersApiClient;
  projectId?: string | null;
  contacts: ContactItem[];
  loadContacts: () => Promise<ContactRecord[] | void>;
  onMemberRemoved?: (contact: ContactItem) => Promise<void> | void;
}

interface UseProjectMembersManagerResult {
  projectMembers: ProjectContactLink[];
  projectMembersLoading: boolean;
  projectMembersError: string | null;
  projectContactIdSet: Set<string>;
  memberPickerOpen: boolean;
  memberPickerSelectedId: string | null;
  memberPickerError: string | null;
  removingContactId: string | null;
  openAddMember: () => Promise<void>;
  confirmAddMember: () => Promise<string | null>;
  removeMember: (contact: ContactItem) => Promise<boolean>;
  closeMemberPicker: () => void;
  selectMemberPickerContact: (contactId: string | null) => void;
}

export const useProjectMembersManager = ({
  apiClient,
  projectId,
  contacts,
  loadContacts,
  onMemberRemoved,
}: UseProjectMembersManagerOptions): UseProjectMembersManagerResult => {
  const { confirm } = useDialogService();
  const [projectMembers, setProjectMembers] = useState<ProjectContactLink[]>([]);
  const [projectMembersLoading, setProjectMembersLoading] = useState(false);
  const [projectMembersError, setProjectMembersError] = useState<string | null>(null);
  const [memberPickerOpen, setMemberPickerOpen] = useState(false);
  const [memberPickerSelectedId, setMemberPickerSelectedId] = useState<string | null>(null);
  const [memberPickerError, setMemberPickerError] = useState<string | null>(null);
  const [removingContactId, setRemovingContactId] = useState<string | null>(null);
  const realtimeMutationGuardRef = useRef<Map<string, number>>(new Map());

  const normalizedProjectId = normalizeProjectScopeId(projectId);
  const projectContactIdSet = useMemo(
    () => new Set(projectMembers.map((item) => item.contactId)),
    [projectMembers],
  );

  const syncProjectMembersFromRows = useCallback((rows: ProjectContactLinkResponse[] | null | undefined) => {
    if (!rows) {
      return;
    }
    setProjectMembers(normalizeProjectContactLinks(rows));
    setProjectMembersError(null);
  }, []);

  const markRealtimeMutationHandled = useCallback((reason: string, contactId?: string | null) => {
    const normalizedReason = String(reason || '').trim();
    const normalizedContactId = String(contactId || '').trim();
    if (!normalizedReason || !normalizedContactId) {
      return;
    }
    realtimeMutationGuardRef.current.set(`${normalizedReason}:${normalizedContactId}`, Date.now());
  }, []);

  const consumeRecentRealtimeMutation = useCallback((reason: string, contactId?: string | null): boolean => {
    const normalizedReason = String(reason || '').trim();
    const normalizedContactId = String(contactId || '').trim();
    if (!normalizedReason || !normalizedContactId) {
      return false;
    }
    const key = `${normalizedReason}:${normalizedContactId}`;
    const seenAt = realtimeMutationGuardRef.current.get(key);
    if (!seenAt) {
      return false;
    }
    if (Date.now() - seenAt > 4000) {
      realtimeMutationGuardRef.current.delete(key);
      return false;
    }
    realtimeMutationGuardRef.current.delete(key);
    return true;
  }, []);

  const reloadProjectMembers = useCallback(async () => {
    if (!projectId) {
      setProjectMembers([]);
      setProjectMembersLoading(false);
      return;
    }
    setProjectMembersLoading(true);
    setProjectMembersError(null);
    try {
      const rows = await loadProjectRunnerContactRows(apiClient, projectId);
      syncProjectMembersFromRows(rows);
    } catch (error) {
      setProjectMembersError(error instanceof Error ? error.message : '加载项目成员失败');
      setProjectMembers([]);
    } finally {
      setProjectMembersLoading(false);
    }
  }, [apiClient, projectId, syncProjectMembersFromRows]);

  useProjectRunRealtime({
    projectId: normalizedProjectId || null,
    enabled: Boolean(normalizedProjectId),
    onMembersUpdated: async (payload) => {
      const reason = String(payload.reason || '').trim();
      const contactId = String(payload.contact_id || '').trim();
      if (reason !== 'project_contact_added' && reason !== 'project_contact_removed') {
        return;
      }
      if (consumeRecentRealtimeMutation(reason, contactId)) {
        return;
      }
      if (normalizedProjectId) {
        markProjectRunnerContactRowsStale(apiClient, normalizedProjectId);
      }
      await reloadProjectMembers();
    },
  });

  useEffect(() => {
    let cancelled = false;
    const loadProjectMembersOnMount = async () => {
      if (!projectId) {
        setProjectMembers([]);
        setProjectMembersLoading(false);
        return;
      }
      setProjectMembersLoading(true);
      setProjectMembersError(null);
      try {
        const rows = await loadProjectRunnerContactRows(apiClient, projectId);
        if (cancelled) {
          return;
        }
        setProjectMembers(normalizeProjectContactLinks(rows));
      } catch (error) {
        if (!cancelled) {
          setProjectMembersError(error instanceof Error ? error.message : '加载项目成员失败');
          setProjectMembers([]);
        }
      } finally {
        if (!cancelled) {
          setProjectMembersLoading(false);
        }
      }
    };
    void loadProjectMembersOnMount();
    return () => {
      cancelled = true;
    };
  }, [apiClient, projectId]);

  const openAddMember = useCallback(async () => {
    setMemberPickerError(null);
    let latestContacts = contacts || [];
    try {
      const loaded = await loadContacts();
      const normalizedLoaded = normalizeProjectMemberContacts(loaded);
      if (normalizedLoaded.length > 0) {
        latestContacts = normalizedLoaded;
      }
    } catch (error) {
      setMemberPickerError(error instanceof Error ? error.message : '加载联系人失败');
    }
    const firstAvailable = latestContacts.find((item) => !projectContactIdSet.has(item.id));
    setMemberPickerSelectedId(firstAvailable?.id || null);
    setMemberPickerOpen(true);
  }, [contacts, loadContacts, projectContactIdSet]);

  const confirmAddMember = useCallback(async (): Promise<string | null> => {
    const contactId = memberPickerSelectedId?.trim() || '';
    if (!contactId) {
      setMemberPickerError('请先选择联系人');
      return null;
    }
    if (!projectId) {
      setMemberPickerError('当前项目不存在');
      return null;
    }
    try {
      const nextRow = await apiClient.addProjectContact(projectId, { contact_id: contactId });
      markRealtimeMutationHandled('project_contact_added', contactId);
      syncProjectMembersFromRows(
        upsertProjectRunnerContactRow(apiClient, projectId, nextRow)
        || getProjectRunnerContactRowsSnapshot(apiClient, projectId)
        || [nextRow],
      );
      setMemberPickerOpen(false);
      setMemberPickerSelectedId(null);
      setMemberPickerError(null);
      return contactId;
    } catch (error) {
      setMemberPickerError(error instanceof Error ? error.message : '添加项目成员失败');
      return null;
    }
  }, [
    apiClient,
    markRealtimeMutationHandled,
    memberPickerSelectedId,
    projectId,
    syncProjectMembersFromRows,
  ]);

  const removeMember = useCallback(async (contact: ContactItem): Promise<boolean> => {
    if (!projectId) {
      return false;
    }
    const confirmed = await confirm({
      title: '移除项目成员',
      message: `确定将 ${contact.name} 从当前项目团队中移除吗？`,
      confirmText: '移除',
      cancelText: '取消',
      type: 'danger',
    });
    if (!confirmed) {
      return false;
    }
    setMemberPickerError(null);
    setRemovingContactId(contact.id);
    try {
      await apiClient.removeProjectContact(projectId, contact.id);
      markRealtimeMutationHandled('project_contact_removed', contact.id);
      syncProjectMembersFromRows(
        removeProjectRunnerContactRow(apiClient, projectId, contact.id)
        || getProjectRunnerContactRowsSnapshot(apiClient, projectId)
        || [],
      );
      if (onMemberRemoved) {
        await onMemberRemoved(contact);
      }
      return true;
    } catch (error) {
      setMemberPickerError(error instanceof Error ? error.message : '移除项目成员失败');
      return false;
    } finally {
      setRemovingContactId((prev) => (prev === contact.id ? null : prev));
    }
  }, [
    apiClient,
    confirm,
    markRealtimeMutationHandled,
    onMemberRemoved,
    projectId,
    syncProjectMembersFromRows,
  ]);

  const closeMemberPicker = useCallback(() => {
    setMemberPickerOpen(false);
    setMemberPickerSelectedId(null);
    setMemberPickerError(null);
  }, []);

  const selectMemberPickerContact = useCallback((contactId: string | null) => {
    setMemberPickerSelectedId(contactId);
    setMemberPickerError(null);
  }, []);

  return {
    projectMembers,
    projectMembersLoading,
    projectMembersError,
    projectContactIdSet,
    memberPickerOpen,
    memberPickerSelectedId,
    memberPickerError,
    removingContactId,
    openAddMember,
    confirmAddMember,
    removeMember,
    closeMemberPicker,
    selectMemberPickerContact,
  };
};
