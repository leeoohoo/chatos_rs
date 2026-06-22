import React, { useEffect, useState } from 'react';

import { useI18n } from '../../i18n/I18nProvider';
import type { TaskRunnerAgentAccountResponse } from '../../lib/api/client/types';
import ManagerFormDialog from '../ui/ManagerFormDialog';
import type { ContactItem } from './types';

interface TaskRunnerConfigModalProps {
  isOpen: boolean;
  contact: ContactItem | null;
  agentAccounts: TaskRunnerAgentAccountResponse[];
  loadingAgentAccounts: boolean;
  saving: boolean;
  error: string | null;
  onClose: () => void;
  onSave: (values: {
    enabled: boolean;
    agentAccountId: string;
  }) => Promise<void> | void;
}

export const TaskRunnerConfigModal: React.FC<TaskRunnerConfigModalProps> = ({
  isOpen,
  contact,
  agentAccounts,
  loadingAgentAccounts,
  saving,
  error,
  onClose,
  onSave,
}) => {
  const { t } = useI18n();
  const [enabled, setEnabled] = useState(false);
  const [agentAccountId, setAgentAccountId] = useState('');

  useEffect(() => {
    if (!isOpen || !contact) {
      return;
    }
    setEnabled(Boolean(contact.taskRunner?.enabled));
    setAgentAccountId(contact.taskRunner?.agentAccountId || '');
  }, [contact, isOpen]);

  if (!contact) {
    return null;
  }
  const taskRunner = contact.taskRunner || {
    enabled: false,
    agentAccountId: '',
    username: '',
    hasPassword: false,
  };

  return (
    <ManagerFormDialog
      open={isOpen}
      title={t('taskRunnerConfig.title')}
      widthClassName="max-w-xl"
      onClose={onClose}
    >
      <form
        className="space-y-4"
        onSubmit={(event) => {
          event.preventDefault();
          void onSave({
            enabled,
            agentAccountId,
          });
        }}
      >
        <div className="space-y-4 rounded-xl border border-border bg-muted/40 p-4">
          <div className="flex items-center justify-between gap-3">
            <div>
              <div className="text-sm font-medium text-foreground">{contact.name}</div>
              <div className="text-xs text-muted-foreground">
                {taskRunner.agentAccountId
                  ? t('taskRunnerConfig.agentAccountSelected')
                  : t('taskRunnerConfig.noAgentAccountSelected')}
              </div>
            </div>
            <label className="inline-flex items-center gap-2 text-sm text-muted-foreground">
              <input
                type="checkbox"
                checked={enabled}
                onChange={(event) => setEnabled(event.target.checked)}
              />
              {t('taskRunnerConfig.enabled')}
            </label>
          </div>

          <div className="text-xs text-muted-foreground">
            {t('taskRunnerConfig.endpointManaged')}
          </div>

          <div>
            <label className="text-sm text-muted-foreground">{t('taskRunnerConfig.agentAccount')}</label>
            <select
              value={agentAccountId}
              onChange={(event) => setAgentAccountId(event.target.value)}
              className="mt-1 w-full rounded border border-border bg-background px-3 py-2 text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
              disabled={loadingAgentAccounts}
            >
              <option value="">{t('taskRunnerConfig.agentAccountPlaceholder')}</option>
              {agentAccounts
                .filter((item) => item.enabled !== false)
                .map((item) => (
                  <option key={item.id} value={item.id}>
                    {item.display_name?.trim() || item.username} ({item.username})
                  </option>
                ))}
            </select>
            {loadingAgentAccounts ? (
              <div className="mt-1 text-xs text-muted-foreground">{t('common.loading')}</div>
            ) : null}
            {!loadingAgentAccounts && agentAccounts.length === 0 ? (
              <div className="mt-1 text-xs text-muted-foreground">{t('taskRunnerConfig.noAgentAccounts')}</div>
            ) : null}
          </div>

          {error ? <div className="text-xs text-destructive">{error}</div> : null}
        </div>

        <div className="flex justify-end gap-2">
          <button
            type="button"
            onClick={onClose}
            className="rounded-lg bg-muted px-3 py-2 text-sm text-muted-foreground transition-colors hover:bg-accent"
          >
            {t('common.cancel')}
          </button>
          <button
            type="submit"
            disabled={saving}
            className="rounded-lg bg-primary px-4 py-2 text-sm text-primary-foreground transition-opacity hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-50"
          >
            {saving ? t('common.saving') : t('common.save')}
          </button>
        </div>
      </form>
    </ManagerFormDialog>
  );
};
