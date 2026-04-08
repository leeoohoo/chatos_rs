import { useEffect } from 'react';
import type React from 'react';
import type { WorkspaceTab } from './WorkspaceTabs';

interface Params {
  projectId?: string;
  projectRootPath?: string;
  expandedReady: boolean;
  expandedPaths: Set<string>;
  showOnlyChanged: boolean;
  setShowOnlyChanged: React.Dispatch<React.SetStateAction<boolean>>;
  workspaceTab: WorkspaceTab;
  setWorkspaceTab: React.Dispatch<React.SetStateAction<WorkspaceTab>>;
  contextMenu: any;
  setContextMenu: React.Dispatch<React.SetStateAction<any>>;
  isResizing: boolean;
  resizeStartX: React.MutableRefObject<number>;
  resizeStartWidth: React.MutableRefObject<number>;
  setTreeWidth: React.Dispatch<React.SetStateAction<number>>;
  treeWidth: number;
  setIsResizing: React.Dispatch<React.SetStateAction<boolean>>;
}

export const useProjectExplorerUiPersistence = ({
  projectId,
  projectRootPath,
  expandedReady,
  expandedPaths,
  showOnlyChanged,
  setShowOnlyChanged,
  workspaceTab,
  setWorkspaceTab,
  contextMenu,
  setContextMenu,
  isResizing,
  resizeStartX,
  resizeStartWidth,
  setTreeWidth,
  treeWidth,
  setIsResizing,
}: Params) => {
  useEffect(() => {
    if (!expandedReady || !projectId || !projectRootPath) return;
    const next = Array.from(expandedPaths);
    localStorage.setItem(`project_explorer_expanded_${projectId}`, JSON.stringify(next));
  }, [expandedPaths, expandedReady, projectId, projectRootPath]);

  useEffect(() => {
    if (!projectId) {
      setShowOnlyChanged(false);
      return;
    }
    if (typeof window === 'undefined') return;
    const saved = window.localStorage.getItem(`project_explorer_only_changed_${projectId}`);
    setShowOnlyChanged(saved === '1');
  }, [projectId, setShowOnlyChanged]);

  useEffect(() => {
    if (!projectId || typeof window === 'undefined') return;
    window.localStorage.setItem(
      `project_explorer_only_changed_${projectId}`,
      showOnlyChanged ? '1' : '0',
    );
  }, [projectId, showOnlyChanged]);

  useEffect(() => {
    if (!projectId || typeof window === 'undefined') {
      setWorkspaceTab('files');
      return;
    }
    const saved = window.localStorage.getItem(`project_workspace_tab_${projectId}`);
    if (saved === 'team') {
      setWorkspaceTab('team');
      return;
    }
    setWorkspaceTab('files');
  }, [projectId, setWorkspaceTab]);

  useEffect(() => {
    if (!projectId || typeof window === 'undefined') return;
    window.localStorage.setItem(`project_workspace_tab_${projectId}`, workspaceTab);
  }, [projectId, workspaceTab]);

  useEffect(() => {
    if (!contextMenu) return undefined;
    const closeMenu = () => setContextMenu(null);
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        closeMenu();
      }
    };

    window.addEventListener('click', closeMenu);
    window.addEventListener('resize', closeMenu);
    window.addEventListener('scroll', closeMenu, true);
    window.addEventListener('keydown', onKeyDown);
    return () => {
      window.removeEventListener('click', closeMenu);
      window.removeEventListener('resize', closeMenu);
      window.removeEventListener('scroll', closeMenu, true);
      window.removeEventListener('keydown', onKeyDown);
    };
  }, [contextMenu, setContextMenu]);

  useEffect(() => {
    if (!isResizing) return;
    const handleMove = (event: MouseEvent) => {
      const delta = event.clientX - resizeStartX.current;
      const next = Math.min(Math.max(resizeStartWidth.current + delta, 200), 640);
      setTreeWidth(next);
    };
    const handleUp = () => {
      setIsResizing(false);
    };
    window.addEventListener('mousemove', handleMove);
    window.addEventListener('mouseup', handleUp);
    document.body.style.cursor = 'col-resize';
    document.body.style.userSelect = 'none';
    return () => {
      window.removeEventListener('mousemove', handleMove);
      window.removeEventListener('mouseup', handleUp);
      document.body.style.cursor = '';
      document.body.style.userSelect = '';
    };
  }, [isResizing, resizeStartWidth, resizeStartX, setIsResizing, setTreeWidth]);

  useEffect(() => {
    localStorage.setItem('project_explorer_tree_width', String(treeWidth));
  }, [treeWidth]);
};
