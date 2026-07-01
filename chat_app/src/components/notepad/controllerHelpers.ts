// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type React from 'react';

import type { NotepadNoteDetailResponse, NotepadNoteResponse } from '../../lib/api/client/types';
import { sanitizeFileName, type NoteDetail, type NoteMeta } from './utils';

export const normalizeNoteMeta = (note: NotepadNoteResponse): NoteMeta => ({
  id: String(note.id || ''),
  title: String(note.title || ''),
  folder: String(note.folder || ''),
  tags: Array.isArray(note.tags) ? note.tags : [],
  updated_at: String(note.updated_at || ''),
});

export const normalizeNoteDetail = (response: NotepadNoteDetailResponse, fallbackId?: string): NoteDetail => {
  const normalizedNote = normalizeNoteMeta(response?.note || {
    id: String(response?.note?.id || fallbackId || ''),
    title: String(response?.note?.title || ''),
    folder: String(response?.note?.folder || ''),
    tags: Array.isArray(response?.note?.tags) ? response.note.tags : [],
    created_at: String(response?.note?.created_at || ''),
    updated_at: String(response?.note?.updated_at || ''),
    file: String(response?.note?.file || ''),
  });

  return {
    note: normalizedNote,
    content: String(response?.content || ''),
  };
};

export const copyTextToClipboard = async (text: string): Promise<void> => {
  if (typeof navigator !== 'undefined' && navigator.clipboard?.writeText) {
    await navigator.clipboard.writeText(text);
    return;
  }

  if (typeof document !== 'undefined') {
    const textarea = document.createElement('textarea');
    textarea.value = text;
    textarea.style.position = 'fixed';
    textarea.style.opacity = '0';
    document.body.appendChild(textarea);
    textarea.select();
    document.execCommand('copy');
    document.body.removeChild(textarea);
    return;
  }

  throw new Error('clipboard is not available');
};

export const downloadMarkdownFile = (title: string, content: string): void => {
  if (typeof document === 'undefined') {
    throw new Error('document is not available');
  }
  const filename = `${sanitizeFileName(title)}.md`;
  const blob = new Blob([content], { type: 'text/markdown;charset=utf-8' });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement('a');
  anchor.href = url;
  anchor.download = filename;
  anchor.style.display = 'none';
  document.body.appendChild(anchor);
  anchor.click();
  document.body.removeChild(anchor);
  URL.revokeObjectURL(url);
};

export const buildContextMenuStyle = (
  contextMenu: { x: number; y: number } | null,
): React.CSSProperties | undefined => {
  if (!contextMenu) {
    return undefined;
  }
  const maxX = typeof window !== 'undefined' ? window.innerWidth - 220 : contextMenu.x;
  const maxY = typeof window !== 'undefined' ? window.innerHeight - 190 : contextMenu.y;
  return {
    left: `${Math.max(8, Math.min(contextMenu.x, maxX))}px`,
    top: `${Math.max(8, Math.min(contextMenu.y, maxY))}px`,
  };
};
