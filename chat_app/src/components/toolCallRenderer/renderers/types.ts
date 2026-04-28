import type { ReactNode } from 'react';

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
  sourceLabel: string;
  matches: (context: ToolResultRenderContext) => boolean;
  render: (context: ToolResultRenderContext) => ReactNode;
}
