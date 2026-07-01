// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { ReactNode } from 'react';
import type { UiLocale } from '../../../i18n/messages';

export interface ToolResultRenderContext {
  toolName: string;
  displayToolName: string;
  result: unknown;
  parsedResult: Record<string, unknown> | null;
  structuredDisplayResult: unknown;
  hasStructuredResult: boolean;
  structuredResultNote: string;
}

export interface ToolResultRenderer {
  id: string;
  sourceLabel: (locale: UiLocale) => string;
  matches: (context: ToolResultRenderContext) => boolean;
  render: (context: ToolResultRenderContext) => ReactNode;
}
