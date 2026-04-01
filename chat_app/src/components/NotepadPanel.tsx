import React from 'react';

import { NotepadContextMenu } from './notepad/NotepadContextMenu';
import { NotepadEditor } from './notepad/NotepadEditor';
import { NotepadSidebar } from './notepad/NotepadSidebar';
import { useNotepadPanelController } from './notepad/useNotepadPanelController';

interface NotepadPanelProps {
  isOpen: boolean;
  onClose: () => void;
}

const NotepadPanel: React.FC<NotepadPanelProps> = ({ isOpen, onClose }) => {
  const controller = useNotepadPanelController({ isOpen });

  if (!isOpen) {
    return null;
  }

  return (
    <>
      <div className="fixed inset-0 bg-black/50 z-40" onClick={onClose} />
      <div className="fixed inset-x-10 top-10 bottom-10 bg-card z-50 rounded-lg border border-border shadow-xl flex overflow-hidden">
        <NotepadSidebar
          onClose={onClose}
          onCreateFolder={() => { void controller.handleCreateFolder(); }}
          onCreateNote={() => { void controller.handleCreateNote(); }}
          searchQuery={controller.searchQuery}
          onSearchQueryChange={controller.setSearchQuery}
          selectedFolder={controller.selectedFolder}
          loading={controller.loading}
          notesCount={controller.notesCount}
          availableFoldersCount={controller.availableFoldersCount}
          folderTree={controller.folderTree}
          selectedNoteId={controller.selectedNoteId}
          expandedFolders={controller.expandedFolders}
          onToggleFolderExpanded={controller.handleToggleFolderExpanded}
          onSelectFolder={controller.handleSelectFolder}
          onOpenNote={controller.handleOpenNote}
          onFolderContextMenu={controller.handleFolderContextMenu}
          onNoteContextMenu={controller.handleNoteContextMenu}
        />
        <NotepadEditor
          selectedNoteId={controller.selectedNoteId}
          viewMode={controller.viewMode}
          onViewModeChange={controller.setViewMode}
          onRefresh={() => { void controller.handleRefresh(); }}
          onCopyText={() => { void controller.handleCopyText(); }}
          onCopyAsMd={() => { void controller.handleCopyAsMd(); }}
          onSave={() => { void controller.handleSave(); }}
          onDelete={() => { void controller.handleDelete(); }}
          dirty={controller.dirty}
          error={controller.error}
          title={controller.title}
          onTitleChange={controller.handleTitleChange}
          tagsText={controller.tagsText}
          onTagsTextChange={controller.handleTagsTextChange}
          content={controller.content}
          onContentChange={controller.handleContentChange}
        />
      </div>

      <NotepadContextMenu
        contextMenu={controller.contextMenu}
        contextMenuStyle={controller.contextMenuStyle}
        selectedNoteMeta={controller.selectedNoteMeta}
        onContextCreateFolder={() => { void controller.handleContextCreateFolder(); }}
        onContextCreateNote={() => { void controller.handleContextCreateNote(); }}
        onContextCopyText={() => { void controller.handleContextCopyText(); }}
        onContextCopyAsMd={() => { void controller.handleContextCopyAsMd(); }}
        onContextDelete={() => { void controller.handleContextDelete(); }}
        onContextDeleteSelectedNote={() => { void controller.handleContextDeleteSelectedNote(); }}
      />
    </>
  );
};

export default NotepadPanel;
