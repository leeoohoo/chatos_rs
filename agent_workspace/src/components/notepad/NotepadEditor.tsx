import React from 'react';
import { MarkdownRenderer } from '../MarkdownRenderer';
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
}) => (
  <div className="flex-1 flex flex-col min-w-0">
    <div className="px-4 py-3 border-b border-border flex items-center justify-between">
      <div className="text-sm text-foreground font-medium">
        {selectedNoteId ? '编辑笔记' : '请选择或创建笔记'}
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
            编辑
          </button>
          <button
            type="button"
            onClick={() => onViewModeChange('preview')}
            className={`px-2 py-1 text-xs border-l border-border ${
              viewMode === 'preview' ? 'bg-accent text-foreground' : 'text-muted-foreground hover:bg-accent/70'
            }`}
          >
            预览
          </button>
          <button
            type="button"
            onClick={() => onViewModeChange('split')}
            className={`px-2 py-1 text-xs border-l border-border ${
              viewMode === 'split' ? 'bg-accent text-foreground' : 'text-muted-foreground hover:bg-accent/70'
            }`}
          >
            分栏
          </button>
        </div>
        <button
          type="button"
          onClick={onRefresh}
          className="px-3 py-1.5 text-xs rounded border border-border hover:bg-accent"
        >
          刷新
        </button>
        <button
          type="button"
          onClick={onCopyText}
          disabled={!selectedNoteId}
          className="px-3 py-1.5 text-xs rounded border border-border hover:bg-accent disabled:opacity-50 disabled:cursor-not-allowed"
        >
          复制文本
        </button>
        <button
          type="button"
          onClick={onCopyAsMd}
          disabled={!selectedNoteId}
          className="px-3 py-1.5 text-xs rounded border border-border hover:bg-accent disabled:opacity-50 disabled:cursor-not-allowed"
        >
          复制为.md
        </button>
        <button
          type="button"
          onClick={onSave}
          disabled={!selectedNoteId || !dirty}
          className="px-3 py-1.5 text-xs rounded bg-emerald-600 text-white hover:bg-emerald-700 disabled:opacity-50 disabled:cursor-not-allowed"
        >
          保存
        </button>
        <button
          type="button"
          onClick={onDelete}
          disabled={!selectedNoteId}
          className="px-3 py-1.5 text-xs rounded bg-destructive text-destructive-foreground hover:bg-destructive/90 disabled:opacity-50 disabled:cursor-not-allowed"
        >
          删除
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
          placeholder="标题"
          className="h-10 rounded border border-input bg-background px-3 text-sm"
        />
        <input
          value={tagsText}
          onChange={(event) => onTagsTextChange(event.target.value)}
          placeholder="标签（用逗号分隔）"
          className="h-10 rounded border border-input bg-background px-3 text-sm"
        />
        <div className={`flex-1 min-h-0 ${viewMode === 'split' ? 'grid grid-cols-2 gap-3' : 'flex'}`}>
          {(viewMode === 'edit' || viewMode === 'split') && (
            <textarea
              value={content}
              onChange={(event) => onContentChange(event.target.value)}
              placeholder="Markdown 内容"
              className={`min-h-0 rounded border border-input bg-background p-3 text-sm leading-6 resize-none ${
                viewMode === 'split' ? 'h-full w-full' : 'flex-1 w-full'
              }`}
            />
          )}
          {(viewMode === 'preview' || viewMode === 'split') && (
            <div className={`min-h-0 rounded border border-input bg-background p-3 overflow-y-auto ${
              viewMode === 'split' ? 'h-full w-full' : 'flex-1 w-full'
            }`}>
              <MarkdownRenderer content={content || '（空内容）'} />
            </div>
          )}
        </div>
      </div>
    ) : (
      <div className="flex-1 flex items-center justify-center text-sm text-muted-foreground">
        在左侧选择笔记，或点击“新建笔记”。
      </div>
    )}
  </div>
);
