import React from 'react';

import { useI18n } from '../../../i18n/I18nProvider';
import { cn } from '../../../lib/utils';
import type { McpToolsetPreset, SelectableMcpConfig } from '../useMcpSelection';

interface InputAreaMcpPickerProps {
  mcpPickerRef: React.RefObject<HTMLDivElement>;
  mcpEnabled: boolean;
  onMcpEnabledChange?: (enabled: boolean) => void;
  disabled: boolean;
  isStreaming: boolean;
  isStopping: boolean;
  onToggleMcpPicker: () => void;
  mcpPickerOpen: boolean;
  isAllMcpSelected: boolean;
  selectableMcpIds: string[];
  selectedMcpCount: number;
  mcpConfigsLoading: boolean;
  mcpConfigsError: string | null;
  availableMcpConfigs: SelectableMcpConfig[];
  builtinMcpConfigs: SelectableMcpConfig[];
  customMcpConfigs: SelectableMcpConfig[];
  mcpToolsetPresets: McpToolsetPreset[];
  projectScopeKey: string | null;
  hasProjectMcpDefault: boolean;
  hasDirectoryContext: boolean;
  hasRemoteContext: boolean;
  isProjectRequiredMcpId: (id: string) => boolean;
  isRemoteRequiredMcpId: (id: string) => boolean;
  sanitizedEnabledMcpIds: string[];
  onRefreshMcpConfigs: () => void;
  onSelectAllMcp: () => void;
  onToggleMcpSelection: (mcpId: string) => void;
  onApplyMcpToolsetPreset: (presetId: string) => void;
  onSaveProjectMcpDefault: () => void;
  onApplyProjectMcpDefault: () => void;
}

