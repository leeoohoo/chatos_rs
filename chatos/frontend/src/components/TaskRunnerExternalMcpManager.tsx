// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React, { useCallback, useEffect, useMemo, useState } from 'react';
import {
  Loader2,
  Pencil,
  Plus,
  Power,
  PowerOff,
  ServerCog,
  Trash2,
  X,
} from 'lucide-react';

import { useI18n } from '../i18n/I18nProvider';
import { useApiClient } from '../lib/api/ApiClientContext';
import type {
  CreateTaskRunnerExternalMcpConfigPayload,
  TaskRunnerExternalMcpConfig,
  TaskRunnerExternalMcpTransport,
} from '../lib/api/client/types';
import { useDialogService } from './ui/DialogProvider';
import ManagerFormDialog from './ui/ManagerFormDialog';

interface Props {
  onClose: () => void;
}

interface FormState {
  name: string;
  transport: TaskRunnerExternalMcpTransport;
  command: string;
  argsText: string;
  url: string;
  headersText: string;
  envText: string;
  cwd: string;
  enabled: boolean;
}

const defaultFormState = (): FormState => ({
  name: '',
  transport: 'stdio',
  command: '',
  argsText: '',
  url: '',
  headersText: '',
  envText: '',
  cwd: '',
  enabled: true,
});

