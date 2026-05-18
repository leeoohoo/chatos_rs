import type React from 'react';

import type {
  CodeNavCapabilities,
  FsEntry,
  FsReadResult,
  ProjectSearchHit,
} from '../../../types';
import { ProjectPreviewPane } from '../PreviewPane';

export interface UseProjectExplorerPreviewPanePropsParams {
  selectedFile: FsReadResult | null;
  selectedPath: string | null;
  selectedEntry: FsEntry | null;
  loadingFile: boolean;
  error: string | null;
  searchQuery: string;
  searchCaseSensitive: boolean;
  searchWholeWord: boolean;
  activeSearchHitIndex: number;
  totalSearchHits: number;
  canOpenPreviousSearchHit: boolean;
  canOpenNextSearchHit: boolean;
  previewTargetLine: number | null;
  previewTargetLineRevision: number;
  navCapabilities: CodeNavCapabilities | null;
  navCapabilitiesError: string | null;
  searchResults: ProjectSearchHit[];
  activeSearchHitId: string | null;
  selectedToken: string | null;
  navResult: React.ComponentProps<typeof ProjectPreviewPane>['navResult'];
  navRequestKind: 'definition' | 'references' | null;
  navLoading: boolean;
  navError: string | null;
  activeNavLocationId: string | null;
  canGoBackFromNav: boolean;
  documentSymbols: React.ComponentProps<typeof ProjectPreviewPane>['documentSymbols'];
  documentSymbolsLoading: boolean;
  documentSymbolsError: string | null;
  handleTokenSelection: React.ComponentProps<typeof ProjectPreviewPane>['onTokenSelection'];
  clearTokenSelection: () => void;
  requestDefinition: () => Promise<void>;
  requestReferences: () => Promise<void>;
  goBackFromNav: () => Promise<void>;
  handleSearchInProject: (query: string) => void;
  handleOpenPreviousSearchHit: () => Promise<void>;
  handleOpenNextSearchHit: () => Promise<void>;
  handleActivateSearchHit: (hit: ProjectSearchHit) => void;
  handleOpenNavLocation: React.ComponentProps<typeof ProjectPreviewPane>['onOpenNavLocation'];
  handleOpenDocumentSymbol: (line: number) => void;
}
