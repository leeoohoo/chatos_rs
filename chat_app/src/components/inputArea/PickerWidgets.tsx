import React from 'react';
import { cn } from '../../lib/utils';
import type { FsEntry, Project, RemoteConnection } from '../../types';
import type { SelectableMcpConfig } from './useMcpSelection';

interface InputAreaProjectFilePickerProps {
  allowAttachments: boolean;
  showProjectFilePicker: boolean;
  pickerRef: React.RefObject<HTMLDivElement>;
  disabled: boolean;
  projectFileAttachingPath: string | null;
  projectFilePickerOpen: boolean;
  onTogglePicker: () => void;
  projectName: string;
  projectFilePathLabel: string;
  projectFileFilter: string;
  onProjectFileFilterChange: (value: string) => void;
  projectFileBusy: boolean;
  projectFileKeywordActive: boolean;
  projectFileParent: string | null;
  onLoadProjectFileEntries: (path?: string | null) => void;
  displayedProjectFileEntries: FsEntry[];
  onAttachProjectFile: (entry: FsEntry) => void;
  toRelativeProjectPath: (path: string) => string;
  projectFileSearchTruncated: boolean;
}

export const InputAreaProjectFilePicker: React.FC<InputAreaProjectFilePickerProps> = ({
  allowAttachments,
  showProjectFilePicker,
  pickerRef,
  disabled,
  projectFileAttachingPath,
  projectFilePickerOpen,
  onTogglePicker,
  projectName,
  projectFilePathLabel,
  projectFileFilter,
  onProjectFileFilterChange,
  projectFileBusy,
  projectFileKeywordActive,
  projectFileParent,
  onLoadProjectFileEntries,
  displayedProjectFileEntries,
  onAttachProjectFile,
  toRelativeProjectPath,
  projectFileSearchTruncated,
}) => {
  if (!allowAttachments || !showProjectFilePicker) {
    return null;
  }

  return (
    <div className="relative flex-shrink-0" ref={pickerRef}>
      <button
        type="button"
        onClick={onTogglePicker}
        disabled={disabled || projectFileAttachingPath !== null}
        className={cn(
          'px-2 py-1 rounded-md border text-xs transition-colors',
          'text-muted-foreground hover:text-foreground hover:bg-accent',
          (disabled || projectFileAttachingPath !== null) && 'opacity-50 cursor-not-allowed'
        )}
        title="从当前项目选择文件"
      >
        项目文件
        <span className="ml-1">▾</span>
      </button>
      {projectFilePickerOpen && (
        <div className="absolute left-0 bottom-full mb-2 z-30 w-80 bg-popover text-popover-foreground border rounded-md shadow-lg">
          <div className="px-3 py-2 border-b space-y-2">
            <div className="space-y-1">
              <div className="text-[11px] text-muted-foreground truncate" title={projectName || '当前项目'}>
                项目: {projectName || '当前项目'}
              </div>
              <div className="text-[11px] text-muted-foreground truncate font-mono" title={projectFilePathLabel || '/'}>
                路径: {projectFilePathLabel || '/'}
              </div>
            </div>
            <input
              type="text"
              value={projectFileFilter}
              onChange={(event) => onProjectFileFilterChange(event.target.value)}
              placeholder="筛选文件（不区分大小写，支持模糊）..."
              className="w-full rounded border bg-background px-2 py-1 text-xs outline-none focus:border-primary"
            />
          </div>
          <div className="max-h-64 overflow-auto py-1">
            {projectFileBusy ? (
              <div className="px-3 py-2 text-xs text-muted-foreground">
                {projectFileKeywordActive ? '搜索中...' : '加载中...'}
              </div>
            ) : (
              <>
                {!projectFileKeywordActive && projectFileParent && (
                  <button
                    type="button"
                    className="w-full px-3 py-1.5 text-left text-sm hover:bg-accent"
                    onClick={() => onLoadProjectFileEntries(projectFileParent)}
                  >
                    ..
                  </button>
                )}
                {displayedProjectFileEntries.map((entry) => (
                  <button
                    key={entry.path}
                    type="button"
                    className="w-full px-3 py-1.5 text-left text-sm hover:bg-accent flex items-center justify-between gap-2"
                    onClick={() => onAttachProjectFile(entry)}
                    disabled={projectFileAttachingPath !== null}
                  >
                    <span className="min-w-0 flex-1 truncate">
                      <span className="inline-flex items-center gap-1.5 min-w-0 max-w-full">
                        {entry.isDir ? (
                          <svg className="w-4 h-4 text-muted-foreground shrink-0" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2}>
                            <path strokeLinecap="round" strokeLinejoin="round" d="M3 7a2 2 0 012-2h4l2 2h8a2 2 0 012 2v8a2 2 0 01-2 2H5a2 2 0 01-2-2V7z" />
                          </svg>
                        ) : (
                          <svg className="w-4 h-4 text-muted-foreground shrink-0" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2}>
                            <path strokeLinecap="round" strokeLinejoin="round" d="M7 3h7l5 5v13a1 1 0 01-1 1H7a1 1 0 01-1-1V4a1 1 0 011-1z" />
                            <path strokeLinecap="round" strokeLinejoin="round" d="M14 3v6h6" />
                          </svg>
                        )}
                        <span className="truncate">{entry.name}</span>
                      </span>
                      {projectFileKeywordActive && !entry.isDir && (
                        <span className="block truncate text-[11px] text-muted-foreground">
                          {toRelativeProjectPath(entry.path)}
                        </span>
                      )}
                    </span>
                    {projectFileAttachingPath === entry.path && (
                      <span className="text-[11px] text-muted-foreground">处理中...</span>
                    )}
                  </button>
                ))}
                {displayedProjectFileEntries.length === 0 && !projectFileBusy && (
                  <div className="px-3 py-2 text-xs text-muted-foreground">
                    {projectFileKeywordActive ? '没有匹配的文件' : '当前目录没有可选文件'}
                  </div>
                )}
                {projectFileKeywordActive && projectFileSearchTruncated && (
                  <div className="px-3 py-2 text-[11px] text-muted-foreground border-t">
                    结果过多，已截断显示前 300 条
                  </div>
                )}
              </>
            )}
          </div>
        </div>
      )}
    </div>
  );
};

