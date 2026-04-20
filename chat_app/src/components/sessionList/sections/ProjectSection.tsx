import React, { useState } from 'react';
import { cn } from '../../../lib/utils';
import type { Project } from '../../../types';
import { DotsVerticalIcon, PlusIcon, TrashIcon } from '../../ui/icons';

interface ProjectSectionProps {
  expanded: boolean;
  projects: Project[];
  currentProjectId?: string | null;
  projectRunStateById?: Record<string, {
    status: string;
    loading: boolean;
    targetCount: number;
    targets?: Array<{
      id: string;
      label: string;
      cwd?: string | null;
    }>;
    error?: string | null;
  }>;
  runningProjectId?: string | null;
  projectLiveStateById?: Record<string, {
    isRunning: boolean;
    terminalName?: string | null;
    canRestart?: boolean;
    actionLoading?: boolean;
  }>;
  onToggle: () => void;
  onCreate: () => void;
  onSelect: (projectId: string) => void;
  onRunProject?: (project: Project, targetId?: string) => void;
  onStopProject?: (project: Project) => void;
  onRestartProject?: (project: Project) => void;
  onArchive: (projectId: string) => void;
  onToggleActionMenu: (event: React.MouseEvent<HTMLButtonElement>) => void;
  closeActionMenus: () => void;
}

