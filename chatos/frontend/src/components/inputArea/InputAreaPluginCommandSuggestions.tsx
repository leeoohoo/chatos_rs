// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useI18n } from '../../i18n/I18nProvider';
import { cn } from '../../lib/utils';
import type { TaskPluginCommandOption } from './pluginCommands';

export function InputAreaPluginCommandSuggestions({
  open,
  loading,
  suggestions,
  activeIndex,
  onActiveIndexChange,
  onSelect,
}: {
  open: boolean;
  loading: boolean;
  suggestions: TaskPluginCommandOption[];
  activeIndex: number;
  onActiveIndexChange: (index: number) => void;
  onSelect: (option: TaskPluginCommandOption) => void;
}) {
  const { t } = useI18n();
  if (!open) {
    return null;
  }

  return (
    <div
      className="absolute bottom-full left-0 right-0 z-40 mb-2 overflow-hidden rounded-lg border bg-popover text-popover-foreground shadow-xl"
      role="listbox"
      aria-label={t('inputArea.plugin.commandSuggestions')}
    >
      <div className="border-b px-3 py-2 text-xs text-muted-foreground">
        {t('inputArea.plugin.commandSuggestionHint')}
      </div>
      <div className="max-h-72 overflow-y-auto p-1">
        {loading && suggestions.length === 0 ? (
          <div className="px-3 py-5 text-center text-sm text-muted-foreground">
            {t('inputArea.plugin.loadingCommands')}
          </div>
        ) : null}
        {!loading && suggestions.length === 0 ? (
          <div className="px-3 py-5 text-center text-sm text-muted-foreground">
            {t('inputArea.plugin.noCommandMatches')}
          </div>
        ) : null}
        {suggestions.map((option, index) => (
          <button
            key={option.key}
            type="button"
            role="option"
            aria-selected={index === activeIndex}
            onMouseEnter={() => onActiveIndexChange(index)}
            onMouseDown={(event) => event.preventDefault()}
            onClick={() => onSelect(option)}
            className={cn(
              'flex w-full items-start gap-3 rounded-md px-3 py-2 text-left transition-colors',
              index === activeIndex ? 'bg-accent text-accent-foreground' : 'hover:bg-muted/60',
            )}
          >
            <span className="min-w-0 flex-1">
              <span className="flex flex-wrap items-center gap-2">
                <span className="font-mono text-sm font-medium text-primary">
                  /{option.command.command_id}
                </span>
                <span className="text-sm">{option.command.display_name}</span>
                {option.command.requires_confirmation ? (
                  <span className="rounded bg-amber-500/10 px-1.5 py-0.5 text-[10px] text-amber-700 dark:text-amber-300">
                    {t('inputArea.plugin.confirmationRequired')}
                  </span>
                ) : null}
              </span>
              <span className="mt-0.5 block text-xs text-muted-foreground">
                {option.plugin.display_name}
                {option.command.description ? ` · ${option.command.description}` : ''}
              </span>
              {option.command.argument_hint ? (
                <span className="mt-1 block font-mono text-[11px] text-muted-foreground">
                  /{option.command.command_id} {option.command.argument_hint}
                </span>
              ) : null}
            </span>
          </button>
        ))}
      </div>
    </div>
  );
}
