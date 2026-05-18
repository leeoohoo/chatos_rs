import React from 'react';

import { useI18n } from '../i18n/I18nProvider';
import type { Project } from '../types';
import { cn } from '../lib/utils';
import { ProjectExplorerFilesWorkspace } from './projectExplorer/ProjectExplorerFilesWorkspace';
import ProjectRunSettingsPanel from './projectExplorer/ProjectRunSettingsPanel';
import TeamMembersPane from './projectExplorer/TeamMembersPane';
import WorkspaceTabs from './projectExplorer/WorkspaceTabs';
import GitBranchButton from './projectExplorer/git/GitBranchButton';
import { useProjectExplorerViewModel } from './projectExplorer/useProjectExplorerViewModel';

interface ProjectExplorerProps {
  project: Project | null;
  className?: string;
}

export const ProjectExplorer: React.FC<ProjectExplorerProps> = ({ project, className }) => {
  const { t } = useI18n();
  const {
    client,
    containerRef,
    workspaceTab,
    setWorkspaceTab,
    treeWidth,
    setIsResizing,
    resizeStartX,
    resizeStartWidth,
    isResizing,
    treePaneProps,
    previewPaneProps,
    projectSettingsProps,
    actionLoading,
    moveConflict,
    setMoveConflict,
    handleMoveConflictCancel,
    handleMoveConflictOverwrite,
    handleMoveConflictRename,
    contextMenu,
    contextMenuStyle,
    isContextRootEntry,
    setContextMenu,
    workspaceHandleCreateDirectory,
    workspaceHandleCreateFile,
    workspaceHandleDownloadSelected,
    workspaceHandleDeleteSelected,
    handleGitRepositoryChanged,
  } = useProjectExplorerViewModel({ project });

  if (!project) {
    return (
      <div className={cn('flex items-center justify-center h-full text-muted-foreground', className)}>
        {t('projectExplorer.emptyProject')}
      </div>
    );
  }

  return (
    <div ref={containerRef} className={cn('flex h-full flex-col overflow-hidden', className)}>
      <WorkspaceTabs
        activeTab={workspaceTab}
        onChange={setWorkspaceTab}
        rightActions={(
          <GitBranchButton
            client={client}
            projectId={project.id}
            projectRoot={project.rootPath}
            onRepositoryChanged={handleGitRepositoryChanged}
          />
        )}
      />

      <div className="flex-1 min-h-0 overflow-hidden">
        {workspaceTab === 'team' ? (
          <TeamMembersPane
            project={project}
            className="h-full"
          />
        ) : workspaceTab === 'settings' ? (
          <div className="h-full overflow-auto p-4">
            <ProjectRunSettingsPanel {...projectSettingsProps} />
          </div>
        ) : (
          <ProjectExplorerFilesWorkspace
            treePaneProps={treePaneProps}
            treeWidth={treeWidth}
            isResizing={isResizing}
            resizeStartX={resizeStartX}
            resizeStartWidth={resizeStartWidth}
            setIsResizing={setIsResizing}
            previewPaneProps={previewPaneProps}
            moveConflict={moveConflict}
            actionLoading={actionLoading}
            setMoveConflict={setMoveConflict}
            onMoveConflictCancel={handleMoveConflictCancel}
            onMoveConflictOverwrite={handleMoveConflictOverwrite}
            onMoveConflictRename={handleMoveConflictRename}
            contextMenu={contextMenu}
            contextMenuStyle={contextMenuStyle}
            isContextRootEntry={isContextRootEntry}
            setContextMenu={setContextMenu}
            onCreateDirectory={workspaceHandleCreateDirectory}
            onCreateFile={workspaceHandleCreateFile}
            onDownloadSelected={workspaceHandleDownloadSelected}
            onDeleteSelected={workspaceHandleDeleteSelected}
          />
        )}
      </div>
    </div>
  );
};

export default ProjectExplorer;
