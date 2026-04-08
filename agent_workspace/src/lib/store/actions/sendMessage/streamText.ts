export const cloneStreamingMessageDraft = <T,>(value: T): T => {
  try {
    if (typeof structuredClone === 'function') {
      return structuredClone(value);
    }
  } catch {
    // ignore and fallback to JSON clone
  }

  try {
    return JSON.parse(JSON.stringify(value));
  } catch {
    return value;
  }
};

export const joinStreamingText = (current: string, chunk: string): string => {
  if (!chunk) return current;
  if (!current) return chunk;

  // 兼容部分模型返回累计快照、部分模型返回增量。
  if (chunk.startsWith(current)) return chunk;
  if (current.startsWith(chunk)) return current;

  const maxOverlap = Math.min(current.length, chunk.length);
  for (let overlap = maxOverlap; overlap >= 8; overlap -= 1) {
    if (current.slice(-overlap) === chunk.slice(0, overlap)) {
      return `${current}${chunk.slice(overlap)}`;
    }
  }

  return `${current}${chunk}`;
};

export const normalizeStreamedText = (value: string): string => {
  if (!value) return value;
  return value
    .replace(/\r\n?/g, '\n')
    .replace(/\n{6,}/g, '\n\n\n\n');
};
