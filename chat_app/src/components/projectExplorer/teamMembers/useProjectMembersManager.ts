// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useCallback, useEffect, useMemo, useState } from 'react';

import { useI18n } from '../../../i18n/I18nProvider';
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
import { useRecentMutationGuard } from '../../../hooks/useRecentMutationGuard';
import { useDialogService } from '../../ui/DialogProvider';
import type { ContactItem, ProjectContactLink } from './types';

interface ProjectMemberMutationPayload {
  reason?: string | null;
  contactId?: string | null;
}

const PROJECT_CONTACT_ADDED_REASON = 'project_contact_added';
const PROJECT_CONTACT_REMOVED_REASON = 'project_contact_removed';

const buildProjectMemberMutationKey = ({
  reason,
  contactId,
}: ProjectMemberMutationPayload): string => {
  const normalizedReason = String(reason || '').trim();
  const normalizedContactId = String(contactId || '').trim();
  if (!normalizedReason || !normalizedContactId) {
    return '';
  }
  return `${normalizedReason}:${normalizedContactId}`;
};

const isProjectMemberMutationReason = (reason: string): boolean => (
  reason === PROJECT_CONTACT_ADDED_REASON || reason === PROJECT_CONTACT_REMOVED_REASON
);

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
  const { t } = useI18n();
  const [projectMembers, setProjectMembers] = useState<ProjectContactLink[]>([]);
  const [projectMembersLoading, setProjectMembersLoading] = useState(false);
  const [projectMembersError, setProjectMembersError] = useState<string | null>(null);
  const [memberPickerOpen, setMemberPickerOpen] = useState(false);
  const [memberPickerSelectedId, setMemberPickerSelectedId] = useState<string | null>(null);
  const [memberPickerError, setMemberPickerError] = useState<string | null>(null);
  const [removingContactId, setRemovingContactId] = useState<string | null>(null);

  const normalizedProjectId = normalizeProjectScopeId(projectId);
  const projectContactIdSet = useMemo(
    () => new Set(projectMembers.map((item) => item.contactId)),
    [projectMembers],
  );
  const {
    markRecentMutation: markRealtimeMutationHandled,
    consumeRecentMutation: consumeRecentRealtimeMutation,
  } = useRecentMutationGuard<ProjectMemberMutationPayload>({
    buildKey: buildProjectMemberMutationKey,
  });

  const syncProjectMembersFromRows = useCallback((rows: ProjectContactLinkResponse[] | null | undefined) => {
    if (!rows) {
      return;
    }
    setProjectMembers(normalizeProjectContactLinks(rows));
    setProjectMembersError(null);
  }, []);

  const loadProjectMembers = useCallback(async (
    shouldApply: () => boolean = () => true,
  ) => {
    if (!projectId) {
      if (shouldApply()) {
        setProjectMembers([]);
        setProjectMembersLoading(false);
      }
      return;
    }
    if (shouldApply()) {
      setProjectMembersLoading(true);
      setProjectMembersError(null);
    }
    try {
      const rows = await loadProjectRunnerContactRows(apiClient, projectId);
      if (!shouldApply()) {
        return;
      }
      syncProjectMembersFromRows(rows);
    } catch (error) {
      if (!shouldApply()) {
        return;
      }
      setProjectMembersError(error instanceof Error ? error.message : t('teamMembers.error.loadMembersFailed'));
      setProjectMembers([]);
    } finally {
      if (shouldApply()) {
        setProjectMembersLoading(false);
      }
    }
  }, [apiClient, projectId, syncProjectMembersFromRows, t]);

  useProjectRunRealtime({
    projectId: normalizedProjectId || null,
    enabled: Boolean(normalizedProjectId),
    onMembersUpdated: async (payload) => {
      const reason = String(payload.reason || '').trim();
      const contactId = String(payload.contact_id || '').trim();
      if (!isProjectMemberMutationReason(reason)) {
        return;
      }
      if (consumeRecentRealtimeMutation({ reason, contactId })) {
        return;
      }
      if (normalizedProjectId) {
        markProjectRunnerContactRowsStale(apiClient, normalizedProjectId);
      }
      await loadProjectMembers();
    },
  });

  useEffect(() => {
    let active = true;
    void loadProjectMembers(() => active);
    return () => {
      active = false;
    };
  }, [loadProjectMembers]);

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
      setMemberPickerError(error instanceof Error ? error.message : t('teamMembers.error.loadContactsFailed'));
    }
    const firstAvailable = latestContacts.find((item) => !projectContactIdSet.has(item.id));
    setMemberPickerSelectedId(firstAvailable?.id || null);
    setMemberPickerOpen(true);
  }, [contacts, loadContacts, projectContactIdSet, t]);

  const confirmAddMember = useCallback(async (): Promise<string | null> => {
    const contactId = memberPickerSelectedId?.trim() || '';
    if (!contactId) {
      setMemberPickerError(t('teamMembers.error.selectContactFirst'));
      return null;
    }
    if (!projectId) {
      setMemberPickerError(t('teamMembers.error.projectMissing'));
      return null;
    }
    try {
      const nextRow = await apiClient.addProjectContact(projectId, { contact_id: contactId });
      markRealtimeMutationHandled({
        reason: PROJECT_CONTACT_ADDED_REASON,
        contactId,
      });
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
      setMemberPickerError(error instanceof Error ? error.message : t('teamMembers.error.addFailed'));
      return null;
    }
  }, [
    apiClient,
    markRealtimeMutationHandled,
    memberPickerSelectedId,
    projectId,
    syncProjectMembersFromRows,
    t,
  ]);

  const removeMember = useCallback(async (contact: ContactItem): Promise<boolean> => {
    if (!projectId) {
      return false;
    }
    const confirmed = await confirm({
      title: t('teamMembers.removeTitle'),
      message: t('teamMembers.removeMessage', { name: contact.name }),
      confirmText: t('teamMembers.remove'),
      cancelText: t('common.cancel'),
      type: 'danger',
    });
    if (!confirmed) {
      return false;
    }
    setMemberPickerError(null);
    setRemovingContactId(contact.id);
    try {
      await apiClient.removeProjectContact(projectId, contact.id);
      markRealtimeMutationHandled({
        reason: PROJECT_CONTACT_REMOVED_REASON,
        contactId: contact.id,
      });
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
      setMemberPickerError(error instanceof Error ? error.message : t('teamMembers.error.removeFailed'));
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
    t,
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
