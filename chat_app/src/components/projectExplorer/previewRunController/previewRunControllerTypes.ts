import type { Dispatch, SetStateAction } from 'react';

import type {
  TerminalDispatchResponse,
  TerminalLogResponse,
  TerminalResponse,
} from '../../../lib/api/client/types';
import type { ProjectRunTarget, Terminal } from '../../../types';

export interface ProjectPreviewActiveRunState {
  terminalId: string;
  terminalName: string;
  cwd: string;
  command: string;
  dispatchedAt: number;
  origin: 'dispatched' | 'discovered';
}

export interface UseProjectPreviewRunControllerParams {
  projectId: string;
  projectRootPath: string;
  runCwd: string;
  runTargets: ProjectRunTarget[];
  selectedRunTargetId: string | null;
  onRunCommand: (payload: { cwd: string; command: string }) => Promise<TerminalDispatchResponse>;
  onInterruptTerminal: (
    terminalId: string,
    payload?: { reason?: string },
  ) => Promise<TerminalDispatchResponse>;
  onListTerminalLogs: (
    terminalId: string,
    params?: { limit?: number; offset?: number; before?: string },
  ) => Promise<TerminalLogResponse[]>;
  onListTerminals: () => Promise<Array<TerminalResponse | Terminal>>;
}

export type ProjectPreviewRunSetter = Dispatch<SetStateAction<ProjectPreviewActiveRunState | null>>;
export type StringStateSetter = Dispatch<SetStateAction<string | null>>;
