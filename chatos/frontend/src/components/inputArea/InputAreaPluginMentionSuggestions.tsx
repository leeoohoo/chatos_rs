// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useI18n } from '../../i18n/I18nProvider';
import type { TaskRunnerSelectablePluginResponse } from '../../lib/api/client/types';
import { cn } from '../../lib/utils';

export function InputAreaPluginMentionSuggestions({
  open,
  loading,
  suggestions,
  activeIndex,
  onActiveIndexChange,
  onSelect,
}: {
  open: boolean;
  loading: boolean;
  suggestions: TaskRunnerSelectablePluginResponse[];
  activeIndex: number;
  onActiveIndexChange: (index: number) => void;
  onSelect: (plugin: TaskRunnerSelectablePluginResponse) => void;
}) {
  const { t } = useI18n();
  if (!open) {
    return null;
  }

  return (
    <div
      className="absolute bottom-full left-0 right-0 z-40 mb-2 overflow-hidden rounded-lg border bg-popover text-popover-foreground shadow-xl"
      role="listbox"
      aria-label={t('inputArea.plugin.mentionSuggestions')}
    >
      <div className="border-b px-3 py-2 text-xs text-muted-foreground">
        {t('inputArea.plugin.mentionSuggestionHint')}
      </div>
      <div className="max-h-72 overflow-y-auto p-1">
        {loading && suggestions.length === 0 ? (
          <div className="px-3 py-5 text-center text-sm text-muted-foreground">
            {t('inputArea.plugin.loadingMentions')}
          </div>
        ) : null}
        {!loading && suggestions.length === 0 ? (
          <div className="px-3 py-5 text-center text-sm text-muted-foreground">
            {t('inputArea.plugin.noMentionMatches')}
          </div>
        ) : null}
        {suggestions.map((plugin, index) => (
          <button
            key={plugin.id}
            type="button"
            role="option"
            aria-selected={index === activeIndex}
            onMouseEnter={() => onActiveIndexChange(index)}
            onMouseDown={(event) => event.preventDefault()}
            onClick={() => onSelect(plugin)}
            className={cn(
              'flex w-full items-start gap-3 rounded-md px-3 py-2 text-left transition-colors',
              index === activeIndex ? 'bg-accent text-accent-foreground' : 'hover:bg-muted/60',
            )}
          >
            <span className="min-w-0 flex-1">
              <span className="flex flex-wrap items-center gap-2">
                <span className="font-mono text-sm font-medium text-primary">
                  @{plugin.plugin_key}
                </span>
                <span className="text-sm">{plugin.display_name}</span>
                <span className="text-[11px] text-muted-foreground">v{plugin.version}</span>
              </span>
              <span className="mt-0.5 block text-xs text-muted-foreground">
                {plugin.description || t('inputArea.plugin.noDescription')}
              </span>
              <span className="mt-1 block text-[11px] text-muted-foreground">
                {t('inputArea.plugin.componentCount', {
                  count: Array.isArray(plugin.component_keys) ? plugin.component_keys.length : 0,
                })}
              </span>
            </span>
          </button>
        ))}
      </div>
    </div>
  );
}
