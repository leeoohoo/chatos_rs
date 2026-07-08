// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

export interface TextMatchSegment {
  text: string;
  matched: boolean;
}

interface SplitTextByQueryOptions {
  caseSensitive?: boolean;
  wholeWord?: boolean;
}

const escapeForRegex = (value: string): string => (
  value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
);

export const splitTextByQuery = (
  text: string,
  query: string,
  options?: SplitTextByQueryOptions,
): TextMatchSegment[] => {
  if (!text) {
    return [{ text: '', matched: false }];
  }

  const keyword = query.trim();
  if (!keyword) {
    return [{ text, matched: false }];
  }

  const segments: TextMatchSegment[] = [];
  const pattern = options?.wholeWord
    ? `\\b${escapeForRegex(keyword)}\\b`
    : escapeForRegex(keyword);
  const flags = options?.caseSensitive ? 'gu' : 'giu';
  const matcher = new RegExp(pattern, flags);

  let cursor = 0;
  let hasMatch = false;
  for (const match of text.matchAll(matcher)) {
    const matchedText = match[0] ?? '';
    const startIndex = match.index ?? -1;
    if (startIndex < 0 || !matchedText) {
      continue;
    }
    hasMatch = true;
    if (startIndex > cursor) {
      segments.push({
        text: text.slice(cursor, startIndex),
        matched: false,
      });
    }
    segments.push({
      text: matchedText,
      matched: true,
    });
    cursor = startIndex + matchedText.length;
  }

  if (!hasMatch) {
    return [{ text, matched: false }];
  }

  if (cursor < text.length) {
    segments.push({
      text: text.slice(cursor),
      matched: false,
    });
  }

  return segments;
};

export const escapeHtml = (value: string) => (
  value
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;')
);
