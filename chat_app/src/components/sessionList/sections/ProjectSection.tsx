// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';
import { useI18n } from '../../../i18n/I18nProvider';
import { getUserVisiblePath } from '../../../lib/domain/filesystem';
import { cn } from '../../../lib/utils';
import type { Project } from '../../../types';
import { DotsVerticalIcon, PlusIcon, TrashIcon } from '../../ui/icons';

interface ProjectSectionProps {
  expanded: boolean;
  projects: Project[];
  currentProjectId?: string | null;
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
        <button
          type="button"
          onClick={onCreate}
          className="p-1 text-muted-foreground hover:text-foreground hover:bg-accent rounded"
          title={t('session.addProject')}
        >
          <PlusIcon className="w-4 h-4" />
        </button>
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
                const visiblePath = project.displayRootPath || getUserVisiblePath(project.rootPath);
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
                    <div className="flex-1 min-w-0">
                      <h3 className="text-sm font-medium text-foreground truncate">
                        {project.name}
                      </h3>
                      <div className="mt-1 text-xs text-muted-foreground truncate" title={visiblePath}>
                        {visiblePath}
                      </div>
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
