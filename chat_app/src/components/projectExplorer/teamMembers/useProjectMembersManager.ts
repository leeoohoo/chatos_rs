import { useCallback, useEffect, useMemo, useState } from 'react';

import { normalizeProjectScopeId } from '../../../features/contactSession/sessionResolver';
import type { ProjectContactLinkResponse } from '../../../lib/api/client/types';
import type { ContactRecord } from '../../../lib/store/types';
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

const normalizeContactList = (value: unknown): ContactItem[] => {
  if (!Array.isArray(value)) {
    return [];
  }
  const out: ContactItem[] = [];
  for (const item of value) {
    const id = typeof item?.id === 'string' ? item.id.trim() : '';
    const agentId = typeof item?.agentId === 'string' ? item.agentId.trim() : '';
    if (!id || !agentId) {
      continue;
    }
    const name = typeof item?.name === 'string' && item.name.trim()
      ? item.name.trim()
      : id;
    out.push({ id, agentId, name });
  }
  return out;
};

const readStringField = (value: unknown, key: string): string => {
  if (!value || typeof value !== 'object') {
    return '';
  }
  const raw = (value as Record<string, unknown>)[key];
  return typeof raw === 'string' ? raw.trim() : '';
};

const normalizeProjectContactRows = (value: unknown): ProjectContactLink[] => {
  const deduped = new Map<string, ProjectContactLink>();
  for (const item of Array.isArray(value) ? value : []) {
    const contactId = readStringField(item, 'contact_id');
    const agentId = readStringField(item, 'agent_id');
    if (!contactId || !agentId) {
      continue;
    }
    const name = readStringField(item, 'agent_name_snapshot') || contactId;
    const ts = new Date(
      readStringField(item, 'updated_at')
      || readStringField(item, 'last_bound_at')
      || Date.now(),
    ).getTime();
    const updatedAt = Number.isFinite(ts) ? ts : 0;
    const current = deduped.get(contactId);
    if (!current || updatedAt >= current.updatedAt) {
      deduped.set(contactId, { contactId, agentId, name, updatedAt });
    }
  }
  return Array.from(deduped.values()).sort((left, right) => right.updatedAt - left.updatedAt);
};

export const useProjectMembersManager = ({
  apiClient,
  projectId,
  contacts,
  loadContacts,
  onMemberRemoved,
}: UseProjectMembersManagerOptions): UseProjectMembersManagerResult => {
  const [projectMembers, setProjectMembers] = useState<ProjectContactLink[]>([]);
  const [projectMembersLoading, setProjectMembersLoading] = useState(false);
  const [projectMembersError, setProjectMembersError] = useState<string | null>(null);
  const [projectMembersReloadSeed, setProjectMembersReloadSeed] = useState(0);
  const [memberPickerOpen, setMemberPickerOpen] = useState(false);
  const [memberPickerSelectedId, setMemberPickerSelectedId] = useState<string | null>(null);
  const [memberPickerError, setMemberPickerError] = useState<string | null>(null);
  const [removingContactId, setRemovingContactId] = useState<string | null>(null);

  const normalizedProjectId = normalizeProjectScopeId(projectId);
  const projectContactIdSet = useMemo(
    () => new Set(projectMembers.map((item) => item.contactId)),
    [projectMembers],
  );

  const emitProjectContactChanged = useCallback((projectIdValue: string) => {
    if (typeof window === 'undefined') {
      return;
    }
    window.dispatchEvent(new CustomEvent('project-contact-changed', {
      detail: { projectId: projectIdValue },
    }));
  }, []);

  useEffect(() => {
    if (typeof window === 'undefined') {
      return;
    }
    const handler = (event: Event) => {
      const customEvent = event as CustomEvent<{ projectId?: string }>;
      const changedProjectId = normalizeProjectScopeId(customEvent?.detail?.projectId ?? null);
      if (changedProjectId !== normalizedProjectId) {
        return;
      }
      setProjectMembersReloadSeed((prev) => prev + 1);
    };
    window.addEventListener('project-contact-changed', handler as EventListener);
    return () => {
      window.removeEventListener('project-contact-changed', handler as EventListener);
    };
  }, [normalizedProjectId]);

  useEffect(() => {
    let cancelled = false;
    const loadProjectMembers = async () => {
      if (!projectId) {
        setProjectMembers([]);
        setProjectMembersLoading(false);
        return;
      }
      setProjectMembersLoading(true);
      setProjectMembersError(null);
      try {
        const rows = await apiClient.listProjectContacts(projectId, { limit: 500, offset: 0 });
        if (cancelled) {
          return;
        }
        setProjectMembers(normalizeProjectContactRows(rows));
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
    void loadProjectMembers();
    return () => {
      cancelled = true;
    };
  }, [apiClient, projectId, projectMembersReloadSeed]);

  const openAddMember = useCallback(async () => {
    setMemberPickerError(null);
    let latestContacts = contacts || [];
    try {
      const loaded = await loadContacts();
      const normalizedLoaded = normalizeContactList(loaded);
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
      await apiClient.addProjectContact(projectId, { contact_id: contactId });
      emitProjectContactChanged(projectId);
      setMemberPickerOpen(false);
      setMemberPickerSelectedId(null);
      setMemberPickerError(null);
      return contactId;
    } catch (error) {
      setMemberPickerError(error instanceof Error ? error.message : '添加项目成员失败');
      return null;
    }
  }, [apiClient, emitProjectContactChanged, memberPickerSelectedId, projectId]);

  const removeMember = useCallback(async (contact: ContactItem): Promise<boolean> => {
    if (!projectId) {
      return false;
    }
    const confirmed = typeof window === 'undefined'
      ? true
      : window.confirm(`确定将 ${contact.name} 从当前项目团队中移除吗？`);
    if (!confirmed) {
      return false;
    }
    setMemberPickerError(null);
    setRemovingContactId(contact.id);
    try {
      await apiClient.removeProjectContact(projectId, contact.id);
      emitProjectContactChanged(projectId);
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
  }, [apiClient, emitProjectContactChanged, onMemberRemoved, projectId]);

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
