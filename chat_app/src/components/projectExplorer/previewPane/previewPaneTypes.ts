import type {
  ChangeLogItem,
  CodeNavCapabilities,
  CodeNavDocumentSymbolsResult,
  CodeNavLocation,
  CodeNavLocationsResult,
  FsEntry,
  FsReadResult,
  ProjectSearchHit,
} from '../../../types';
import type { ProjectRunnerMember } from '../useProjectExplorerRunState';

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
  selectedLog: ChangeLogItem | null;
  runStatus: string;
  runCatalogLoading: boolean;
  projectMembers: ProjectRunnerMember[];
  projectMembersLoading: boolean;
  runnerScriptExists: boolean;
  runnerScriptChecking: boolean;
  runnerScriptPath: string;
  runnerStartCommand: string;
  runnerStopCommand: string;
  runnerRestartCommand: string;
  starting: boolean;
  stopping: boolean;
  restarting: boolean;
  runnerMessage: string | null;
  runnerError: string | null;
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
  onRunnerStart: () => void;
  onRunnerStop: () => void;
  onRunnerRestart: () => void;
  onRefreshRunnerState: () => void;
  onGenerateRunnerScriptForContact: (member: ProjectRunnerMember) => Promise<void>;
}