interface InputAreaProjectSelectorProps {
  showProjectSelector: boolean;
  availableProjects: Project[];
  selectedProjectId?: string | null;
  onProjectChange?: (projectId: string | null) => void;
  disabled: boolean;
  isStreaming: boolean;
  isStopping: boolean;
}

export const InputAreaProjectSelector: React.FC<InputAreaProjectSelectorProps> = ({
  showProjectSelector,
  availableProjects,
  selectedProjectId,
  onProjectChange,
  disabled,
  isStreaming,
  isStopping,
}) => {
  if (!showProjectSelector || availableProjects.length === 0) {
    return null;
  }

  return (
    <select
      value={selectedProjectId || ''}
      onChange={(event) => onProjectChange?.(event.target.value || null)}
      disabled={disabled || isStreaming || isStopping}
      className={cn(
        'flex-shrink-0 px-2 py-1 text-xs rounded-md border bg-background',
        'text-foreground focus:outline-none focus:ring-1 focus:ring-primary',
        (disabled || isStreaming || isStopping) && 'opacity-50 cursor-not-allowed'
      )}
      title="发送时透传 project_root"
    >
      <option value="">请选择项目</option>
      {availableProjects.map((project) => (
        <option key={project.id} value={project.id}>
          {project.name}
        </option>
      ))}
    </select>
  );
};

interface InputAreaWorkspacePickerProps {
  showWorkspaceRootPicker: boolean;
  workspacePickerRef: React.RefObject<HTMLDivElement>;
  disabled: boolean;
  isStreaming: boolean;
  isStopping: boolean;
  onToggleWorkspacePicker: () => void;
  normalizedWorkspaceRoot: string | null;
  workspaceRootDisplayName: string;
  workspacePickerOpen: boolean;
  workspacePath: string | null;
  workspaceParent: string | null;
  workspaceLoading: boolean;
  workspaceEntries: FsEntry[];
  workspaceRoots: FsEntry[];
  onLoadWorkspaceDirectories: (nextPath?: string | null) => void;
  onSelectWorkspaceRoot: (path: string | null) => void;
}

