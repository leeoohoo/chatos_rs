// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';
import { useI18n } from '../../i18n/I18nProvider';
import { LazyMarkdownRenderer } from '../LazyMarkdownRenderer';
export type NotepadViewMode = 'edit' | 'preview' | 'split';

interface NotepadEditorProps {
  selectedNoteId: string;
  viewMode: NotepadViewMode;
  onViewModeChange: (mode: NotepadViewMode) => void;
  onRefresh: () => void;
  onCopyText: () => void;
  onCopyAsMd: () => void;
  onSave: () => void;
  onDelete: () => void;
  dirty: boolean;
  error: string | null;
  title: string;
  onTitleChange: (value: string) => void;
  tagsText: string;
  onTagsTextChange: (value: string) => void;
  content: string;
  onContentChange: (value: string) => void;
}

export const NotepadEditor: React.FC<NotepadEditorProps> = ({
  selectedNoteId,
  viewMode,
  onViewModeChange,
  onRefresh,
  onCopyText,
  onCopyAsMd,
  onSave,
  onDelete,
  dirty,
  error,
  title,
  onTitleChange,
  tagsText,
  onTagsTextChange,
  content,
  onContentChange,
}) => {
  const { t } = useI18n();

  return (
    <div className="flex-1 flex flex-col min-w-0">
      <div className="px-4 py-3 border-b border-border flex items-center justify-between">
        <div className="text-sm text-foreground font-medium">
          {selectedNoteId ? t('notepad.editor.titleEditing') : t('notepad.editor.titleEmpty')}
        </div>
        <div className="flex items-center gap-2">
          <div className="flex items-center rounded border border-border overflow-hidden">
            <button
              type="button"
              onClick={() => onViewModeChange('edit')}
              className={`px-2 py-1 text-xs ${
                viewMode === 'edit' ? 'bg-accent text-foreground' : 'text-muted-foreground hover:bg-accent/70'
              }`}
            >
              {t('notepad.editor.mode.edit')}
            </button>
            <button
              type="button"
              onClick={() => onViewModeChange('preview')}
              className={`px-2 py-1 text-xs border-l border-border ${
                viewMode === 'preview' ? 'bg-accent text-foreground' : 'text-muted-foreground hover:bg-accent/70'
              }`}
            >
              {t('notepad.editor.mode.preview')}
            </button>
            <button
              type="button"
              onClick={() => onViewModeChange('split')}
              className={`px-2 py-1 text-xs border-l border-border ${
                viewMode === 'split' ? 'bg-accent text-foreground' : 'text-muted-foreground hover:bg-accent/70'
              }`}
            >
              {t('notepad.editor.mode.split')}
            </button>
          </div>
          <button
            type="button"
            onClick={onRefresh}
            className="px-3 py-1.5 text-xs rounded border border-border hover:bg-accent"
          >
            {t('notepad.action.refresh')}
          </button>
          <button
            type="button"
            onClick={onCopyText}
            disabled={!selectedNoteId}
            className="px-3 py-1.5 text-xs rounded border border-border hover:bg-accent disabled:opacity-50 disabled:cursor-not-allowed"
          >
            {t('notepad.action.copyText')}
          </button>
          <button
            type="button"
            onClick={onCopyAsMd}
            disabled={!selectedNoteId}
            className="px-3 py-1.5 text-xs rounded border border-border hover:bg-accent disabled:opacity-50 disabled:cursor-not-allowed"
          >
            {t('notepad.action.copyAsMd')}
          </button>
          <button
            type="button"
            onClick={onSave}
            disabled={!selectedNoteId || !dirty}
            className="px-3 py-1.5 text-xs rounded bg-emerald-600 text-white hover:bg-emerald-700 disabled:opacity-50 disabled:cursor-not-allowed"
          >
            {t('common.save')}
          </button>
          <button
            type="button"
            onClick={onDelete}
            disabled={!selectedNoteId}
            className="px-3 py-1.5 text-xs rounded bg-destructive text-destructive-foreground hover:bg-destructive/90 disabled:opacity-50 disabled:cursor-not-allowed"
          >
            {t('aiModelManager.action.delete')}
          </button>
        </div>
      </div>

      {error ? (
        <div className="mx-4 mt-3 px-3 py-2 text-xs rounded border border-destructive/30 bg-destructive/10 text-destructive">
          {error}
        </div>
      ) : null}

      {selectedNoteId ? (
        <div className="flex-1 min-h-0 flex flex-col p-4 gap-3">
          <input
            value={title}
            onChange={(event) => onTitleChange(event.target.value)}
            placeholder={t('notepad.editor.titlePlaceholder')}
            className="h-10 rounded border border-input bg-background px-3 text-sm"
          />
          <input
            value={tagsText}
            onChange={(event) => onTagsTextChange(event.target.value)}
            placeholder={t('notepad.editor.tagsPlaceholder')}
            className="h-10 rounded border border-input bg-background px-3 text-sm"
          />
          <div className={`flex-1 min-h-0 ${viewMode === 'split' ? 'grid grid-cols-2 gap-3' : 'flex'}`}>
            {(viewMode === 'edit' || viewMode === 'split') && (
              <textarea
                value={content}
                onChange={(event) => onContentChange(event.target.value)}
                placeholder={t('notepad.editor.contentPlaceholder')}
                className={`min-h-0 rounded border border-input bg-background p-3 text-sm leading-6 resize-none ${
                  viewMode === 'split' ? 'h-full w-full' : 'flex-1 w-full'
                }`}
              />
            )}
            {(viewMode === 'preview' || viewMode === 'split') && (
              <div className={`min-h-0 rounded border border-input bg-background p-3 overflow-y-auto ${
                viewMode === 'split' ? 'h-full w-full' : 'flex-1 w-full'
              }`}>
                <LazyMarkdownRenderer content={content || t('notepad.editor.emptyContent')} />
              </div>
            )}
          </div>
        </div>
      ) : (
        <div className="flex-1 flex items-center justify-center text-sm text-muted-foreground">
          {t('notepad.editor.emptyState')}
        </div>
      )}
    </div>
  );
};
