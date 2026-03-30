import React from 'react';
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
    error?: string | null;
  }>;
  runningProjectId?: string | null;
  onToggle: () => void;
  onCreate: () => void;
  onSelect: (projectId: string) => void;
  onRunProject?: (project: Project) => void;
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
  onToggle,
  onCreate,
  onSelect,
  onRunProject,
  onArchive,
  onToggleActionMenu,
  closeActionMenus,
}) => {
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
                  const status = String(runState?.status || 'analyzing');
                  const isAnalyzing = Boolean(runState?.loading) || status === 'analyzing';
                  const isReady = status === 'ready' && targetCount > 0;
                  const isRunning = runningProjectId === project.id;
                  const runDisabled = isRunning || !isReady;
                  const runTitle = isRunning
                    ? '运行中...'
                    : isReady
                      ? `运行默认目标（${targetCount}）`
                      : (runState?.error || (isAnalyzing ? '正在分析启动目标' : '未检测到可运行目标'));

                  return (
                    <div
                      key={project.id}
                      className={cn(
                        'group relative flex items-center p-2 rounded-lg cursor-pointer transition-colors',
                        currentProjectId === project.id
                          ? 'bg-accent border border-border'
                          : 'hover:bg-accent/50',
                      )}
                      onClick={() => onSelect(project.id)}
                    >
                      <div className="flex-1 min-w-0">
                        <h3 className="text-sm font-medium text-foreground truncate">
                          {project.name}
                        </h3>
                        <div className="mt-1 text-xs text-muted-foreground truncate" title={project.rootPath}>
                          {project.rootPath}
                        </div>
                      </div>
                      <button
                        type="button"
                        title={runTitle}
                        onClick={(event) => {
                          event.stopPropagation();
                          if (!runDisabled) {
                            onRunProject?.(project);
                          }
                        }}
                        disabled={runDisabled}
                        className={cn(
                          'mr-1 h-7 w-7 rounded-full border text-xs transition-colors disabled:cursor-not-allowed',
                          isReady
                            ? 'border-emerald-500/60 text-emerald-600 hover:bg-emerald-500/10'
                            : 'border-border text-muted-foreground',
                          isAnalyzing && 'animate-pulse',
                          runDisabled && 'opacity-60'
                        )}
                      >
                        ▶
                      </button>
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
