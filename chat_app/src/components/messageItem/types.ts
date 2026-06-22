import type { ToolCall } from '../../types';

export type RenderSegment = {
  type: 'text' | 'thinking' | 'tool_call';
  content?: string;
  toolCallId?: string;
};

export type ToolCallLookupMap = Map<string, ToolCall>;
