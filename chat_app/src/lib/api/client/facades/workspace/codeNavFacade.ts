import * as workspaceApi from '../../workspace';
import type {
  CodeNavCapabilitiesResponse,
  CodeNavDocumentSymbolsResponse,
  CodeNavLocationsResponse,
} from '../../types';
import type ApiClient from '../../../client';

export interface WorkspaceCodeNavFacade {
  getCodeNavCapabilities(projectRoot: string, filePath: string): Promise<CodeNavCapabilitiesResponse>;
  getCodeNavDefinition(data: {
    projectRoot: string;
    filePath: string;
    line: number;
    column: number;
  }): Promise<CodeNavLocationsResponse>;
  getCodeNavReferences(data: {
    projectRoot: string;
    filePath: string;
    line: number;
    column: number;
  }): Promise<CodeNavLocationsResponse>;
  getCodeNavDocumentSymbols(projectRoot: string, filePath: string): Promise<CodeNavDocumentSymbolsResponse>;
}

export const workspaceCodeNavFacade: WorkspaceCodeNavFacade & ThisType<ApiClient> = {
  async getCodeNavCapabilities(projectRoot, filePath) {
    return workspaceApi.getCodeNavCapabilities(this.getRequestFn(), projectRoot, filePath);
  },
  async getCodeNavDefinition(data) {
    return workspaceApi.getCodeNavDefinition(this.getRequestFn(), data);
  },
  async getCodeNavReferences(data) {
    return workspaceApi.getCodeNavReferences(this.getRequestFn(), data);
  },
  async getCodeNavDocumentSymbols(projectRoot, filePath) {
    return workspaceApi.getCodeNavDocumentSymbols(this.getRequestFn(), projectRoot, filePath);
  },
};
