import type ApiClient from '../../client';

import {
  workspaceCodeNavFacade,
  type WorkspaceCodeNavFacade,
} from './workspace/codeNavFacade';
import {
  workspaceContactFacade,
  type WorkspaceContactFacade,
} from './workspace/contactsFacade';
import {
  workspaceFilesystemFacade,
  type WorkspaceFilesystemFacade,
} from './workspace/filesystemFacade';
import {
  workspaceGitFacade,
  type WorkspaceGitFacade,
} from './workspace/gitFacade';
import {
  workspaceProjectFacade,
  type WorkspaceProjectFacade,
} from './workspace/projectsFacade';
import {
  workspaceRemoteConnectionFacade,
  type WorkspaceRemoteConnectionFacade,
} from './workspace/remoteConnectionsFacade';
import {
  workspaceSessionFacade,
  type WorkspaceSessionFacade,
} from './workspace/sessionsFacade';
import {
  workspaceTerminalFacade,
  type WorkspaceTerminalFacade,
} from './workspace/terminalsFacade';

export interface WorkspaceFacade
  extends WorkspaceSessionFacade,
    WorkspaceContactFacade,
    WorkspaceProjectFacade,
    WorkspaceTerminalFacade,
    WorkspaceRemoteConnectionFacade,
    WorkspaceFilesystemFacade,
    WorkspaceCodeNavFacade,
    WorkspaceGitFacade {}

export const workspaceFacade = Object.assign(
  {},
  workspaceSessionFacade,
  workspaceContactFacade,
  workspaceProjectFacade,
  workspaceTerminalFacade,
  workspaceRemoteConnectionFacade,
  workspaceFilesystemFacade,
  workspaceCodeNavFacade,
  workspaceGitFacade,
) as WorkspaceFacade & ThisType<ApiClient>;
