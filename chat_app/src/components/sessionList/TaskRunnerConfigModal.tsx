import React, { useEffect, useState } from 'react';

import { useI18n } from '../../i18n/I18nProvider';
import ManagerFormDialog from '../ui/ManagerFormDialog';
import type { ContactItem } from './types';

const DEFAULT_TASK_RUNNER_BASE_URL = 'http://127.0.0.1:39090';

interface TaskRunnerConfigModalProps {
  isOpen: boolean;
  contact: ContactItem | null;
  saving: boolean;
  error: string | null;
  onClose: () => void;
  onSave: (values: {
    enabled: boolean;
    baseUrl: string;
    username: string;
    password?: string;
    clearPassword?: boolean;
  }) => Promise<void> | void;
}

export const TaskRunnerConfigModal: React.FC<TaskRunnerConfigModalProps> = ({
  isOpen,
  contact,
  saving,
  error,
  onClose,
  onSave,
}) => {
  const { t } = useI18n();
  const [enabled, setEnabled] = useState(false);
  const [baseUrl, setBaseUrl] = useState(DEFAULT_TASK_RUNNER_BASE_URL);
  const [username, setUsername] = useState('');
  const [password, setPassword] = useState('');
  const [clearPassword, setClearPassword] = useState(false);

  useEffect(() => {
    if (!isOpen || !contact) {
      return;
    }
    setEnabled(Boolean(contact.taskRunner?.enabled));
    setBaseUrl(contact.taskRunner?.baseUrl || DEFAULT_TASK_RUNNER_BASE_URL);
    setUsername(contact.taskRunner?.username || '');
    setPassword('');
    setClearPassword(false);
  }, [contact, isOpen]);

  if (!contact) {
    return null;
  }
  const taskRunner = contact.taskRunner || {
    enabled: false,
    baseUrl: '',
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
            baseUrl,
            username,
            password: password.trim() || undefined,
            clearPassword,
          });
        }}
      >
        <div className="space-y-4 rounded-xl border border-border bg-muted/40 p-4">
          <div className="flex items-center justify-between gap-3">
            <div>
              <div className="text-sm font-medium text-foreground">{contact.name}</div>
              <div className="text-xs text-muted-foreground">
                {taskRunner.hasPassword
                  ? t('taskRunnerConfig.passwordSaved')
                  : t('taskRunnerConfig.passwordMissing')}
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

          <div>
            <label className="text-sm text-muted-foreground">{t('taskRunnerConfig.baseUrl')}</label>
            <input
              value={baseUrl}
              onChange={(event) => setBaseUrl(event.target.value)}
              className="mt-1 w-full rounded border border-border bg-background px-3 py-2 text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
              placeholder={DEFAULT_TASK_RUNNER_BASE_URL}
            />
          </div>

          <div>
            <label className="text-sm text-muted-foreground">{t('taskRunnerConfig.username')}</label>
            <input
              value={username}
              onChange={(event) => setUsername(event.target.value)}
              className="mt-1 w-full rounded border border-border bg-background px-3 py-2 text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
              autoComplete="off"
            />
          </div>

          <div>
            <label className="text-sm text-muted-foreground">{t('taskRunnerConfig.password')}</label>
            <input
              value={password}
              onChange={(event) => {
                setPassword(event.target.value);
                if (event.target.value.trim()) {
                  setClearPassword(false);
                }
              }}
              type="password"
              className="mt-1 w-full rounded border border-border bg-background px-3 py-2 text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
              placeholder={taskRunner.hasPassword ? t('taskRunnerConfig.passwordKeepPlaceholder') : ''}
              autoComplete="new-password"
            />
          </div>

          {taskRunner.hasPassword ? (
            <label className="inline-flex items-center gap-2 text-sm text-muted-foreground">
              <input
                type="checkbox"
                checked={clearPassword}
                disabled={Boolean(password.trim())}
                onChange={(event) => setClearPassword(event.target.checked)}
              />
              {t('taskRunnerConfig.clearPassword')}
            </label>
          ) : null}

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
