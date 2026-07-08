// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { ToolCall } from '../../types';

export type RenderSegment = {
  type: 'text' | 'thinking' | 'tool_call';
  content?: string;
  toolCallId?: string;
};

export type ToolCallLookupMap = Map<string, ToolCall>;