const TaskRunnerExternalMcpManager: React.FC<Props> = ({ onClose }) => {
  const { t } = useI18n();
  const apiClient = useApiClient();
  const { confirm } = useDialogService();
  const [configs, setConfigs] = useState<TaskRunnerExternalMcpConfig[]>([]);
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [formError, setFormError] = useState<string | null>(null);
  const [formOpen, setFormOpen] = useState(false);
  const [editingConfig, setEditingConfig] = useState<TaskRunnerExternalMcpConfig | null>(null);
  const [formData, setFormData] = useState<FormState>(defaultFormState);

  const loadConfigs = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const items = await apiClient.listTaskRunnerExternalMcpConfigs();
      setConfigs(items);
    } catch (loadError) {
      console.error('Failed to load task runner external MCP configs:', loadError);
      setError(loadError instanceof Error ? loadError.message : t('externalMcpManager.error.load'));
    } finally {
      setLoading(false);
    }
  }, [apiClient, t]);

  useEffect(() => {
    void loadConfigs();
  }, [loadConfigs]);

  const resetForm = useCallback(() => {
    setFormData(defaultFormState());
    setEditingConfig(null);
    setFormError(null);
    setSaving(false);
    setFormOpen(false);
  }, []);

  const openCreateDialog = () => {
    setEditingConfig(null);
    setFormData(defaultFormState());
    setFormError(null);
    setFormOpen(true);
  };

  const openEditDialog = (config: TaskRunnerExternalMcpConfig) => {
    setEditingConfig(config);
    setFormData({
      name: config.name || '',
      transport: config.transport === 'http' ? 'http' : 'stdio',
      command: config.command || '',
      argsText: (config.args || []).join('\n'),
      url: config.url || '',
      headersText: stringifyMap(config.headers),
      envText: stringifyMap(config.env),
      cwd: config.cwd || '',
      enabled: config.enabled,
    });
    setFormError(null);
    setFormOpen(true);
  };

  const updateFormData = (patch: Partial<FormState>) => {
    setFormData((current) => ({
      ...current,
      ...patch,
    }));
  };

  const handleSubmit = async (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setFormError(null);
    let payload: CreateTaskRunnerExternalMcpConfigPayload;
    try {
      payload = buildPayload(formData);
    } catch (payloadError) {
      setFormError(payloadError instanceof Error ? payloadError.message : t('externalMcpManager.error.invalidForm'));
      return;
    }
    if (!payload.name.trim()) {
      setFormError(t('externalMcpManager.error.nameRequired'));
      return;
    }
    if (payload.transport === 'http' && !payload.url?.trim()) {
      setFormError(t('externalMcpManager.error.urlRequired'));
      return;
    }
    if (payload.transport !== 'http' && !payload.command?.trim()) {
      setFormError(t('externalMcpManager.error.commandRequired'));
      return;
    }

    setSaving(true);
    try {
      if (editingConfig) {
        await apiClient.updateTaskRunnerExternalMcpConfig(editingConfig.id, payload);
      } else {
        await apiClient.createTaskRunnerExternalMcpConfig(payload);
      }
      await loadConfigs();
      resetForm();
    } catch (saveError) {
      console.error('Failed to save task runner external MCP config:', saveError);
      setFormError(saveError instanceof Error ? saveError.message : t('externalMcpManager.error.save'));
    } finally {
      setSaving(false);
    }
  };

  const handleDelete = async (config: TaskRunnerExternalMcpConfig) => {
    const confirmed = await confirm({
      title: t('externalMcpManager.confirmDeleteTitle'),
      message: t('externalMcpManager.confirmDeleteMessage', { name: config.name || config.id }),
      confirmText: t('common.delete'),
      cancelText: t('common.cancel'),
      type: 'danger',
    });
    if (!confirmed) {
      return;
    }
    try {
      await apiClient.deleteTaskRunnerExternalMcpConfig(config.id);
      await loadConfigs();
    } catch (deleteError) {
      console.error('Failed to delete task runner external MCP config:', deleteError);
      setError(deleteError instanceof Error ? deleteError.message : t('externalMcpManager.error.delete'));
    }
  };

  const toggleEnabled = async (config: TaskRunnerExternalMcpConfig) => {
    try {
      await apiClient.updateTaskRunnerExternalMcpConfig(config.id, {
        enabled: !config.enabled,
      });
      await loadConfigs();
    } catch (toggleError) {
      console.error('Failed to toggle task runner external MCP config:', toggleError);
      setError(toggleError instanceof Error ? toggleError.message : t('externalMcpManager.error.save'));
    }
  };

  const sortedConfigs = useMemo(() => (
    [...configs].sort((a, b) => String(b.updated_at || '').localeCompare(String(a.updated_at || '')))
  ), [configs]);

  return (
    <>
      <div className="fixed inset-0 z-40 bg-black/50 backdrop-blur-sm" onClick={onClose} />
      <div className="fixed right-0 top-0 z-50 flex h-full w-full max-w-[640px] flex-col border-l border-border bg-card shadow-xl sm:w-[620px]">
        <div className="flex items-center justify-between border-b border-border p-4">
          <div className="flex min-w-0 items-center gap-3">
            <ServerCog className="h-5 w-5 shrink-0 text-primary" />
            <h2 className="truncate text-lg font-semibold text-foreground">
              {t('externalMcpManager.title')}
            </h2>
          </div>
          <button
            type="button"
            onClick={onClose}
            className="rounded-lg p-2 text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
            aria-label={t('common.close')}
          >
            <X className="h-5 w-5" />
          </button>
        </div>

        <div className="flex-1 overflow-y-auto overflow-x-hidden p-4">
          <button
            type="button"
            onClick={openCreateDialog}
            className="mb-4 flex w-full items-center justify-center gap-2 rounded-lg border-2 border-dashed border-border p-4 text-sm text-muted-foreground transition-colors hover:border-primary hover:text-primary"
          >
            <Plus className="h-4 w-4" />
            <span>{t('externalMcpManager.createButton')}</span>
          </button>

          {error ? (
            <div className="mb-4 rounded-lg border border-red-200 bg-red-50 px-3 py-2 text-sm text-red-700 dark:border-red-900 dark:bg-red-950/40 dark:text-red-200">
              {error}
            </div>
          ) : null}

          {loading ? (
            <div className="flex items-center justify-center gap-2 py-10 text-sm text-muted-foreground">
              <Loader2 className="h-4 w-4 animate-spin" />
              <span>{t('common.loading')}</span>
            </div>
          ) : sortedConfigs.length === 0 ? (
            <div className="rounded-lg border border-border py-10 text-center text-sm text-muted-foreground">
              {t('externalMcpManager.empty')}
            </div>
          ) : (
            <div className="space-y-3">
              {sortedConfigs.map((config) => (
                <ExternalMcpConfigRow
                  key={config.id}
                  config={config}
                  onEdit={openEditDialog}
                  onDelete={(item) => void handleDelete(item)}
                  onToggle={(item) => void toggleEnabled(item)}
                />
              ))}
            </div>
          )}
        </div>
      </div>

      <ManagerFormDialog
        open={formOpen}
        title={editingConfig ? t('externalMcpManager.form.editTitle') : t('externalMcpManager.form.createTitle')}
        widthClassName="max-w-2xl"
        onClose={resetForm}
      >
        {formError ? (
          <div className="mb-3 rounded-lg border border-red-200 bg-red-50 px-3 py-2 text-sm text-red-700 dark:border-red-900 dark:bg-red-950/40 dark:text-red-200">
            {formError}
          </div>
        ) : null}
        <ExternalMcpConfigForm
          data={formData}
          saving={saving}
          editing={Boolean(editingConfig)}
          onChange={updateFormData}
          onCancel={resetForm}
          onSubmit={handleSubmit}
        />
      </ManagerFormDialog>
    </>
  );
};

interface RowProps {
  config: TaskRunnerExternalMcpConfig;
  onEdit: (config: TaskRunnerExternalMcpConfig) => void;
  onDelete: (config: TaskRunnerExternalMcpConfig) => void;
  onToggle: (config: TaskRunnerExternalMcpConfig) => void;
}