export const ProjectSection: React.FC<ProjectSectionProps> = ({
  expanded,
  projects,
  currentProjectId,
  projectRunStateById,
  runningProjectId,
  projectLiveStateById,
  onToggle,
  onCreate,
  onSelect,
  onRunProject,
  onStopProject,
  onRestartProject,
  onArchive,
  onToggleActionMenu,
  closeActionMenus,
}) => {
  const [chooserProjectId, setChooserProjectId] = useState<string | null>(null);

  return (
    <div className={cn('flex flex-col min-h-0', expanded ? 'flex-1' : 'shrink-0')}>
      <div className="px-3 py-2 text-xs text-muted-foreground flex items-center justify-between">
        <button
          type="button"
          onClick={onToggle}
          className="flex items-center gap-2 uppercase tracking-wide"
        >
          <span>{expanded ? '▾' : '▸'}</span>
          <span>PROJECTS</span>
        </button>
        <button
          type="button"
          onClick={onCreate}
          className="p-1 text-muted-foreground hover:text-foreground hover:bg-accent rounded"
          title="新增项目"
        >
          <PlusIcon className="w-4 h-4" />
        </button>
      </div>

      {expanded && (
        <div className="flex-1 min-h-0 overflow-y-auto">
          {projects.length === 0 ? (
            <div className="px-3 py-3 text-xs text-muted-foreground">
              还没有项目，点击右侧 + 新建。
            </div>
          ) : (
            <div className="p-2 space-y-1">
              {projects.map((project) => (
                (() => {
                  const runState = projectRunStateById?.[project.id];
                  const targetCount = runState?.targetCount ?? 0;
                  const status = String(runState?.status || 'loading');
                  const isAnalyzing = Boolean(runState?.loading) || status === 'loading';
                  const isReady = status === 'ready' && targetCount > 0;
                  const liveState = projectLiveStateById?.[project.id];
                  const isRunning = Boolean(liveState?.isRunning);
                  const actionLoading = Boolean(liveState?.actionLoading);
                  const runDisabled = isRunning || actionLoading || runningProjectId === project.id || !isReady;
                  const runTitle = isRunning
                    ? `运行中：${liveState?.terminalName || '终端'}`
                    : status === 'missing_root'
                      ? (runState?.error || '项目目录不存在，请检查项目路径')
                    : isReady
                      ? '启动项目全部服务'
                      : (runState?.error || (isAnalyzing ? '正在检查脚本状态' : '缺少启动脚本'));

                  return (
                    <div
                      key={project.id}
                      className={cn(
                        'group relative flex items-center p-2 rounded-lg cursor-pointer transition-colors',
                        currentProjectId === project.id
                          ? 'bg-accent border border-border'
                          : 'hover:bg-accent/50',
                      )}
                      onClick={() => {
                        setChooserProjectId(null);
                        onSelect(project.id);
                      }}
                    >
                      <div className="flex-1 min-w-0">
                        <h3 className="text-sm font-medium text-foreground truncate">
                          {project.name}
                        </h3>
                        <div className="mt-1 text-xs text-muted-foreground truncate" title={project.rootPath}>
                          {project.rootPath}
                        </div>
                      </div>
                      {isRunning ? (
                        <div className="mr-1 flex items-center gap-1">
                          <button
                            type="button"
                            title="停止"
                            onClick={(event) => {
                              event.stopPropagation();
                              if (!actionLoading) {
                                onStopProject?.(project);
                              }
                            }}
                            disabled={actionLoading}
                            className="h-7 w-7 rounded border border-rose-500/60 text-rose-600 hover:bg-rose-500/10 disabled:opacity-60 disabled:cursor-not-allowed"
                          >
                            ■
                          </button>
                          <button
                            type="button"
                            title={liveState?.canRestart === false ? '未找到可重启命令' : '重启'}
                            onClick={(event) => {
                              event.stopPropagation();
                              if (!actionLoading && liveState?.canRestart !== false) {
                                onRestartProject?.(project);
                              }
                            }}
                            disabled={actionLoading || liveState?.canRestart === false}
                            className="h-7 w-7 rounded border border-border text-muted-foreground hover:bg-accent disabled:opacity-60 disabled:cursor-not-allowed"
                          >
                            ↻
                          </button>
                        </div>
                      ) : (
                        <div className="relative mr-1">
                          <button
                            type="button"
                            title={runTitle}
                            onClick={(event) => {
                              event.stopPropagation();
                              if (runDisabled) return;
                              const targets = runState?.targets || [];
                              if (targets.length > 1) {
                                setChooserProjectId((prev) => (prev === project.id ? null : project.id));
                                return;
                              }
                              onRunProject?.(project, targets[0]?.id);
                            }}
                            disabled={runDisabled}
                            className={cn(
                              'h-7 w-7 rounded-full border text-xs transition-colors disabled:cursor-not-allowed',
                              isReady
                                ? 'border-emerald-500/60 text-emerald-600 hover:bg-emerald-500/10'
                                : 'border-border text-muted-foreground',
                              isAnalyzing && 'animate-pulse',
                              runDisabled && 'opacity-60'
                            )}
                          >
                            ▶
                          </button>
                          {chooserProjectId === project.id && (runState?.targets?.length || 0) > 1 && (
                            <div
                              className="absolute right-0 top-8 z-20 w-64 rounded-md border border-border bg-popover shadow-lg p-1"
                              onClick={(event) => event.stopPropagation()}
                            >
                              <div className="px-2 py-1 text-[11px] text-muted-foreground">选择启动目标</div>
                              {runState?.targets?.map((target) => (
                                <button
                                  key={target.id}
                                  type="button"
                                  onClick={() => {
                                    setChooserProjectId(null);
                                    onRunProject?.(project, target.id);
                                  }}
                                  className="w-full text-left rounded px-2 py-1.5 hover:bg-accent"
                                >
                                  <div className="text-xs text-foreground truncate">{target.label}</div>
                                  <div className="text-[11px] text-muted-foreground truncate">{target.cwd || '-'}</div>
                                </button>
                              ))}
                            </div>
                          )}
                        </div>
                      )}
                      <div className="relative" data-action-menu-root="true">
                        <button
                          className="p-1 text-muted-foreground hover:text-foreground opacity-0 group-hover:opacity-100 transition-opacity"
                          onClick={onToggleActionMenu}
                        >
                          <DotsVerticalIcon className="w-4 h-4" />
                        </button>
                        <div className="js-inline-action-menu hidden absolute right-0 z-10 mt-1 w-40 bg-popover border border-border rounded-md shadow-lg">
                          <div className="py-1">
                            <button
                              onClick={(e) => {
                                e.stopPropagation();
                                onArchive(project.id);
                                closeActionMenus();
                              }}
                              className="flex items-center w-full px-3 py-2 text-sm text-destructive hover:bg-destructive/10"
                            >
                              <TrashIcon className="w-4 h-4 mr-2" />
                              归档
                            </button>
                          </div>
                        </div>
                      </div>
                    </div>
                  );
                })()
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
};
