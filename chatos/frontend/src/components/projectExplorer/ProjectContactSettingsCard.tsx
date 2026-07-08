// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React, { useCallback, useEffect, useMemo, useState } from 'react';
import { RefreshCw } from 'lucide-react';

import { useI18n } from '../../i18n/I18nProvider';
import { useApiClient } from '../../lib/api/ApiClientContext';
import type { ContactResponse, ProjectContactLinkResponse } from '../../lib/api/client/types';
import { normalizeContact } from '../../lib/domain/contacts';
import {
  removeProjectRunnerContactRow,
  syncProjectRunnerContactRows,
  upsertProjectRunnerContactRow,
} from '../../lib/domain/projectRunner';
import { cn } from '../../lib/utils';
import type { Project } from '../../types';
import { ProjectContactPickerModal } from '../sessionList/ProjectContactPickerModal';

interface ProjectContactSettingsCardProps {
  project: Project;
}

interface ContactOption {
  id: string;
  name: string;
  agentId: string;
}

const readString = (value: unknown): string => (
  typeof value === 'string' ? value.trim() : ''
);

const normalizeProjectContact = (value: ProjectContactLinkResponse | null | undefined) => {
  const contactId = readString(value?.contact_id ?? value?.contactId);
  const agentId = readString(value?.agent_id ?? value?.agentId);
  const name = readString(value?.agent_name_snapshot ?? value?.agentNameSnapshot) || contactId;
  if (!contactId || !agentId) {
    return null;
  }
  return {
    contactId,
    agentId,
    name,
    updatedAt: readString(value?.updated_at ?? value?.updatedAt),
  };
};

const chooseLatestProjectContact = (
  rows: ProjectContactLinkResponse[],
): ReturnType<typeof normalizeProjectContact> => {
  const normalized = rows
    .map(normalizeProjectContact)
    .filter((item): item is NonNullable<typeof item> => Boolean(item));
  normalized.sort((left, right) => right.updatedAt.localeCompare(left.updatedAt));
  return normalized[0] || null;
};

const normalizeContactOptions = (rows: ContactResponse[] | unknown): ContactOption[] => (
  (Array.isArray(rows) ? rows : [])
    .map(normalizeContact)
    .filter((item): item is NonNullable<typeof item> => Boolean(item))
    .filter((item) => item.status !== 'archived')
    .map((item) => ({
      id: item.id,
      name: item.name || item.agentId,
      agentId: item.agentId,
    }))
);

