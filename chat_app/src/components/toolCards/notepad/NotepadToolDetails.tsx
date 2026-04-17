import React from 'react';

import { RowsCard, StringListCard, TextBlockCard, renderCardHeader } from '../shared/primitives';
import { asArray, asBoolean, asNumber, asRecord, asString } from '../shared/value';

const normalizeFolder = (folder: string): string => folder.trim() || 'root';

const NoteListCard: React.FC<{ title: string; items: unknown[] }> = ({ title, items }) => {
  const notes = items
    .map((item) => asRecord(item))
    .filter((item): item is Record<string, unknown> => item !== null);

  if (notes.length === 0) return null;

  return (
    <div className="tool-detail-card tool-detail-card--full">
      {renderCardHeader(title, `${notes.length} 条`)}
      <div className="tool-detail-list">
        {notes.map((note, index) => {
          const titleText = asString(note.title).trim() || `note ${index + 1}`;
          const folder = normalizeFolder(asString(note.folder).trim());
          const tags = asArray(note.tags).map((item) => asString(item).trim()).filter(Boolean);
          const updatedAt = asString(note.updated_at ?? note.updatedAt).trim();
          const file = asString(note.file).trim();

          return (
            <div key={`notepad-note-${index}`} className="tool-detail-item">
              <div className="tool-detail-item-title">{titleText}</div>
              <div className="tool-detail-item-meta">
                {[folder, updatedAt].filter(Boolean).join(' · ')}
              </div>
              <div className="tool-detail-item-body">
                {[file, tags.length > 0 ? `#${tags.join(' #')}` : ''].filter(Boolean).join(' · ')}
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
};

const TagListCard: React.FC<{ items: unknown[] }> = ({ items }) => {
  const tags = items
    .map((item) => asRecord(item))
    .filter((item): item is Record<string, unknown> => item !== null);

  if (tags.length === 0) return null;

  return (
    <div className="tool-detail-card tool-detail-card--full">
      {renderCardHeader('Tags', `${tags.length} 个`)}
      <div className="tool-detail-list">
        {tags.map((tag, index) => (
          <div key={`notepad-tag-${index}`} className="tool-detail-item">
            <div className="tool-detail-item-title">{asString(tag.tag).trim() || `tag ${index + 1}`}</div>
            <div className="tool-detail-item-meta">{asNumber(tag.count) ?? 0} notes</div>
          </div>
        ))}
      </div>
    </div>
  );
};

interface NotepadToolDetailsProps {
  displayName: string;
  result: unknown;
}

export const NotepadToolDetails: React.FC<NotepadToolDetailsProps> = ({
  displayName,
  result,
}) => {
  const record = asRecord(result);
  if (!record) return null;

  if (displayName === 'init') {
    return (
      <div className="tool-detail-stack">
        <RowsCard
          title="Notepad ready"
          rows={[
            { key: 'initialized', value: asBoolean(record.ok) },
            { key: 'notes', value: asNumber(record.notes) },
            { key: 'version', value: asNumber(record.version) },
          ]}
        />
      </div>
    );
  }

  if (displayName === 'list_folders') {
    return (
      <div className="tool-detail-stack">
        <RowsCard
          title="Folder summary"
          rows={[
            { key: 'count', value: asArray(record.folders).length },
          ]}
        />
        <StringListCard
          title="Folders"
          values={asArray(record.folders).map((item) => normalizeFolder(asString(item)))}
          fullWidth
        />
      </div>
    );
  }

  if (displayName === 'list_notes' || displayName === 'search_notes') {
    return (
      <div className="tool-detail-stack">
        <RowsCard
          title={displayName === 'search_notes' ? 'Search result' : 'Note summary'}
          rows={[
            { key: 'count', value: asArray(record.notes).length },
          ]}
        />
        <NoteListCard title="Notes" items={asArray(record.notes)} />
      </div>
    );
  }

  if (displayName === 'list_tags') {
    return (
      <div className="tool-detail-stack">
        <RowsCard
          title="Tag summary"
          rows={[
            { key: 'count', value: asArray(record.tags).length },
          ]}
        />
        <TagListCard items={asArray(record.tags)} />
      </div>
    );
  }

  if (displayName === 'read_note') {
    const note = asRecord(record.note);
    const tags = asArray(note?.tags).map((item) => asString(item).trim()).filter(Boolean);

    return (
      <div className="tool-detail-stack">
        <RowsCard
          title="Note"
          rows={[
            { key: 'title', value: asString(note?.title).trim() },
            { key: 'folder', value: normalizeFolder(asString(note?.folder).trim()) },
            { key: 'file', value: asString(note?.file).trim() },
            { key: 'updated at', value: asString(note?.updated_at ?? note?.updatedAt).trim() },
            { key: 'tags', value: tags.length > 0 ? tags.join(', ') : '' },
          ]}
          fullWidth
        />
        <TextBlockCard title="Note content" content={asString(record.content)} />
      </div>
    );
  }

  if (displayName === 'create_note' || displayName === 'update_note') {
    const note = asRecord(record.note);
    const tags = asArray(note?.tags).map((item) => asString(item).trim()).filter(Boolean);

    return (
      <div className="tool-detail-stack">
        <RowsCard
          title="Saved note"
          rows={[
            { key: 'title', value: asString(note?.title).trim() },
            { key: 'folder', value: normalizeFolder(asString(note?.folder).trim()) },
            { key: 'file', value: asString(note?.file).trim() },
            { key: 'updated at', value: asString(note?.updated_at ?? note?.updatedAt).trim() },
            { key: 'tags', value: tags.length > 0 ? tags.join(', ') : '' },
          ]}
          fullWidth
        />
      </div>
    );
  }

  if (displayName === 'create_folder' || displayName === 'delete_folder') {
    return (
      <div className="tool-detail-stack">
        <RowsCard
          title="Folder result"
          rows={[
            { key: 'success', value: asBoolean(record.ok) },
            { key: 'folder', value: normalizeFolder(asString(record.folder).trim()) },
            { key: 'deleted notes', value: asNumber(record.deleted_notes ?? record.deletedNotes) },
          ]}
        />
      </div>
    );
  }

  if (displayName === 'rename_folder') {
    return (
      <div className="tool-detail-stack">
        <RowsCard
          title="Folder moved"
          rows={[
            { key: 'success', value: asBoolean(record.ok) },
            { key: 'from', value: normalizeFolder(asString(record.from).trim()) },
            { key: 'to', value: normalizeFolder(asString(record.to).trim()) },
            { key: 'moved notes', value: asNumber(record.moved_notes ?? record.movedNotes) },
          ]}
        />
      </div>
    );
  }

  if (displayName === 'delete_note') {
    return (
      <div className="tool-detail-stack">
        <RowsCard
          title="Delete result"
          rows={[
            { key: 'deleted', value: asBoolean(record.ok) },
            { key: 'note id', value: asString(record.id).trim() },
          ]}
        />
      </div>
    );
  }

  return (
    <div className="tool-detail-stack">
      <RowsCard
        title="Notepad result"
        rows={[
          { key: 'success', value: asBoolean(record.ok) },
          { key: 'folder', value: normalizeFolder(asString(record.folder).trim()) },
          { key: 'notes', value: asNumber(record.notes) },
          { key: 'version', value: asNumber(record.version) },
        ]}
      />
    </div>
  );
};

export default NotepadToolDetails;