export const InputAreaMcpPicker: React.FC<InputAreaMcpPickerProps> = ({
  mcpPickerRef,
  mcpEnabled,
  onMcpEnabledChange,
  disabled,
  isStreaming,
  isStopping,
  onToggleMcpPicker,
  mcpPickerOpen,
  isAllMcpSelected,
  selectableMcpIds,
  selectedMcpCount,
  mcpConfigsLoading,
  mcpConfigsError,
  availableMcpConfigs,
  builtinMcpConfigs,
  customMcpConfigs,
  mcpToolsetPresets,
  projectScopeKey,
  hasProjectMcpDefault,
  hasDirectoryContext,
  hasRemoteContext,
  isProjectRequiredMcpId,
  isRemoteRequiredMcpId,
  sanitizedEnabledMcpIds,
  onRefreshMcpConfigs,
  onSelectAllMcp,
  onToggleMcpSelection,
  onApplyMcpToolsetPreset,
  onSaveProjectMcpDefault,
  onApplyProjectMcpDefault,
}) => {
  const { t } = useI18n();

  return (
  <div className="relative flex-shrink-0" ref={mcpPickerRef}>
    <div className="flex items-center gap-1">
      <button
        type="button"
        onClick={() => onMcpEnabledChange?.(!mcpEnabled)}
        disabled={disabled || isStreaming || isStopping}
        className={cn(
          'flex-shrink-0 px-2 py-1 text-xs rounded-md transition-colors',
          mcpEnabled
            ? 'bg-primary text-primary-foreground hover:bg-primary/90'
            : 'bg-muted text-muted-foreground hover:text-foreground',
          (disabled || isStreaming || isStopping) && 'opacity-50 cursor-not-allowed',
        )}
        title={mcpEnabled ? t('inputArea.mcp.enabledTitle') : t('inputArea.mcp.disabledTitle')}
      >
        {t('inputArea.mcp.button', { state: mcpEnabled ? t('composer.toggle.on') : t('composer.toggle.off') })}
      </button>
      <button
        type="button"
        onClick={onToggleMcpPicker}
        disabled={disabled || isStreaming || isStopping}
        className={cn(
          'px-2 py-1 rounded-md border text-xs transition-colors',
          'text-muted-foreground hover:text-foreground hover:bg-accent',
          (disabled || isStreaming || isStopping) && 'opacity-50 cursor-not-allowed',
        )}
        title={t('inputArea.mcp.selectTitle')}
      >
        {t('inputArea.mcp.selectButton')}
        <span className="ml-1">▾</span>
      </button>
    </div>
    {mcpPickerOpen && (
      <div className="absolute left-0 bottom-full mb-2 z-30 w-80 max-w-[calc(100vw-2rem)] bg-popover text-popover-foreground border rounded-md shadow-lg">
        <div className="px-3 py-2 border-b flex items-center justify-between gap-2">
          <div className="min-w-0">
            <div className="text-xs font-medium">{t('inputArea.mcp.selectButton')}</div>
            <div className="text-[11px] text-muted-foreground">
              {mcpEnabled
                ? (isAllMcpSelected
                  ? t('inputArea.mcp.selectedAll', { count: selectableMcpIds.length || 0 })
                  : t('inputArea.mcp.selectedCount', { selected: selectedMcpCount, total: selectableMcpIds.length || 0 }))
                : t('inputArea.mcp.offSummary')}
            </div>
          </div>
          <button
            type="button"
            onClick={onRefreshMcpConfigs}
            disabled={mcpConfigsLoading}
            className="px-2 py-0.5 text-[11px] rounded border text-muted-foreground hover:text-foreground hover:bg-accent disabled:opacity-50"
          >
            {t('common.refresh')}
          </button>
        </div>

        <div className="max-h-72 overflow-auto py-1">
          {mcpConfigsLoading ? (
            <div className="px-3 py-3 text-xs text-muted-foreground">{t('common.loading')}</div>
          ) : mcpConfigsError ? (
            <div className="px-3 py-3 text-xs text-destructive">{mcpConfigsError}</div>
          ) : availableMcpConfigs.length === 0 ? (
            <div className="px-3 py-3 text-xs text-muted-foreground">{t('inputArea.mcp.empty')}</div>
          ) : (
            <>
              {mcpToolsetPresets.length > 0 && (
                <div className="px-3 pt-2 pb-2 border-b">
                  <div className="text-[11px] uppercase tracking-wide text-muted-foreground">{t('inputArea.mcp.presets')}</div>
                  <div className="mt-2 grid grid-cols-2 gap-1.5">
                    {mcpToolsetPresets.map((preset) => (
                      <button
                        key={preset.id}
                        type="button"
                        onClick={() => onApplyMcpToolsetPreset(preset.id)}
                        disabled={disabled || isStreaming || isStopping || preset.disabled}
                        className={cn(
                          'px-2 py-1 rounded border text-[11px] text-left transition-colors',
                          'hover:bg-accent hover:text-foreground',
                          'disabled:opacity-50 disabled:cursor-not-allowed',
                        )}
                        title={preset.description}
                      >
                        <div className="flex items-center justify-between gap-2">
                          <span className="truncate">{preset.label}</span>
                          <span className="text-[10px] text-muted-foreground">{preset.targetIds.length}</span>
                        </div>
                      </button>
                    ))}
                  </div>
                </div>
              )}

              <div className="px-3 py-2 border-b">
                <div className="text-[11px] uppercase tracking-wide text-muted-foreground">{t('inputArea.mcp.projectDefault')}</div>
                <div className="mt-2 flex items-center gap-1.5">
                  <button
                    type="button"
                    onClick={onSaveProjectMcpDefault}
                    disabled={disabled || isStreaming || isStopping || !projectScopeKey}
                    className={cn(
                      'px-2 py-1 rounded border text-[11px] transition-colors',
                      'hover:bg-accent hover:text-foreground',
                      'disabled:opacity-50 disabled:cursor-not-allowed',
                    )}
                    title={projectScopeKey ? t('inputArea.mcp.saveDefaultTitle') : t('inputArea.mcp.needProjectTitle')}
                  >
                    {t('inputArea.mcp.saveDefault')}
                  </button>
                  <button
                    type="button"
                    onClick={onApplyProjectMcpDefault}
                    disabled={disabled || isStreaming || isStopping || !hasProjectMcpDefault}
                    className={cn(
                      'px-2 py-1 rounded border text-[11px] transition-colors',
                      'hover:bg-accent hover:text-foreground',
                      'disabled:opacity-50 disabled:cursor-not-allowed',
                    )}
                    title={hasProjectMcpDefault ? t('inputArea.mcp.applyDefaultTitle') : t('inputArea.mcp.noDefaultTitle')}
                  >
                    {t('inputArea.mcp.applyDefault')}
                  </button>
                </div>
              </div>

              <label className="w-full px-3 py-2 text-sm flex items-center gap-2 border-b">
                <input
                  type="checkbox"
                  checked={isAllMcpSelected}
                  onChange={() => {
                    if (!mcpEnabled) {
                      onMcpEnabledChange?.(true);
                    }
                    onSelectAllMcp();
                  }}
                  disabled={disabled || isStreaming || isStopping}
                />
                <span>{t('inputArea.mcp.allAvailable')}</span>
              </label>

              {builtinMcpConfigs.length > 0 && (
                <>
                  <div className="px-3 pt-2 pb-1 text-[11px] uppercase tracking-wide text-muted-foreground">
                    {t('inputArea.mcp.builtin')}
                  </div>
                  {builtinMcpConfigs.map((item) => {
                    const projectDisabled = !hasDirectoryContext && isProjectRequiredMcpId(item.id);
                    const remoteDisabled = !hasRemoteContext && isRemoteRequiredMcpId(item.id);
                    const disabledByContext = projectDisabled || remoteDisabled;
                    const checked = !disabledByContext && (isAllMcpSelected || sanitizedEnabledMcpIds.includes(item.id));
                    return (
                      <label
                        key={item.id}
                        className={cn(
                          'w-full px-3 py-1.5 text-sm flex items-center gap-2',
                          disabledByContext ? 'opacity-50 cursor-not-allowed' : 'hover:bg-accent',
                        )}
                      >
                        <input
                          type="checkbox"
                          checked={checked}
                          onChange={() => {
                            if (disabledByContext) {
                              return;
                            }
                            if (!mcpEnabled) {
                              onMcpEnabledChange?.(true);
                            }
                            onToggleMcpSelection(item.id);
                          }}
                          disabled={disabled || isStreaming || isStopping || disabledByContext}
                        />
                        <span className="truncate" title={item.displayName}>{item.displayName}</span>
                        {projectDisabled && (
                          <span className="text-[11px] text-muted-foreground">{t('inputArea.mcp.needDirectory')}</span>
                        )}
                        {remoteDisabled && (
                          <span className="text-[11px] text-muted-foreground">{t('inputArea.mcp.needServer')}</span>
                        )}
                      </label>
                    );
                  })}
                </>
              )}

              {customMcpConfigs.length > 0 && (
                <>
                  <div className="px-3 pt-2 pb-1 text-[11px] uppercase tracking-wide text-muted-foreground">
                    {t('inputArea.mcp.custom')}
                  </div>
                  {customMcpConfigs.map((item) => {
                    const checked = isAllMcpSelected || sanitizedEnabledMcpIds.includes(item.id);
                    return (
                      <label key={item.id} className="w-full px-3 py-1.5 text-sm flex items-center gap-2 hover:bg-accent">
                        <input
                          type="checkbox"
                          checked={checked}
                          onChange={() => {
                            if (!mcpEnabled) {
                              onMcpEnabledChange?.(true);
                            }
                            onToggleMcpSelection(item.id);
                          }}
                          disabled={disabled || isStreaming || isStopping}
                        />
                        <span className="truncate" title={item.displayName}>{item.displayName}</span>
                      </label>
                    );
                  })}
                </>
              )}
            </>
          )}
        </div>
      </div>
    )}
  </div>
  );
};
