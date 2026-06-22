import type {
  CodeNavCapabilitiesResponse,
  CodeNavDocumentSymbolResponse,
  CodeNavDocumentSymbolsResponse,
  CodeNavLocationResponse,
  CodeNavLocationsResponse,
} from '../api/client/types';
import type {
  CodeNavCapabilities,
  CodeNavDocumentSymbol,
  CodeNavDocumentSymbolsResult,
  CodeNavLocation,
  CodeNavLocationsResult,
} from '../../types';
import {
  asRecord,
  readBooleanFirst,
  readNumberFirst,
  readString,
  readStringFirst,
  readValue,
} from './normalizerUtils';

export const normalizeCodeNavCapabilities = (raw: CodeNavCapabilitiesResponse | unknown): CodeNavCapabilities => {
  const record = asRecord(raw);
  return {
    language: readString(record, 'language', 'unknown'),
    provider: readString(record, 'provider', 'fallback'),
    supportsDefinition: readBooleanFirst(record, ['supports_definition', 'supportsDefinition']),
    supportsReferences: readBooleanFirst(record, ['supports_references', 'supportsReferences']),
    supportsDocumentSymbols: readBooleanFirst(record, ['supports_document_symbols', 'supportsDocumentSymbols']),
    fallbackAvailable: readBooleanFirst(record, ['fallback_available', 'fallbackAvailable'], true),
  };
};

export const normalizeCodeNavLocation = (raw: CodeNavLocationResponse | unknown): CodeNavLocation => {
  const record = asRecord(raw);
  const path = readString(record, 'path');
  const line = readNumberFirst(record, ['line'], 1);
  const column = readNumberFirst(record, ['column'], 1);
  return {
    path,
    relativePath: readStringFirst(record, ['relative_path', 'relativePath'], path),
    line,
    column,
    endLine: readNumberFirst(record, ['end_line', 'endLine'], line),
    endColumn: readNumberFirst(record, ['end_column', 'endColumn'], column),
    preview: readString(record, 'preview'),
    score: readNumberFirst(record, ['score']),
  };
};

export const normalizeCodeNavLocationsResult = (raw: CodeNavLocationsResponse | unknown): CodeNavLocationsResult => {
  const record = asRecord(raw);
  const locations = readValue(record, 'locations');
  return {
    provider: readString(record, 'provider', 'fallback'),
    language: readString(record, 'language', 'unknown'),
    mode: readString(record, 'mode', 'unknown'),
    token: (readValue(record, 'token') ?? null) as CodeNavLocationsResult['token'],
    locations: Array.isArray(locations) ? locations.map(normalizeCodeNavLocation) : [],
  };
};

export const normalizeCodeNavDocumentSymbol = (raw: CodeNavDocumentSymbolResponse | unknown): CodeNavDocumentSymbol => {
  const record = asRecord(raw);
  const line = readNumberFirst(record, ['line'], 1);
  const column = readNumberFirst(record, ['column'], 1);
  return {
    name: readString(record, 'name'),
    kind: readString(record, 'kind', 'symbol'),
    line,
    column,
    endLine: readNumberFirst(record, ['end_line', 'endLine'], line),
    endColumn: readNumberFirst(record, ['end_column', 'endColumn'], column),
  };
};

export const normalizeCodeNavDocumentSymbolsResult = (
  raw: CodeNavDocumentSymbolsResponse | unknown,
): CodeNavDocumentSymbolsResult => {
  const record = asRecord(raw);
  const symbols = readValue(record, 'symbols');
  return {
    provider: readString(record, 'provider', 'fallback'),
    language: readString(record, 'language', 'unknown'),
    mode: readString(record, 'mode', 'unknown'),
    symbols: Array.isArray(symbols) ? symbols.map(normalizeCodeNavDocumentSymbol) : [],
  };
};

export const buildCodeNavLocationId = (item: CodeNavLocation): string => (
  `${item.path}:${item.line}:${item.column}:${item.endLine}:${item.endColumn}`
);