const ProjectContactSettingsCard: React.FC<ProjectContactSettingsCardProps> = ({ project }) => {
  const { t } = useI18n();
  const apiClient = useApiClient();
  const [projectContactRows, setProjectContactRows] = useState<ProjectContactLinkResponse[]>([]);
  const [contacts, setContacts] = useState<ContactOption[]>([]);
  const [locked, setLocked] = useState(false);
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [pickerOpen, setPickerOpen] = useState(false);
  const [selectedContactId, setSelectedContactId] = useState<string | null>(null);

  const currentContact = useMemo(
    () => chooseLatestProjectContact(projectContactRows),
    [projectContactRows],
  );

  const loadProjectContact = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const [projectContacts, allContacts, lockState] = await Promise.all([
        apiClient.listProjectContacts(project.id, { limit: 500, offset: 0 }),
        apiClient.getContacts(undefined, { limit: 500, offset: 0 }),
        apiClient.getProjectContactLock(project.id),
      ]);
      setProjectContactRows(
        syncProjectRunnerContactRows(apiClient, project.id, projectContacts) || projectContacts,
      );
      setContacts(normalizeContactOptions(allContacts));
      setLocked(lockState.locked === true);
    } catch (err) {
      setError(err instanceof Error ? err.message : t('projectContact.error.loadFailed'));
    } finally {
      setLoading(false);
    }
  }, [apiClient, project.id, t]);

  useEffect(() => {
    void loadProjectContact();
  }, [loadProjectContact]);

  const openPicker = useCallback(() => {
    if (locked) {
      return;
    }
    setSelectedContactId(null);
    setError(null);
    setPickerOpen(true);
  }, [locked]);

  const handleConfirmPicker = useCallback(async () => {
    if (!selectedContactId) {
      setError(t('projectContact.error.selectRequired'));
      return;
    }
    setSaving(true);
    setError(null);
    try {
      const nextRow = await apiClient.addProjectContact(project.id, { contact_id: selectedContactId });
      const nextContactId = normalizeProjectContact(nextRow)?.contactId || selectedContactId;
      const optimisticRows = [
        nextRow,
        ...projectContactRows.filter((row) => normalizeProjectContact(row)?.contactId !== nextContactId),
      ];
      setProjectContactRows(
        upsertProjectRunnerContactRow(apiClient, project.id, nextRow)
        || syncProjectRunnerContactRows(apiClient, project.id, optimisticRows)
        || optimisticRows,
      );
      setPickerOpen(false);
      setSelectedContactId(null);
      await loadProjectContact();
    } catch (err) {
      setError(err instanceof Error ? err.message : t('projectContact.error.saveFailed'));
    } finally {
      setSaving(false);
    }
  }, [apiClient, loadProjectContact, project.id, projectContactRows, selectedContactId, t]);

  const handleUnbind = useCallback(async () => {
    if (!currentContact || locked) {
      return;
    }
    setSaving(true);
    setError(null);
    try {
      await apiClient.removeProjectContact(project.id, currentContact.contactId);
      const optimisticRows = projectContactRows.filter(
        (row) => normalizeProjectContact(row)?.contactId !== currentContact.contactId,
      );
      setProjectContactRows(
        removeProjectRunnerContactRow(apiClient, project.id, currentContact.contactId)
        || syncProjectRunnerContactRows(apiClient, project.id, optimisticRows)
        || optimisticRows,
      );
      await loadProjectContact();
    } catch (err) {
      setError(err instanceof Error ? err.message : t('projectContact.error.unbindFailed'));
    } finally {
      setSaving(false);
    }
  }, [apiClient, currentContact, loadProjectContact, locked, project.id, projectContactRows, t]);

  const actionsDisabled = loading || saving || locked;

  return (
    <>
      <section className="mb-4 rounded-xl border border-border bg-card p-4 shadow-sm">
        <div className="flex items-start justify-between gap-3">
          <div className="min-w-0">
            <h2 className="text-sm font-semibold text-foreground">{t('projectContact.title')}</h2>
            <p className="mt-1 text-xs leading-5 text-muted-foreground">
              {t('projectContact.description')}
            </p>
          </div>
          <button
            type="button"
            className="rounded-md border border-border bg-background p-1.5 text-muted-foreground hover:bg-accent hover:text-foreground disabled:opacity-60"
            onClick={() => void loadProjectContact()}
            disabled={loading || saving}
            aria-label={t('projectContact.refresh')}
            title={t('projectContact.refresh')}
          >
            <RefreshCw className={cn('h-4 w-4', loading && 'animate-spin')} />
          </button>
        </div>

        <div className="mt-4 rounded-lg border border-border bg-background px-3 py-3">
          {currentContact ? (
            <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
              <div className="min-w-0">
                <div className="truncate text-sm font-medium text-foreground">
                  {currentContact.name}
                </div>
                <div className="mt-1 truncate text-xs text-muted-foreground">
                  {t('projectContact.agentId', { id: currentContact.agentId })}
                </div>
              </div>
              <div className="flex shrink-0 items-center gap-2">
                <button
                  type="button"
                  className="rounded-md border border-border px-3 py-1.5 text-xs text-muted-foreground hover:bg-accent hover:text-foreground disabled:opacity-60"
                  disabled={actionsDisabled}
                  onClick={openPicker}
                >
                  {t('projectContact.change')}
                </button>
                <button
                  type="button"
                  className="rounded-md border border-border px-3 py-1.5 text-xs text-muted-foreground hover:border-destructive hover:text-destructive disabled:opacity-60"
                  disabled={actionsDisabled}
                  onClick={() => void handleUnbind()}
                >
                  {t('projectContact.unbind')}
                </button>
              </div>
            </div>
          ) : (
            <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
              <div>
                <div className="text-sm font-medium text-foreground">{t('projectContact.emptyTitle')}</div>
                <div className="mt-1 text-xs text-muted-foreground">{t('projectContact.emptyDescription')}</div>
              </div>
              <button
                type="button"
                className="rounded-md bg-primary px-3 py-1.5 text-xs text-primary-foreground hover:bg-primary/90 disabled:opacity-60"
                disabled={actionsDisabled}
                onClick={openPicker}
              >
                {t('projectContact.bind')}
              </button>
            </div>
          )}
        </div>

        {locked ? (
          <div className="mt-3 rounded-md border border-amber-200 bg-amber-50 px-3 py-2 text-xs text-amber-800">
            {t('projectContact.locked')}
          </div>
        ) : null}

        {error ? (
          <div className="mt-3 rounded-md border border-destructive/30 bg-destructive/10 px-3 py-2 text-xs text-destructive">
            {error}
          </div>
        ) : null}
      </section>

      <ProjectContactPickerModal
        isOpen={pickerOpen}
        projectName={project.name}
        contacts={contacts}
        disabledContactIds={currentContact ? [currentContact.contactId] : []}
        selectedContactId={selectedContactId}
        error={error}
        onClose={() => {
          setPickerOpen(false);
          setSelectedContactId(null);
        }}
        onSelectedContactChange={setSelectedContactId}
        onConfirm={() => void handleConfirmPicker()}
      />
    </>
  );
};

export default ProjectContactSettingsCard;
