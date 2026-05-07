import * as workspaceApi from '../../workspace';
import type {
  DeleteSuccessResponse,
  TerminalDispatchResponse,
  TerminalLogResponse,
  TerminalResponse,
} from '../../types';
import type ApiClient from '../../../client';

export interface WorkspaceTerminalFacade {
  listTerminals(userId?: string): Promise<TerminalResponse[]>;
  createTerminal(data: { name?: string; cwd: string; user_id?: string }): Promise<TerminalResponse>;
  dispatchTerminalCommand(data: {
    cwd: string;
    command: string;
    user_id?: string;
    project_id?: string;
    create_if_missing?: boolean;
  }): Promise<TerminalDispatchResponse>;
  getTerminal(id: string): Promise<TerminalResponse>;
  interruptTerminal(id: string, data?: { reason?: string }): Promise<TerminalDispatchResponse>;
  deleteTerminal(id: string): Promise<DeleteSuccessResponse>;
  listTerminalLogs(
    terminalId: string,
    params?: { limit?: number; offset?: number; before?: string },
  ): Promise<TerminalLogResponse[]>;
}

export const workspaceTerminalFacade: WorkspaceTerminalFacade & ThisType<ApiClient> = {
  async listTerminals(userId) {
    return workspaceApi.listTerminals(this.getRequestFn(), userId);
  },
  async createTerminal(data) {
    return workspaceApi.createTerminal(this.getRequestFn(), data);
  },
  async dispatchTerminalCommand(data) {
    return workspaceApi.dispatchTerminalCommand(this.getRequestFn(), data);
  },
  async getTerminal(id) {
    return workspaceApi.getTerminal(this.getRequestFn(), id);
  },
  async interruptTerminal(id, data) {
    return workspaceApi.interruptTerminal(this.getRequestFn(), id, data);
  },
  async deleteTerminal(id) {
    return workspaceApi.deleteTerminal(this.getRequestFn(), id);
  },
  async listTerminalLogs(terminalId, params) {
    return workspaceApi.listTerminalLogs(this.getRequestFn(), terminalId, params);
  },
};
