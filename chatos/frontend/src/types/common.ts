// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

export type UnknownRecord = Record<string, unknown>;

export type Theme = 'light' | 'dark' | 'auto';

export interface StreamResponse {
  content: string;
  done: boolean;
  error?: string;
  metadata?: UnknownRecord;
}

export interface ChatError {
  code: string;
  message: string;
  details?: UnknownRecord;
}

export interface QueryOptions {
  limit?: number;
  offset?: number;
  sortBy?: string;
  sortOrder?: 'asc' | 'desc';
  filters?: UnknownRecord;
}

export interface SearchResult<T> {
  items: T[];
  total: number;
  hasMore: boolean;
}
