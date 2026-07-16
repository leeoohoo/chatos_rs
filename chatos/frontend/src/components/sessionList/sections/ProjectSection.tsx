// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';
import { Cloud, FolderOpen } from 'lucide-react';
import { useI18n } from '../../../i18n/I18nProvider';
import { isCloudProject } from '../../../lib/domain/projectExecution';
import { cn } from '../../../lib/utils';
import type { Project } from '../../../types';
import { DotsVerticalIcon, PlusIcon, TrashIcon } from '../../ui/icons';

interface ProjectSectionProps {
  expanded: boolean;
  projects: Project[];
  currentProjectId?: string | null;
  canCreate: boolean;
  onToggle: () => void;
  onCreate: () => void;
  onSelect: (projectId: string) => void;
  onArchive: (projectId: string) => void;
  onToggleActionMenu: (event: React.MouseEvent<HTMLButtonElement>) => void;
  closeActionMenus: () => void;
}

export const ProjectSection: React.FC<ProjectSectionProps> = ({
  expanded,
  projects,
  currentProjectId,
  canCreate,
  onToggle,
  onCreate,
  onSelect,
  onArchive,
  onToggleActionMenu,
  closeActionMenus,
}) => {
  const { t } = useI18n();

  return (
    <div className={cn('flex flex-col min-h-0', expanded ? 'flex-1' : 'shrink-0')}>
      <div className="px-3 py-2 text-xs text-muted-foreground flex items-center justify-between">
        <button
          type="button"
          onClick={onToggle}
          className="flex items-center gap-2 uppercase tracking-wide"
        >
          <span>{expanded ? '▾' : '▸'}</span>
          <span>{t('session.projects')}</span>
        </button>
        {canCreate ? (
          <button
            type="button"
            onClick={onCreate}
            className="p-1 text-muted-foreground hover:text-foreground hover:bg-accent rounded"
            title={t('session.addProject')}
          >
            <PlusIcon className="w-4 h-4" />
          </button>
        ) : null}
      </div>

      {expanded && (
        <div className="flex-1 min-h-0 overflow-y-auto">
          {projects.length === 0 ? (
            <div className="px-3 py-3 text-xs text-muted-foreground">
              {t('session.noProjects')}
            </div>
          ) : (
            <div className="p-2 space-y-1">
              {projects.map((project) => {
                const cloudProject = isCloudProject(project);
                const projectTypeLabel = cloudProject
                  ? t('session.cloudProject')
                  : t('session.localProject');
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
                      onSelect(project.id);
                    }}
                  >
                    <div className="flex flex-1 min-w-0 items-center gap-2">
                      <span
                        className={cn(
                          'flex h-5 w-5 shrink-0 items-center justify-center',
                          cloudProject ? 'text-sky-600' : 'text-emerald-600',
                        )}
                        title={projectTypeLabel}
                        aria-label={projectTypeLabel}
                      >
                        {cloudProject ? (
                          <Cloud className="h-4 w-4" aria-hidden="true" />
                        ) : (
                          <FolderOpen className="h-4 w-4" aria-hidden="true" />
                        )}
                      </span>
                      <h3 className="text-sm font-medium text-foreground truncate">
                        {project.name}
                      </h3>
                    </div>
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
                            {t('session.archiveProject')}
                          </button>
                        </div>
                      </div>
                    </div>
                  </div>
                );
              })}
            </div>
          )}
        </div>
      )}
    </div>
  );
};
