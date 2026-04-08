import React from 'react';
import { formatTime, type FolderNode, type NoteMeta } from './utils';

interface NotepadTreeProps {
  folderTree: FolderNode;
  selectedFolder: string;
  selectedNoteId: string;
  expandedFolders: Set<string>;
  onToggleFolderExpanded: (folderPath: string) => void;
  onSelectFolder: (folderPath: string) => void;
  onOpenNote: (noteId: string) => void;
  onFolderContextMenu: (event: React.MouseEvent, folderPath: string) => void;
  onNoteContextMenu: (event: React.MouseEvent, note: NoteMeta) => void;
}

const renderNote = (
  note: NoteMeta,
  paddingLeft: string,
  selectedNoteId: string,
  onOpenNote: (noteId: string) => void,
  onNoteContextMenu: (event: React.MouseEvent, note: NoteMeta) => void,
) => (
  <button
    key={note.id}
    type="button"
    onClick={() => { onOpenNote(note.id); }}
    onContextMenu={(event) => onNoteContextMenu(event, note)}
    className={`w-full text-left rounded px-2 py-1.5 ${
      selectedNoteId === note.id
        ? 'bg-indigo-500/10 border border-indigo-500/50'
        : 'hover:bg-accent border border-transparent'
    }`}
    style={{ paddingLeft }}
    title={note.title || 'Untitled'}
  >
    <div className="text-sm text-foreground truncate">📄 {note.title || 'Untitled'}</div>
    <div className="text-[10px] text-muted-foreground truncate">
      {note.updated_at ? formatTime(note.updated_at) : ''}
    </div>
  </button>
);

const renderFolderNode = (
  node: FolderNode,
  depth: number,
  props: NotepadTreeProps,
): React.ReactNode => {
  const folderKey = node.path || '__root__';
  const expanded = props.expandedFolders.has(node.path);
  const hasChildren = node.folders.length > 0 || node.notes.length > 0;
  const indent = 8 + depth * 14;

  return (
    <div key={folderKey}>
      <div
        className={`group flex items-center gap-1 rounded px-1 py-1 ${
          props.selectedFolder === node.path
            ? 'bg-indigo-500/10 text-indigo-600 dark:text-indigo-300'
            : 'hover:bg-accent'
        }`}
        style={{ paddingLeft: `${indent}px` }}
        onContextMenu={(event) => props.onFolderContextMenu(event, node.path)}
      >
        <button
          type="button"
          onClick={(event) => {
            event.stopPropagation();
            if (hasChildren) {
              props.onToggleFolderExpanded(node.path);
            }
          }}
          className="w-4 h-4 text-[10px] text-muted-foreground hover:text-foreground"
          title={hasChildren ? (expanded ? '收起目录' : '展开目录') : '空目录'}
        >
          {hasChildren ? (expanded ? '▾' : '▸') : '·'}
        </button>
        <button
          type="button"
          onClick={() => props.onSelectFolder(node.path)}
          className="flex-1 min-w-0 text-left text-sm truncate"
          title={node.path}
        >
          {node.name}
        </button>
      </div>
      {expanded && (
        <>
          {node.folders.map((child) => renderFolderNode(child, depth + 1, props))}
          {node.notes.map((note) => renderNote(
            note,
            `${indent + 18}px`,
            props.selectedNoteId,
            props.onOpenNote,
            props.onNoteContextMenu,
          ))}
        </>
      )}
    </div>
  );
};

export const NotepadTree: React.FC<NotepadTreeProps> = (props) => (
  <>
    {props.folderTree.folders.map((folder) => renderFolderNode(folder, 0, props))}
    {props.folderTree.notes.map((note) => renderNote(
      note,
      '26px',
      props.selectedNoteId,
      props.onOpenNote,
      props.onNoteContextMenu,
    ))}
  </>
);
