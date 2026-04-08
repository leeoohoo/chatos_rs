import type React from 'react';

import type { NotepadNoteResponse } from '../../lib/api/client/types';
import { sanitizeFileName, type NoteMeta } from './utils';

export const normalizeNoteMeta = (note: NotepadNoteResponse): NoteMeta => ({
  id: String(note.id || ''),
  title: String(note.title || ''),
  folder: String(note.folder || ''),
  tags: Array.isArray(note.tags) ? note.tags : [],
  updated_at: String(note.updated_at || ''),
});

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