const ExternalMcpConfigRow = ({
  config,
  onEdit,
  onDelete,
  onToggle,
}: RowProps) => {
  const { t } = useI18n();
  const endpoint = config.transport === 'http'
    ? config.url || '-'
    : [config.command, ...(config.args || [])].filter(Boolean).join(' ') || '-';
  const owner = config.owner_display_name
    || config.owner_username
    || config.creator_display_name
    || config.creator_username
    || config.owner_user_id
    || config.creator_user_id
    || '-';

  return (
    <div className="rounded-lg border border-border bg-card p-4">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0 flex-1">
          <div className="flex min-w-0 flex-wrap items-center gap-2">
            <h3 className="truncate text-sm font-semibold text-foreground" title={config.name}>
              {config.name || config.id}
            </h3>
            <span className="rounded-full bg-accent px-2 py-0.5 text-xs text-accent-foreground">
              {config.transport === 'http' ? 'http' : 'stdio'}
            </span>
            <span className={`rounded-full px-2 py-0.5 text-xs ${
              config.enabled
                ? 'bg-emerald-100 text-emerald-700 dark:bg-emerald-950/50 dark:text-emerald-200'
                : 'bg-muted text-muted-foreground'
            }`}
            >
              {config.enabled ? t('common.enabled') : t('common.disabled')}
            </span>
          </div>
          <p className="mt-2 truncate font-mono text-xs text-muted-foreground" title={endpoint}>
            {endpoint}
          </p>
          <p className="mt-2 truncate text-xs text-muted-foreground" title={owner}>
            {t('externalMcpManager.owner')}: {owner}
          </p>
        </div>
        <div className="flex shrink-0 items-center gap-1">
          <button
            type="button"
            onClick={() => onToggle(config)}
            className="rounded-lg p-2 text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
            title={config.enabled ? t('common.disable') : t('common.enable')}
            aria-label={config.enabled ? t('common.disable') : t('common.enable')}
          >
            {config.enabled ? <PowerOff className="h-4 w-4" /> : <Power className="h-4 w-4" />}
          </button>
          <button
            type="button"
            onClick={() => onEdit(config)}
            className="rounded-lg p-2 text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
            title={t('common.edit')}
            aria-label={t('common.edit')}
          >
            <Pencil className="h-4 w-4" />
          </button>
          <button
            type="button"
            onClick={() => onDelete(config)}
            className="rounded-lg p-2 text-muted-foreground transition-colors hover:bg-accent hover:text-red-600"
            title={t('common.delete')}
            aria-label={t('common.delete')}
          >
            <Trash2 className="h-4 w-4" />
          </button>
        </div>
      </div>
    </div>
  );
};

interface FormProps {
  data: FormState;
  saving: boolean;
  editing: boolean;
  onChange: (patch: Partial<FormState>) => void;
  onCancel: () => void;
  onSubmit: (event: React.FormEvent<HTMLFormElement>) => void;
}

