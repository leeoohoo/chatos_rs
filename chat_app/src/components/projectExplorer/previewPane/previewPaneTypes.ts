// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type {
  CodeNavCapabilities,
  CodeNavDocumentSymbolsResult,
  CodeNavLocation,
  CodeNavLocationsResult,
  FsEntry,
  FsReadResult,
  ProjectSearchHit,
} from '../../../types';

export interface PreviewTokenSelection {
  token: string;
  line: number;
  column: number;
}

export interface ProjectPreviewPaneProps {
  selectedFile: FsReadResult | null;
  selectedPath: string | null;
  selectedEntry: FsEntry | null;
  loadingFile: boolean;
  error: string | null;
  saveError: string | null;
  savingFile: boolean;
  searchQuery: string;
  searchCaseSensitive: boolean;
  searchWholeWord: boolean;
  searchResults: ProjectSearchHit[];
  activeSearchHitId: string | null;
  activeSearchHitIndex: number;
  totalSearchHits: number;
  canOpenPreviousSearchHit: boolean;
  canOpenNextSearchHit: boolean;
  targetLine: number | null;
  targetLineRevision: number;
  navCapabilities: CodeNavCapabilities | null;
  navCapabilitiesError: string | null;
  selectedToken: string | null;
  navResult: CodeNavLocationsResult | null;
  navRequestKind: 'definition' | 'references' | null;
  navLoading: boolean;
  navError: string | null;
  activeNavLocationId: string | null;
  canGoBackFromNav: boolean;
  documentSymbols: CodeNavDocumentSymbolsResult | null;
  documentSymbolsLoading: boolean;
  documentSymbolsError: string | null;
  onRequestDocumentSymbols: () => void;
  onTokenSelection: (selection: PreviewTokenSelection | null) => void;
  onClearTokenSelection: () => void;
  onRequestDefinition: () => void;
  onRequestReferences: () => void;
  onGoBackFromNav: () => void;
  onSearchInProject: (query: string) => void;
  onOpenPreviousSearchHit: () => void;
  onOpenNextSearchHit: () => void;
  onActivateSearchHit: (hit: ProjectSearchHit) => void;
  onOpenNavLocation: (location: CodeNavLocation) => void;
  onOpenDocumentSymbol: (line: number) => void;
  onSaveFile: (path: string, content: string) => Promise<boolean>;
}
