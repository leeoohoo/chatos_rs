// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type {
  CodeNavCapabilitiesResponse,
  CodeNavDocumentSymbolsResponse,
  CodeNavLocationsResponse,
} from '../../../lib/api/client/types';
import type {
  CodeNavCapabilities,
  CodeNavDocumentSymbolsResult,
  CodeNavLocation,
  CodeNavLocationsResult,
} from '../../../types';

export interface ProjectExplorerCodeNavApiClient {
  getCodeNavCapabilities: (projectRoot: string, filePath: string) => Promise<CodeNavCapabilitiesResponse>;
  getCodeNavDefinition: (data: {
    projectRoot: string;
    filePath: string;
    line: number;
    column: number;
  }) => Promise<CodeNavLocationsResponse>;
  getCodeNavReferences: (data: {
    projectRoot: string;
    filePath: string;
    line: number;
    column: number;
  }) => Promise<CodeNavLocationsResponse>;
  getCodeNavDocumentSymbols: (projectRoot: string, filePath: string) => Promise<CodeNavDocumentSymbolsResponse>;
}

export interface UseProjectExplorerCodeNavOptions {
  client: ProjectExplorerCodeNavApiClient;
  projectRootPath?: string | null;
  selectedFilePath?: string | null;
  targetLine: number | null;
  openLocation: (
    location: CodeNavLocation,
    options?: {
      preserveHistory?: boolean;
      targetLine?: number | null;
    },
  ) => Promise<void>;
}

export interface TokenSelection {
  token: string;
  line: number;
  column: number;
}

export type NavRequestKind = 'definition' | 'references';

export interface CodeNavHistoryEntry {
  path: string;
  targetLine: number | null;
}

export interface UseProjectExplorerCodeNavResult {
  navCapabilities: CodeNavCapabilities | null;
  navCapabilitiesLoading: boolean;
  navCapabilitiesError: string | null;
  selectedToken: string | null;
  selectedTokenLine: number | null;
  selectedTokenColumn: number | null;
  navResult: CodeNavLocationsResult | null;
  navRequestKind: NavRequestKind | null;
  navLoading: boolean;
  navError: string | null;
  activeNavLocationId: string | null;
  canGoBackFromNav: boolean;
  documentSymbols: CodeNavDocumentSymbolsResult | null;
  documentSymbolsLoading: boolean;
  documentSymbolsError: string | null;
  requestDocumentSymbols: () => Promise<void>;
  handleTokenSelection: (selection: TokenSelection | null) => void;
  clearTokenSelection: () => void;
  requestDefinition: () => Promise<void>;
  requestReferences: () => Promise<void>;
  handleOpenNavLocation: (location: CodeNavLocation) => Promise<void>;
  goBackFromNav: () => Promise<void>;
}
