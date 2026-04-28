import type { Project } from '../../types';
import type { useProjectExplorerCodeNav } from './useProjectExplorerCodeNav';
import type { useProjectExplorerDataLoading } from './useProjectExplorerDataLoading';
import type { useProjectExplorerLogs } from './useProjectExplorerLogs';
import type { useProjectExplorerPathHelpers } from './useProjectExplorerPathHelpers';
import type { useProjectExplorerRunState } from './useProjectExplorerRunState';
import type { useProjectExplorerSearch } from './useProjectExplorerSearch';
import type { useProjectExplorerSelection } from './useProjectExplorerSelection';
import type { useProjectExplorerSessionBridge } from './useProjectExplorerSessionBridge';
import type { useProjectExplorerState } from './useProjectExplorerState';
import type { useProjectExplorerTreeStateOps } from './useProjectExplorerTreeStateOps';

export interface UseProjectExplorerWorkspaceModelParams {
  project: Project | null;
  client: ReturnType<typeof useProjectExplorerSessionBridge>['client'];
  state: ReturnType<typeof useProjectExplorerState>;
  pathHelpers: ReturnType<typeof useProjectExplorerPathHelpers>;
  search: ReturnType<typeof useProjectExplorerSearch>;
  dataLoading: ReturnType<typeof useProjectExplorerDataLoading>;
  logs: ReturnType<typeof useProjectExplorerLogs>;
  selection: ReturnType<typeof useProjectExplorerSelection>;
  runState: ReturnType<typeof useProjectExplorerRunState>;
  codeNav: ReturnType<typeof useProjectExplorerCodeNav>;
  treeStateOps: ReturnType<typeof useProjectExplorerTreeStateOps>;
  handleGenerateRunnerScriptForContact: ReturnType<
    typeof useProjectExplorerSessionBridge
  >['handleGenerateRunnerScriptForContact'];
}
