// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

export interface CodeNavCapabilitiesResponse {
  language?: string;
  provider?: string;
  supports_definition?: boolean;
  supportsDefinition?: boolean;
  supports_references?: boolean;
  supportsReferences?: boolean;
  supports_document_symbols?: boolean;
  supportsDocumentSymbols?: boolean;
  fallback_available?: boolean;
  fallbackAvailable?: boolean;
}

export interface CodeNavLocationResponse {
  path?: string;
  relative_path?: string;
  relativePath?: string;
  line?: number;
  column?: number;
  end_line?: number;
  endLine?: number;
  end_column?: number;
  endColumn?: number;
  preview?: string;
  score?: number;
}

export interface CodeNavLocationsResponse {
  provider?: string;
  language?: string;
  mode?: string;
  token?: string | null;
  locations?: CodeNavLocationResponse[];
}

export interface CodeNavDocumentSymbolResponse {
  name?: string;
  kind?: string;
  line?: number;
  column?: number;
  end_line?: number;
  endLine?: number;
  end_column?: number;
  endColumn?: number;
}

export interface CodeNavDocumentSymbolsResponse {
  provider?: string;
  language?: string;
  mode?: string;
  symbols?: CodeNavDocumentSymbolResponse[];
}
