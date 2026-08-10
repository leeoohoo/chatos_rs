// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useI18n } from '../../../i18n/I18nProvider';
import { cn } from '../../../lib/utils';
import type { useTaskPluginPicker } from '../useTaskPluginPicker';

type PluginPickerModel = ReturnType<typeof useTaskPluginPicker>;

export const InputAreaPluginPicker = ({
  pluginPicker,
  disabled,
}: {
  pluginPicker: PluginPickerModel;
  disabled: boolean;
}) => {
  const { t } = useI18n();
  if (!pluginPicker.enabled) {
    return null;
  }

  return (
    <div ref={pluginPicker.pickerRef} className="relative flex-shrink-0">
      <button
        type="button"
        onClick={pluginPicker.toggleOpen}
        disabled={disabled}
        className={cn(
          'flex items-center gap-1 rounded-md px-2 py-1 text-xs transition-colors',
          pluginPicker.selectedPluginIds.length > 0
            ? 'bg-primary text-primary-foreground hover:bg-primary/90'
            : 'bg-muted text-muted-foreground hover:text-foreground',
          disabled && 'cursor-not-allowed opacity-50',
        )}
        title={t('inputArea.plugin.chooseTitle')}
      >
        <svg className="h-4 w-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M8 3v3m8-3v3M9 18v3m6-3v3M3 8h3m-3 8h3m12-8h3m-3 8h3M8 8h8v8H8z" />
        </svg>
        <span>{t('inputArea.plugin.button')}</span>
        {pluginPicker.selectedPluginIds.length > 0 ? (
          <span>({pluginPicker.selectedPluginIds.length})</span>
        ) : null}
      </button>

      {pluginPicker.open ? (
        <div className="absolute bottom-full left-0 z-50 mb-2 w-[min(92vw,560px)] rounded-lg border bg-popover p-3 text-popover-foreground shadow-xl">
          <div className="mb-3 flex items-start justify-between gap-3">
            <div>
              <div className="text-sm font-medium">{t('inputArea.plugin.title')}</div>
              <div className="mt-1 text-xs text-muted-foreground">
                {t('inputArea.plugin.descriptionProject')}
              </div>
            </div>
            <button
              type="button"
              onClick={pluginPicker.close}
              className="rounded p-1 text-muted-foreground hover:bg-muted hover:text-foreground"
              aria-label={t('inputArea.plugin.done')}
            >
              ×
            </button>
          </div>

          <input
            value={pluginPicker.search}
            onChange={(event) => pluginPicker.setSearch(event.target.value)}
            placeholder={t('inputArea.plugin.search')}
            className="mt-3 w-full rounded-md border bg-background px-3 py-2 text-sm text-foreground outline-none focus:ring-1 focus:ring-primary"
          />

          {pluginPicker.error ? (
            <div className="mt-2 text-xs text-destructive">{pluginPicker.error}</div>
          ) : null}
          {pluginPicker.commandArgumentIssue ? (
            <div className="mt-2 text-xs text-destructive">
              {t('inputArea.plugin.commandArgumentsInvalid')}
            </div>
          ) : null}

          <div className="mt-3 max-h-64 space-y-2 overflow-y-auto pr-1">
            {pluginPicker.loading ? (
              <div className="py-6 text-center text-sm text-muted-foreground">
                {t('inputArea.plugin.loading')}
              </div>
            ) : null}
            {!pluginPicker.loading && pluginPicker.filteredPlugins.length === 0 ? (
              <div className="py-6 text-center text-sm text-muted-foreground">
                {t('inputArea.plugin.emptyProject')}
              </div>
            ) : null}
            {!pluginPicker.loading && pluginPicker.filteredPlugins.map((plugin) => {
              const selected = pluginPicker.selectedPluginIds.includes(plugin.id);
              return (
                <div
                  key={plugin.id}
                  className={cn(
                    'rounded-lg border p-3 transition-colors',
                    selected ? 'border-primary bg-primary/5' : 'hover:bg-muted/60',
                  )}
                >
                  <label className="flex cursor-pointer items-start gap-3">
                    <input
                      type="checkbox"
                      checked={selected}
                      onChange={() => pluginPicker.togglePlugin(plugin.id)}
                      className="mt-1"
                    />
                    <span className="min-w-0 flex-1">
                      <span className="flex flex-wrap items-center gap-2">
                        <span className="font-medium">{plugin.display_name}</span>
                        <span className="rounded bg-muted px-1.5 py-0.5 text-[10px] text-muted-foreground">
                          v{plugin.version}
                        </span>
                        {plugin.plugin_key === 'browser' ? (
                          <span className="rounded bg-blue-500/10 px-1.5 py-0.5 text-[10px] text-blue-600 dark:text-blue-400">
                            Browser
                          </span>
                        ) : null}
                        <span className="rounded bg-violet-500/10 px-1.5 py-0.5 text-[10px] text-violet-700 dark:text-violet-300">
                          {plugin.execution_type === 'cloud'
                            ? '云端'
                            : plugin.execution_type === 'portable'
                              ? '可移植'
                              : plugin.execution_type === 'hybrid'
                                ? 'Hybrid'
                                : '本地'}
                        </span>
                        {plugin.requires_device ? (
                          <span className="rounded bg-amber-500/10 px-1.5 py-0.5 text-[10px] text-amber-700 dark:text-amber-300">
                            需要设备
                          </span>
                        ) : null}
                      </span>
                      <span className="mt-1 block text-xs text-muted-foreground">
                        {plugin.description}
                      </span>
                      {plugin.component_keys.length > 0 ? (
                        <span className="mt-1 block truncate text-[10px] text-muted-foreground">
                          {plugin.component_keys.join(' · ')}
                        </span>
                      ) : null}
                      {(Array.isArray(plugin.components) ? plugin.components : []).length > 0 ? (
                        <span className="mt-2 block space-y-1">
                          {(Array.isArray(plugin.components) ? plugin.components : []).map((component) => (
                            <span
                              key={component.component_key}
                              className="block rounded border bg-background/60 px-2 py-1 text-[10px] text-muted-foreground"
                            >
                              <span className="flex flex-wrap items-center gap-1.5">
                                <span className="font-mono text-foreground">
                                  {component.component_key}
                                </span>
                                <span>{component.kind}</span>
                                <span>
                                  {component.execution_host === 'cloud'
                                    ? '云端'
                                    : component.execution_host === 'portable'
                                      ? '可移植'
                                      : '本地'}
                                </span>
                                <span>{component.available ? 'ready' : component.status}</span>
                                <span>
                                  {component.prepare_provider === 'task_runner_cloud'
                                    ? '云端任务'
                                    : 'Local Connector'}
                                </span>
                                {component.requires_workspace ? <span>需要工作区</span> : null}
                              </span>
                              {component.content_sha256 ? (
                                <span className="mt-0.5 block truncate font-mono">
                                  sha256:{component.content_sha256}
                                </span>
                              ) : null}
                              {component.reason ? (
                                <span className="mt-0.5 block text-destructive">
                                  {component.reason}
                                </span>
                              ) : null}
                            </span>
                          ))}
                        </span>
                      ) : null}
                    </span>
                  </label>

                  {(Array.isArray(plugin.commands) ? plugin.commands : []).length > 0 ? (
                    <div className="mt-3 space-y-2 border-t pt-2">
                      <div className="text-[11px] font-medium text-muted-foreground">
                        {t('inputArea.plugin.commands')}
                      </div>
                      {(Array.isArray(plugin.commands) ? plugin.commands : []).map((command) => {
                        const invocation = pluginPicker.selectedCommandInvocations.find((item) => (
                          item.plugin_id === plugin.id && item.command_id === command.command_id
                        ));
                        const commandSelected = Boolean(invocation);
                        const commandKey = `${plugin.id}\u0000${command.command_id}`;
                        const argumentsInvalid = pluginPicker.commandArgumentIssue?.key === commandKey;
                        return (
                          <div
                            key={command.command_id}
                            className={cn(
                              'rounded-md border bg-background/70 p-2',
                              commandSelected && 'border-primary/50',
                            )}
                          >
                            <label className="flex cursor-pointer items-start gap-2">
                              <input
                                type="checkbox"
                                checked={commandSelected}
                                onChange={() => pluginPicker.toggleCommand(
                                  plugin.id,
                                  command.command_id,
                                )}
                                className="mt-0.5"
                              />
                              <span className="min-w-0 flex-1">
                                <span className="flex flex-wrap items-center gap-1.5">
                                  <span className="font-mono text-xs text-primary">
                                    /{command.command_id}
                                  </span>
                                  <span className="text-xs font-medium">
                                    {command.display_name}
                                  </span>
                                  {command.requires_confirmation ? (
                                    <span className="rounded bg-amber-500/10 px-1.5 py-0.5 text-[10px] text-amber-700 dark:text-amber-300">
                                      {t('inputArea.plugin.confirmationRequired')}
                                    </span>
                                  ) : null}
                                </span>
                                {command.description ? (
                                  <span className="mt-0.5 block text-[11px] text-muted-foreground">
                                    {command.description}
                                  </span>
                                ) : null}
                              </span>
                            </label>
                            {commandSelected ? (
                              <input
                                value={typeof invocation?.arguments === 'string'
                                  ? invocation.arguments
                                  : ''}
                                onChange={(event) => pluginPicker.setCommandArguments(
                                  plugin.id,
                                  command.command_id,
                                  event.target.value,
                                )}
                                placeholder={command.argument_hint
                                  || t('inputArea.plugin.commandArgumentsOptional')}
                                aria-label={t('inputArea.plugin.commandArgumentsLabel', {
                                  command: command.display_name,
                                })}
                                className={cn(
                                  'mt-2 w-full rounded-md border bg-background px-2 py-1.5 font-mono text-xs text-foreground outline-none focus:ring-1 focus:ring-primary',
                                  argumentsInvalid && 'border-destructive focus:ring-destructive',
                                )}
                              />
                            ) : null}
                          </div>
                        );
                      })}
                    </div>
                  ) : null}
                </div>
              );
            })}
          </div>

          <div className="mt-3 flex items-center justify-between border-t pt-3">
            <button
              type="button"
              onClick={pluginPicker.clearSelectedPlugins}
              disabled={pluginPicker.selectedPluginIds.length === 0}
              className="text-xs text-muted-foreground hover:text-foreground disabled:opacity-40"
            >
              {t('inputArea.plugin.clear')}
            </button>
            <button
              type="button"
              onClick={pluginPicker.close}
              className="rounded-md bg-primary px-3 py-1.5 text-xs text-primary-foreground hover:bg-primary/90"
            >
              {t('inputArea.plugin.done')}
            </button>
          </div>
        </div>
      ) : null}
    </div>
  );
};
