import React from 'react';

import type { Project } from '../types';
import { cn } from '../lib/utils';
import { ProjectExplorerFilesWorkspace } from './projectExplorer/ProjectExplorerFilesWorkspace';
import TeamMembersPane from './projectExplorer/TeamMembersPane';
import WorkspaceTabs from './projectExplorer/WorkspaceTabs';
import GitBranchButton from './projectExplorer/git/GitBranchButton';
import { useProjectExplorerViewModel } from './projectExplorer/useProjectExplorerViewModel';

interface ProjectExplorerProps {
  project: Project | null;
  className?: string;
}

export const ProjectExplorer: React.FC<ProjectExplorerProps> = ({ project, className }) => {
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
    actionLoading,
    loadingLogs,
    logsError,
    changeLogs,
    selectedLogId,
    setSelectedLogId,
    moveConflict,
    setMoveConflict,
    handleMoveConflictCancel,
    handleMoveConflictOverwrite,
    handleMoveConflictRename,
    contextMenu,
    contextMenuStyle,
    isContextRootEntry,
    setContextMenu,
    workspaceCanRunFile,
    workspaceHandleCreateDirectory,
    workspaceHandleCreateFile,
    workspaceHandleRunFile,
    workspaceHandleDownloadSelected,
    workspaceHandleDeleteSelected,
    handleGitRepositoryChanged,
  } = useProjectExplorerViewModel({ project });

  if (!project) {
    return (
      <div className={cn('flex items-center justify-center h-full text-muted-foreground', className)}>
        请选择一个项目查看文件
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
        ) : (
          <ProjectExplorerFilesWorkspace
            treePaneProps={treePaneProps}
            treeWidth={treeWidth}
            isResizing={isResizing}
            resizeStartX={resizeStartX}
            resizeStartWidth={resizeStartWidth}
            setIsResizing={setIsResizing}
            previewPaneProps={previewPaneProps}
            loadingLogs={loadingLogs}
            logsError={logsError}
            changeLogs={changeLogs}
            selectedLogId={selectedLogId}
            setSelectedLogId={setSelectedLogId}
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
            canRunFile={workspaceCanRunFile}
            onCreateDirectory={workspaceHandleCreateDirectory}
            onCreateFile={workspaceHandleCreateFile}
            onRunFile={workspaceHandleRunFile}
            onDownloadSelected={workspaceHandleDownloadSelected}
            onDeleteSelected={workspaceHandleDeleteSelected}
          />
        )}
      </div>
    </div>
  );
};

export default ProjectExplorer;
