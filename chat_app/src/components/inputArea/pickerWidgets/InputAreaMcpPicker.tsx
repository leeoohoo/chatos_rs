import React from 'react';

import { cn } from '../../../lib/utils';
import type { SelectableMcpConfig } from '../useMcpSelection';

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
  hasDirectoryContext: boolean;
  hasRemoteContext: boolean;
  isProjectRequiredMcpId: (id: string) => boolean;
  isRemoteRequiredMcpId: (id: string) => boolean;
  sanitizedEnabledMcpIds: string[];
  onRefreshMcpConfigs: () => void;
  onSelectAllMcp: () => void;
  onToggleMcpSelection: (mcpId: string) => void;
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
  hasDirectoryContext,
  hasRemoteContext,
  isProjectRequiredMcpId,
  isRemoteRequiredMcpId,
  sanitizedEnabledMcpIds,
  onRefreshMcpConfigs,
  onSelectAllMcp,
  onToggleMcpSelection,
}) => (
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
        title={mcpEnabled ? 'MCP 已开启' : 'MCP 已关闭'}
      >
        MCP {mcpEnabled ? '开' : '关'}
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
        title="选择当前对话可用 MCP"
      >
        MCP 选择
        <span className="ml-1">▾</span>
      </button>
    </div>
    {mcpPickerOpen && (
      <div className="absolute left-0 bottom-full mb-2 z-30 w-80 max-w-[calc(100vw-2rem)] bg-popover text-popover-foreground border rounded-md shadow-lg">
        <div className="px-3 py-2 border-b flex items-center justify-between gap-2">
          <div className="min-w-0">
            <div className="text-xs font-medium">MCP 选择</div>
            <div className="text-[11px] text-muted-foreground">
              {mcpEnabled
                ? (isAllMcpSelected
                  ? `已选全部 (${selectableMcpIds.length || 0})`
                  : `已选 ${selectedMcpCount}/${selectableMcpIds.length || 0}`)
                : 'MCP 总开关已关闭'}
            </div>
          </div>
          <button
            type="button"
            onClick={onRefreshMcpConfigs}
            disabled={mcpConfigsLoading}
            className="px-2 py-0.5 text-[11px] rounded border text-muted-foreground hover:text-foreground hover:bg-accent disabled:opacity-50"
          >
            刷新
          </button>
        </div>

        <div className="max-h-72 overflow-auto py-1">
          {mcpConfigsLoading ? (
            <div className="px-3 py-3 text-xs text-muted-foreground">加载中...</div>
          ) : mcpConfigsError ? (
            <div className="px-3 py-3 text-xs text-destructive">{mcpConfigsError}</div>
          ) : availableMcpConfigs.length === 0 ? (
            <div className="px-3 py-3 text-xs text-muted-foreground">暂无可用 MCP</div>
          ) : (
            <>
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
                <span>全部可用</span>
              </label>

              {builtinMcpConfigs.length > 0 && (
                <>
                  <div className="px-3 pt-2 pb-1 text-[11px] uppercase tracking-wide text-muted-foreground">
                    内置 MCP
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
                          <span className="text-[11px] text-muted-foreground">需选择目录</span>
                        )}
                        {remoteDisabled && (
                          <span className="text-[11px] text-muted-foreground">需选择服务器</span>
                        )}
                      </label>
                    );
                  })}
                </>
              )}

              {customMcpConfigs.length > 0 && (
                <>
                  <div className="px-3 pt-2 pb-1 text-[11px] uppercase tracking-wide text-muted-foreground">
                    自定义 MCP
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
