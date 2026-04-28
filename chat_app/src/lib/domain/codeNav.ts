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

type UnknownRecord = Record<string, unknown>;

const asRecord = (value: unknown): UnknownRecord | null => (
  value !== null && typeof value === 'object' && !Array.isArray(value)
    ? value as UnknownRecord
    : null
);

const readValue = (record: UnknownRecord | null, key: string): unknown => record?.[key];

const readFirst = (record: UnknownRecord | null, keys: string[]): unknown => {
  for (const key of keys) {
    const value = readValue(record, key);
    if (value !== undefined) {
      return value;
    }
  }
  return undefined;
};

const readString = (record: UnknownRecord | null, key: string, fallback = ''): string => {
  const value = readValue(record, key);
  return typeof value === 'string' ? value : fallback;
};

const readStringFirst = (record: UnknownRecord | null, keys: string[], fallback = ''): string => {
  const value = readFirst(record, keys);
  return typeof value === 'string' ? value : fallback;
};

const readNumberFirst = (record: UnknownRecord | null, keys: string[], fallback = 0): number => {
  const value = Number(readFirst(record, keys));
  return Number.isFinite(value) ? value : fallback;
};

const readBooleanFirst = (record: UnknownRecord | null, keys: string[], fallback = false): boolean => (
  Boolean(readFirst(record, keys) ?? fallback)
);

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
