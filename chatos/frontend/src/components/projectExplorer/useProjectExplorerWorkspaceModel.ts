// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useProjectExplorerEffects } from './useProjectExplorerEffects';
import { useProjectExplorerPreviewNavigation } from './useProjectExplorerPreviewNavigation';
import { useProjectExplorerWorkspaceActions } from './useProjectExplorerWorkspaceActions';
import { useProjectExplorerWorkspaceViewModel } from './useProjectExplorerWorkspaceViewModel';
import {
  buildWorkspaceActionsParams,
  buildWorkspaceEffectsParams,
  buildWorkspaceViewModelParams,
} from './workspaceModelBuilders';
import type { UseProjectExplorerWorkspaceModelParams } from './workspaceModelTypes';

export const useProjectExplorerWorkspaceModel = ({
  project,
  filesTabActive,
  client,
  state,
  pathHelpers,
  search,
  dataLoading,
  selection,
  runState,
  codeNav,
  treeStateOps,
}: UseProjectExplorerWorkspaceModelParams) => {
  const modelParams = {
    project,
    filesTabActive,
    client,
    state,
    pathHelpers,
    search,
    dataLoading,
    selection,
    runState,
    codeNav,
    treeStateOps,
  };
  const {
    handlePreviewTokenSelection,
    handleOpenDocumentSymbol,
  } = useProjectExplorerPreviewNavigation({
    handleTokenSelection: codeNav.handleTokenSelection,
    setPreviewTargetLine: search.setPreviewTargetLine,
  });

  const actions = useProjectExplorerWorkspaceActions(
    buildWorkspaceActionsParams(modelParams),
  );

  useProjectExplorerEffects(buildWorkspaceEffectsParams(modelParams, actions));

  const workspaceShell = useProjectExplorerWorkspaceViewModel(buildWorkspaceViewModelParams(modelParams, actions, {
    handlePreviewTokenSelection,
    handleOpenDocumentSymbol,
  }));

  return {
    ...workspaceShell,
    handleMoveConflictCancel: actions.handleMoveConflictCancel,
    handleMoveConflictOverwrite: actions.handleMoveConflictOverwrite,
    handleMoveConflictRename: actions.handleMoveConflictRename,
  };
};