export const InputAreaWorkspacePicker: React.FC<InputAreaWorkspacePickerProps> = ({
  showWorkspaceRootPicker,
  workspacePickerRef,
  disabled,
  isStreaming,
  isStopping,
  onToggleWorkspacePicker,
  normalizedWorkspaceRoot,
  workspaceRootDisplayName,
  workspacePickerOpen,
  workspacePath,
  workspaceParent,
  workspaceLoading,
  workspaceEntries,
  workspaceRoots,
  onLoadWorkspaceDirectories,
  onSelectWorkspaceRoot,
}) => {
  if (!showWorkspaceRootPicker) {
    return null;
  }

  return (
    <div className="relative flex-shrink-0" ref={workspacePickerRef}>
      <button
        type="button"
        onClick={onToggleWorkspacePicker}
        disabled={disabled || isStreaming || isStopping}
        className={cn(
          'px-2 py-1 rounded-md border text-xs transition-colors',
          'text-muted-foreground hover:text-foreground hover:bg-accent',
          (disabled || isStreaming || isStopping) && 'opacity-50 cursor-not-allowed'
        )}
        title={normalizedWorkspaceRoot || '选择工作目录'}
      >
        {`工作目录: ${workspaceRootDisplayName}`}
        <span className="ml-1">▾</span>
      </button>
      {workspacePickerOpen && (
        <div className="absolute left-0 bottom-full mb-2 z-30 w-80 bg-popover text-popover-foreground border rounded-md shadow-lg">
          <div className="px-3 py-2 border-b space-y-2">
            <div className="text-[11px] text-muted-foreground truncate" title={workspacePath || '请选择目录'}>
              当前路径: {workspacePath || '请选择目录'}
            </div>
            <div className="flex items-center gap-2">
              <button
                type="button"
                className="px-2 py-1 rounded border text-[11px] text-muted-foreground hover:text-foreground hover:bg-accent disabled:opacity-50"
                onClick={() => onLoadWorkspaceDirectories(workspaceParent || null)}
                disabled={workspaceLoading || !workspaceParent}
              >
                返回上级
              </button>
              <button
                type="button"
                className="px-2 py-1 rounded border text-[11px] text-muted-foreground hover:text-foreground hover:bg-accent disabled:opacity-50"
                onClick={() => onLoadWorkspaceDirectories(workspacePath || normalizedWorkspaceRoot || null)}
                disabled={workspaceLoading}
              >
                刷新
              </button>
              <button
                type="button"
                className="px-2 py-1 rounded border text-[11px] text-muted-foreground hover:text-foreground hover:bg-accent disabled:opacity-50"
                onClick={() => onSelectWorkspaceRoot(workspacePath)}
                disabled={workspaceLoading || !workspacePath}
              >
                选择当前目录
              </button>
              <button
                type="button"
                className="px-2 py-1 rounded border text-[11px] text-muted-foreground hover:text-foreground hover:bg-accent disabled:opacity-50"
                onClick={() => onSelectWorkspaceRoot(null)}
                disabled={workspaceLoading && !normalizedWorkspaceRoot}
              >
                清空
              </button>
            </div>
          </div>
          <div className="max-h-64 overflow-auto py-1">
            {workspaceLoading ? (
              <div className="px-3 py-2 text-xs text-muted-foreground">加载中...</div>
            ) : (
              (() => {
                const items = workspacePath ? workspaceEntries : workspaceRoots;
                if (!items || items.length === 0) {
                  return <div className="px-3 py-2 text-xs text-muted-foreground">没有可用目录</div>;
                }
                return items.map((entry) => (
                  <button
                    key={entry.path}
                    type="button"
                    className="w-full px-3 py-1.5 text-left text-sm hover:bg-accent flex items-center gap-2"
                    onClick={() => onLoadWorkspaceDirectories(entry.path)}
                  >
                    <svg className="w-4 h-4 text-muted-foreground shrink-0" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2}>
                      <path strokeLinecap="round" strokeLinejoin="round" d="M3 7a2 2 0 012-2h4l2 2h8a2 2 0 012 2v8a2 2 0 01-2 2H5a2 2 0 01-2-2V7z" />
                    </svg>
                    <span className="truncate">{entry.name}</span>
                  </button>
                ));
              })()
            )}
          </div>
        </div>
      )}
    </div>
  );
};

interface InputAreaRemoteConnectionPickerProps {
  availableRemoteConnections: RemoteConnection[];
  currentRemoteConnectionId?: string | null;
  onRemoteConnectionChange?: (connectionId: string | null) => void;
  disabled: boolean;
  isStreaming: boolean;
  isStopping: boolean;
}

export const InputAreaRemoteConnectionPicker: React.FC<InputAreaRemoteConnectionPickerProps> = ({
  availableRemoteConnections,
  currentRemoteConnectionId,
  onRemoteConnectionChange,
  disabled,
  isStreaming,
  isStopping,
}) => {
  if (!Array.isArray(availableRemoteConnections) || availableRemoteConnections.length === 0) {
    return null;
  }

  return (
    <select
      value={currentRemoteConnectionId || ''}
      onChange={(event) => {
        const connectionId = event.target.value || null;
        onRemoteConnectionChange?.(connectionId);
      }}
      disabled={disabled || isStreaming || isStopping}
      className={cn(
        'flex-shrink-0 px-2 py-1 text-xs rounded-md border bg-background',
        'text-foreground focus:outline-none focus:ring-1 focus:ring-primary max-w-[220px]',
        (disabled || isStreaming || isStopping) && 'opacity-50 cursor-not-allowed'
      )}
      title="选择远程服务器（会透传给 AI 工具）"
    >
      <option value="">
        服务器: 不选择
      </option>
      {availableRemoteConnections.map((connection) => (
        <option key={connection.id} value={connection.id}>
          {`服务器: ${connection.name || connection.host}`}
        </option>
      ))}
    </select>
  );
};

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
          (disabled || isStreaming || isStopping) && 'opacity-50 cursor-not-allowed'
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
          (disabled || isStreaming || isStopping) && 'opacity-50 cursor-not-allowed'
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
