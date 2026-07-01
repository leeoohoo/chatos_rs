// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';
import { useI18n } from '../../i18n/I18nProvider';
import type { NoteMeta } from './utils';

export type ContextMenuTarget =
  | { type: 'folder'; folderPath: string }
  | { type: 'note'; note: NoteMeta };

export interface ContextMenuState {
  x: number;
  y: number;
  target: ContextMenuTarget;
}

interface NotepadContextMenuProps {
  contextMenu: ContextMenuState | null;
  contextMenuStyle?: React.CSSProperties;
  selectedNoteMeta: NoteMeta | null;
  onContextCreateFolder: () => void;
  onContextCreateNote: () => void;
  onContextCopyText: () => void;
  onContextCopyAsMd: () => void;
  onContextDelete: () => void;
  onContextDeleteSelectedNote: () => void;
}

export const NotepadContextMenu: React.FC<NotepadContextMenuProps> = ({
  contextMenu,
  contextMenuStyle,
  selectedNoteMeta,
  onContextCreateFolder,
  onContextCreateNote,
  onContextCopyText,
  onContextCopyAsMd,
  onContextDelete,
  onContextDeleteSelectedNote,
}) => {
  const { t } = useI18n();

  if (!contextMenu || !contextMenuStyle) {
    return null;
  }

  return (
    <div
      className="fixed z-[80] w-56 rounded-md border border-border bg-popover text-popover-foreground shadow-lg p-1"
      style={contextMenuStyle}
      onClick={(event) => event.stopPropagation()}
      onContextMenu={(event) => event.preventDefault()}
    >
      <div className="px-2 py-1 text-[11px] text-muted-foreground truncate">
        {contextMenu.target.type === 'folder'
          ? t('notepad.context.folderLabel', { name: contextMenu.target.folderPath || t('notepad.rootFolder') })
          : t('notepad.context.noteLabel', { name: contextMenu.target.note.title || t('notepad.tree.noteUntitled') })}
      </div>
      <button
        type="button"
        onClick={onContextCreateFolder}
        className="w-full text-left px-2 py-1.5 text-sm rounded hover:bg-accent"
      >
        {t('notepad.action.createFolder')}
      </button>
      <button
        type="button"
        onClick={onContextCreateNote}
        className="w-full text-left px-2 py-1.5 text-sm rounded hover:bg-accent"
      >
        {t('notepad.action.createNote')}
      </button>
      {(contextMenu.target.type === 'note' || selectedNoteMeta) && (
        <button
          type="button"
          onClick={onContextCopyText}
          className="w-full text-left px-2 py-1.5 text-sm rounded hover:bg-accent"
        >
          {t('notepad.action.copyText')}
        </button>
      )}
      {(contextMenu.target.type === 'note' || selectedNoteMeta) && (
        <button
          type="button"
          onClick={onContextCopyAsMd}
          className="w-full text-left px-2 py-1.5 text-sm rounded hover:bg-accent"
        >
          {t('notepad.action.copyAsMdFile')}
        </button>
      )}
      <button
        type="button"
        onClick={onContextDelete}
        className="w-full text-left px-2 py-1.5 text-sm rounded text-destructive hover:bg-destructive/10"
      >
        {contextMenu.target.type === 'folder' ? t('notepad.action.deleteFolder') : t('notepad.action.deleteNote')}
      </button>
      {contextMenu.target.type === 'folder' && selectedNoteMeta && (
        <button
          type="button"
          onClick={onContextDeleteSelectedNote}
          className="w-full text-left px-2 py-1.5 text-sm rounded text-destructive hover:bg-destructive/10"
        >
          {t('notepad.action.deleteSelectedNote')}
        </button>
      )}
    </div>
  );
};