const ExternalMcpConfigForm = ({
  data,
  saving,
  editing,
  onChange,
  onCancel,
  onSubmit,
}: FormProps) => {
  const { t } = useI18n();
  return (
    <form className="space-y-4" onSubmit={onSubmit}>
      <div className="grid gap-4 sm:grid-cols-[1fr_160px]">
        <label className="block">
          <span className="mb-2 block text-sm font-medium text-foreground">
            {t('externalMcpManager.form.name')}
          </span>
          <input
            type="text"
            value={data.name}
            onChange={(event) => onChange({ name: event.target.value })}
            className="w-full rounded-md border border-input bg-background px-3 py-2 text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
            required
            autoFocus
          />
        </label>
        <label className="block">
          <span className="mb-2 block text-sm font-medium text-foreground">
            {t('externalMcpManager.form.transport')}
          </span>
          <select
            value={data.transport}
            onChange={(event) => onChange({ transport: event.target.value as TaskRunnerExternalMcpTransport })}
            className="w-full rounded-md border border-input bg-background px-3 py-2 text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
          >
            <option value="stdio">stdio</option>
            <option value="http">http</option>
          </select>
        </label>
      </div>

      <label className="flex items-center justify-between rounded-lg border border-border bg-muted/40 px-3 py-2">
        <span className="text-sm font-medium text-foreground">{t('externalMcpManager.form.enabled')}</span>
        <input
          type="checkbox"
          checked={data.enabled}
          onChange={(event) => onChange({ enabled: event.target.checked })}
          className="h-4 w-4 accent-primary"
        />
      </label>

      {data.transport === 'http' ? (
        <>
          <label className="block">
            <span className="mb-2 block text-sm font-medium text-foreground">URL</span>
            <input
              type="url"
              value={data.url}
              onChange={(event) => onChange({ url: event.target.value })}
              className="w-full rounded-md border border-input bg-background px-3 py-2 text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
              placeholder="http://127.0.0.1:3001/mcp"
              required
            />
          </label>
          <label className="block">
            <span className="mb-2 block text-sm font-medium text-foreground">
              {t('externalMcpManager.form.headers')}
            </span>
            <textarea
              value={data.headersText}
              onChange={(event) => onChange({ headersText: event.target.value })}
              className="min-h-[112px] w-full rounded-md border border-input bg-background px-3 py-2 font-mono text-sm text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
              placeholder='{"Authorization": "Bearer ..."}'
            />
          </label>
        </>
      ) : (
        <>
          <label className="block">
            <span className="mb-2 block text-sm font-medium text-foreground">
              {t('externalMcpManager.form.command')}
            </span>
            <input
              type="text"
              value={data.command}
              onChange={(event) => onChange({ command: event.target.value })}
              className="w-full rounded-md border border-input bg-background px-3 py-2 text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
              placeholder="npx"
              required
            />
          </label>
          <label className="block">
            <span className="mb-2 block text-sm font-medium text-foreground">
              {t('externalMcpManager.form.args')}
            </span>
            <textarea
              value={data.argsText}
              onChange={(event) => onChange({ argsText: event.target.value })}
              className="min-h-[112px] w-full rounded-md border border-input bg-background px-3 py-2 font-mono text-sm text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
              placeholder={'-y\n@modelcontextprotocol/server-filesystem\n/Users/me/project'}
            />
          </label>
          <label className="block">
            <span className="mb-2 block text-sm font-medium text-foreground">cwd</span>
            <input
              type="text"
              value={data.cwd}
              onChange={(event) => onChange({ cwd: event.target.value })}
              className="w-full rounded-md border border-input bg-background px-3 py-2 text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
              placeholder="/Users/me/project"
            />
          </label>
          <label className="block">
            <span className="mb-2 block text-sm font-medium text-foreground">
              {t('externalMcpManager.form.env')}
            </span>
            <textarea
              value={data.envText}
              onChange={(event) => onChange({ envText: event.target.value })}
              className="min-h-[112px] w-full rounded-md border border-input bg-background px-3 py-2 font-mono text-sm text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
              placeholder='{"TOKEN": "..."}'
            />
          </label>
        </>
      )}

      <div className="flex items-center justify-end gap-2">
        <button
          type="button"
          onClick={onCancel}
          className="rounded-lg bg-muted px-3 py-2 text-sm transition-colors hover:bg-accent"
        >
          {t('common.cancel')}
        </button>
        <button
          type="submit"
          disabled={saving}
          className="inline-flex items-center gap-2 rounded-lg bg-primary px-3 py-2 text-sm text-primary-foreground transition-opacity hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-60"
        >
          {saving ? <Loader2 className="h-4 w-4 animate-spin" /> : null}
          <span>{editing ? t('common.save') : t('common.create')}</span>
        </button>
      </div>
    </form>
  );
};

function buildPayload(data: FormState): CreateTaskRunnerExternalMcpConfigPayload {
  const transport = data.transport === 'http' ? 'http' : 'stdio';
  const payload = {
    name: data.name.trim(),
    transport,
    enabled: data.enabled,
  };
  if (transport === 'http') {
    return {
      ...payload,
      command: '',
      args: [],
      url: data.url.trim(),
      headers: parseStringMap(data.headersText, 'Headers JSON'),
      env: {},
      cwd: '',
    };
  }
  return {
    ...payload,
    command: data.command.trim(),
    args: parseLines(data.argsText),
    url: '',
    headers: {},
    env: parseStringMap(data.envText, 'Env JSON'),
    cwd: data.cwd.trim(),
  };
}

function parseLines(value: string): string[] {
  return value
    .split('\n')
    .map((item) => item.trim())
    .filter(Boolean);
}

function parseStringMap(value: string, label: string): Record<string, string> {
  const trimmed = value.trim();
  if (!trimmed) {
    return {};
  }
  let parsed: unknown;
  try {
    parsed = JSON.parse(trimmed);
  } catch {
    throw new Error(`${label} must be valid JSON`);
  }
  if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) {
    throw new Error(`${label} must be a JSON object`);
  }
  return Object.fromEntries(
    Object.entries(parsed as Record<string, unknown>)
      .map(([key, item]) => [key.trim(), String(item).trim()])
      .filter(([key]) => key.length > 0),
  );
}

function stringifyMap(value?: Record<string, string> | null): string {
  if (!value || Object.keys(value).length === 0) {
    return '';
  }
  return JSON.stringify(value, null, 2);
}

export default TaskRunnerExternalMcpManager;
