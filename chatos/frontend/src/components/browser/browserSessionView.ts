// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { BrowserSessionCommandResponse } from '../../lib/api/localRuntime/browserSession';

export const text = (value: unknown): string => (typeof value === 'string' ? value.trim() : '');
export const number = (value: unknown): number => (typeof value === 'number' && Number.isFinite(value) ? value : 0);
export const record = (value: unknown): Record<string, unknown> | null => (
  value && typeof value === 'object' && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null
);
export const records = (value: unknown): Record<string, unknown>[] => (
  Array.isArray(value)
    ? value.map(record).filter((item): item is Record<string, unknown> => item !== null)
    : []
);

export type BrowserDetailTab = 'snapshot' | 'console' | 'network' | 'websocket';
export type BrowserPreviewCursor = { x: number; y: number };
export type BrowserPreviewPoint = BrowserPreviewCursor & { browserX: number; browserY: number };

export const pageValue = (response: BrowserSessionCommandResponse | null, key: string): string => (
  text(response?.page?.[key])
);

export const readableError = (error: unknown): string => (
  error instanceof Error ? error.message : String(error || 'Unknown error')
);
