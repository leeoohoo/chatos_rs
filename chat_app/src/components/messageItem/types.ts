import type { ToolCall } from '../../types';

export type DerivedProcessStats = {
  hasProcess: boolean;
  hasStreamingAssistant: boolean;
  toolCallCount: number;
  thinkingCount: number;
  processMessageCount: number;
};

export type RenderSegment = {
  type: 'text' | 'thinking' | 'tool_call';
  content?: string;
  toolCallId?: string;
};

export type ToolCallLookupMap = Map<string, ToolCall>;
