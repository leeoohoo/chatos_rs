import type { RenderSegment } from './types';

export const EMPTY_DERIVED_PROCESS_STATS = {
  hasProcess: false,
  hasStreamingAssistant: false,
  toolCallCount: 0,
  thinkingCount: 0,
  processMessageCount: 0,
};

const compactTextChunks = (chunks: string[]): string => {
  let merged = '';
  for (const rawChunk of chunks) {
    const chunk = typeof rawChunk === 'string' ? rawChunk : '';
    if (!chunk) {
      continue;
    }
    if (!merged) {
      merged = chunk;
      continue;
    }
    if (chunk.startsWith(merged)) {
      merged = chunk;
      continue;
    }
    if (merged.startsWith(chunk) || merged.endsWith(chunk)) {
      continue;
    }
    merged += chunk;
  }
  return merged;
};

export const normalizeMetaId = (value: unknown): string => (
  typeof value === 'string' ? value.trim() : ''
);

export const normalizeContentSegmentsForRender = (segments: unknown[]): RenderSegment[] => {
  if (!Array.isArray(segments) || segments.length === 0) {
    return [];
  }

  const normalized: RenderSegment[] = [];
  let index = 0;

  while (index < segments.length) {
    const segment = segments[index] as Record<string, unknown> | undefined;
    const type = String(segment?.type || '').trim();

    if (type === 'text') {
      const textChunks: string[] = [];
      while (index < segments.length) {
        const current = segments[index] as Record<string, unknown> | undefined;
        if (String(current?.type || '').trim() !== 'text') {
          break;
        }
        const chunk = typeof current?.content === 'string' ? current.content : '';
        if (chunk) {
          textChunks.push(chunk);
        }
        index += 1;
      }
      const mergedText = compactTextChunks(textChunks);
      if (mergedText.trim().length > 0) {
        normalized.push({ type: 'text', content: mergedText });
      }
      continue;
    }

    if (type === 'thinking') {
      const content = typeof segment?.content === 'string' ? segment.content : '';
      if (content.trim().length > 0) {
        normalized.push({ type: 'thinking', content });
      }
      index += 1;
      continue;
    }

    if (type === 'tool_call') {
      const toolCallId = typeof segment?.toolCallId === 'string' ? segment.toolCallId.trim() : '';
      if (toolCallId) {
        normalized.push({ type: 'tool_call', toolCallId });
      }
      index += 1;
      continue;
    }

    index += 1;
  }

  return normalized;
};
