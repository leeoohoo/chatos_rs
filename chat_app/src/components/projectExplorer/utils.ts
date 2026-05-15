import type {
  FsContentSearchEntryResponse,
} from '../../lib/api/client/types';
import type {
  FsEntry,
  FsReadResult,
  ProjectSearchHit,
} from '../../types';
import {
  isValidEntryName as isDomainValidEntryName,
  normalizeFsEntry as normalizeDomainFsEntry,
  normalizeFsReadResult,
} from '../../lib/domain/filesystem';
import {
  buildProjectSearchHitId as buildDomainProjectSearchHitId,
  normalizeProjectSearchHit as normalizeDomainProjectSearchHit,
} from '../../lib/domain/projectSearch';
import {
  escapeHtml as escapeDomainHtml,
  splitTextByQuery as splitDomainTextByQuery,
  type TextMatchSegment,
} from '../../lib/domain/projectExplorerText';
import {
  CHANGE_KIND_COLOR_CLASS,
  CHANGE_KIND_LABEL,
  CHANGE_KIND_PRIORITY,
  CHANGE_KIND_ROW_CLASS,
  CHANGE_KIND_TEXT_CLASS,
} from '../../lib/domain/projectExplorerPresentation';
import { getHighlightLanguage as getDomainHighlightLanguage } from '../../lib/domain/projectExplorerPreview';
export type { ChangeKind } from '../../lib/domain/projectExplorer';
export {
  buildProjectChangeSummaryFromGitStatus,
  EMPTY_CHANGE_SUMMARY,
  isProjectChangeSummaryEqual,
  normalizeChangeKind,
  normalizeProjectChangeSummary,
  normalizeProjectRunCatalog,
  normalizeProjectRunTarget,
} from '../../lib/domain/projectExplorer';
export {
  buildCodeNavLocationId,
  normalizeCodeNavCapabilities,
  normalizeCodeNavDocumentSymbol,
  normalizeCodeNavDocumentSymbolsResult,
  normalizeCodeNavLocation,
  normalizeCodeNavLocationsResult,
} from '../../lib/domain/codeNav';

export const normalizeEntry = (raw: unknown): FsEntry => normalizeDomainFsEntry(raw);

export const normalizeFile = (raw: unknown): FsReadResult => normalizeFsReadResult(raw);

export const normalizeProjectSearchHit = (
  raw: FsContentSearchEntryResponse | unknown,
): ProjectSearchHit => normalizeDomainProjectSearchHit(raw);

export const buildProjectSearchHitId = (hit: ProjectSearchHit): string => (
  buildDomainProjectSearchHitId(hit)
);

export const splitTextByQuery = (
  text: string,
  query: string,
  options?: {
    caseSensitive?: boolean;
    wholeWord?: boolean;
  },
): TextMatchSegment[] => splitDomainTextByQuery(text, query, options);

export {
  CHANGE_KIND_COLOR_CLASS,
  CHANGE_KIND_LABEL,
  CHANGE_KIND_PRIORITY,
  CHANGE_KIND_ROW_CLASS,
  CHANGE_KIND_TEXT_CLASS,
};

export const getHighlightLanguage = (filename: string): string | null => (
  getDomainHighlightLanguage(filename)
);

export const escapeHtml = (value: string) => escapeDomainHtml(value);

export const isValidEntryName = (name: string): boolean => isDomainValidEntryName(name);
