// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import * as workspaceApi from '../../workspace';
import type {
  CodeNavCapabilitiesResponse,
  CodeNavDocumentSymbolsResponse,
  CodeNavLocationsResponse,
} from '../../types';
import type ApiClient from '../../../client';
import { parseLocalConnectorProjectRoot } from '../../../localRuntime';

const localCodeNavUnavailable = {
  provider: 'local_runtime',
  language: 'unknown',
};

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
    if (parseLocalConnectorProjectRoot(projectRoot)) {
      return {
        ...localCodeNavUnavailable,
        supports_definition: false,
        supports_references: false,
        supports_document_symbols: false,
        fallback_available: false,
      };
    }
    return workspaceApi.getCodeNavCapabilities(this.getRequestFn(), projectRoot, filePath);
  },
  async getCodeNavDefinition(data) {
    if (parseLocalConnectorProjectRoot(data.projectRoot)) {
      return { ...localCodeNavUnavailable, mode: 'unavailable', token: null, locations: [] };
    }
    return workspaceApi.getCodeNavDefinition(this.getRequestFn(), data);
  },
  async getCodeNavReferences(data) {
    if (parseLocalConnectorProjectRoot(data.projectRoot)) {
      return { ...localCodeNavUnavailable, mode: 'unavailable', token: null, locations: [] };
    }
    return workspaceApi.getCodeNavReferences(this.getRequestFn(), data);
  },
  async getCodeNavDocumentSymbols(projectRoot, filePath) {
    if (parseLocalConnectorProjectRoot(projectRoot)) {
      return { ...localCodeNavUnavailable, mode: 'unavailable', symbols: [] };
    }
    return workspaceApi.getCodeNavDocumentSymbols(this.getRequestFn(), projectRoot, filePath);
  },
};
