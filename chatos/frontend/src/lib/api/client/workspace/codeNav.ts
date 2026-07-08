// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type {
  CodeNavCapabilitiesResponse,
  CodeNavDocumentSymbolsResponse,
  CodeNavLocationsResponse,
} from '../types';
import type { ApiRequestFn } from './common';

export const getCodeNavCapabilities = (
  request: ApiRequestFn,
  projectRoot: string,
  filePath: string,
): Promise<CodeNavCapabilitiesResponse> => {
  return request<CodeNavCapabilitiesResponse>('/code-nav/capabilities', {
    method: 'POST',
    body: JSON.stringify({
      project_root: projectRoot,
      file_path: filePath,
    }),
  });
};

export const getCodeNavDefinition = (
  request: ApiRequestFn,
  data: { projectRoot: string; filePath: string; line: number; column: number },
): Promise<CodeNavLocationsResponse> => {
  return request<CodeNavLocationsResponse>('/code-nav/definition', {
    method: 'POST',
    body: JSON.stringify({
      project_root: data.projectRoot,
      file_path: data.filePath,
      line: data.line,
      column: data.column,
    }),
  });
};

export const getCodeNavReferences = (
  request: ApiRequestFn,
  data: { projectRoot: string; filePath: string; line: number; column: number },
): Promise<CodeNavLocationsResponse> => {
  return request<CodeNavLocationsResponse>('/code-nav/references', {
    method: 'POST',
    body: JSON.stringify({
      project_root: data.projectRoot,
      file_path: data.filePath,
      line: data.line,
      column: data.column,
    }),
  });
};

export const getCodeNavDocumentSymbols = (
  request: ApiRequestFn,
  projectRoot: string,
  filePath: string,
): Promise<CodeNavDocumentSymbolsResponse> => {
  return request<CodeNavDocumentSymbolsResponse>('/code-nav/document-symbols', {
    method: 'POST',
    body: JSON.stringify({
      project_root: projectRoot,
      file_path: filePath,
    }),
  });
};
