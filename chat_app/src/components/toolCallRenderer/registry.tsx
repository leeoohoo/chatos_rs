// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { fallbackToolResultRenderer } from './renderers/FallbackToolResultRenderer';
import { builtinToolResultRenderer } from './renderers/BuiltinToolResultRenderer';
import { structuredToolResultRenderer } from './renderers/StructuredToolResultRenderer';
import type { ToolResultRenderContext, ToolResultRenderer } from './renderers/types';

const TOOL_RESULT_RENDERERS: ToolResultRenderer[] = [
  builtinToolResultRenderer,
  structuredToolResultRenderer,
];

export const resolveToolResultRenderer = (
  context: ToolResultRenderContext,
): ToolResultRenderer => (
  TOOL_RESULT_RENDERERS.find((renderer) => renderer.matches(context))
  ?? fallbackToolResultRenderer
);

export type { ToolResultRenderContext, ToolResultRenderer } from './renderers/types';
