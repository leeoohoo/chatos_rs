import type { ApiRequestFn } from './workspace';

export const notepadInit = (request: ApiRequestFn): Promise<any> => {
  return request<any>('/notepad/init');
};

export const listNotepadFolders = (request: ApiRequestFn): Promise<any> => {
  return request<any>('/notepad/folders');
};

export const createNotepadFolder = (request: ApiRequestFn, payload: { folder: string }): Promise<any> => {
  return request<any>('/notepad/folders', {
    method: 'POST',
    body: JSON.stringify(payload),
  });
};

export const renameNotepadFolder = (
  request: ApiRequestFn,
  payload: { from: string; to: string }
): Promise<any> => {
  return request<any>('/notepad/folders', {
    method: 'PATCH',
    body: JSON.stringify(payload),
  });
};

export const deleteNotepadFolder = (
  request: ApiRequestFn,
  options: { folder: string; recursive?: boolean }
): Promise<any> => {
  const params = new URLSearchParams();
  params.set('folder', options.folder);
  if (options.recursive === true) {
    params.set('recursive', 'true');
  }
  return request<any>('/notepad/folders?' + params.toString(), {
    method: 'DELETE',
  });
};

export const listNotepadNotes = (
  request: ApiRequestFn,
  options?: {
    folder?: string;
    recursive?: boolean;
    tags?: string[];
    match?: 'all' | 'any';
    query?: string;
    limit?: number;
  }
): Promise<any> => {
  const params = new URLSearchParams();
  if (options?.folder) {
    params.set('folder', options.folder);
  }
  if (typeof options?.recursive === 'boolean') {
    params.set('recursive', options.recursive ? 'true' : 'false');
  }
  if (options?.tags && options.tags.length > 0) {
    params.set('tags', options.tags.join(','));
  }
  if (options?.match) {
    params.set('match', options.match);
  }
  if (options?.query) {
    params.set('query', options.query);
  }
  if (typeof options?.limit === 'number') {
    params.set('limit', String(options.limit));
  }
  const query = params.toString();
  return request<any>(`/notepad/notes${query ? `?${query}` : ''}`);
};

export const createNotepadNote = (
  request: ApiRequestFn,
  payload: {
    folder?: string;
    title?: string;
    content?: string;
    tags?: string[];
  }
): Promise<any> => {
  return request<any>('/notepad/notes', {
    method: 'POST',
    body: JSON.stringify(payload),
  });
};

export const getNotepadNote = (request: ApiRequestFn, noteId: string): Promise<any> => {
  return request<any>(`/notepad/notes/${encodeURIComponent(noteId)}`);
};

export const updateNotepadNote = (
  request: ApiRequestFn,
  noteId: string,
  payload: {
    title?: string;
    content?: string;
    folder?: string;
    tags?: string[];
  }
): Promise<any> => {
  return request<any>(`/notepad/notes/${encodeURIComponent(noteId)}`, {
    method: 'PATCH',
    body: JSON.stringify(payload),
  });
};

export const deleteNotepadNote = (request: ApiRequestFn, noteId: string): Promise<any> => {
  return request<any>(`/notepad/notes/${encodeURIComponent(noteId)}`, {
    method: 'DELETE',
  });
};

export const listNotepadTags = (request: ApiRequestFn): Promise<any> => {
  return request<any>('/notepad/tags');
};

export const searchNotepadNotes = (
  request: ApiRequestFn,
  options: {
    query: string;
    folder?: string;
    recursive?: boolean;
    tags?: string[];
    match?: 'all' | 'any';
    include_content?: boolean;
    limit?: number;
  }
): Promise<any> => {
  const params = new URLSearchParams();
  params.set('query', options.query);
  if (options.folder) {
    params.set('folder', options.folder);
  }
  if (typeof options.recursive === 'boolean') {
    params.set('recursive', options.recursive ? 'true' : 'false');
  }
  if (options.tags && options.tags.length > 0) {
    params.set('tags', options.tags.join(','));
  }
  if (options.match) {
    params.set('match', options.match);
  }
  if (typeof options.include_content === 'boolean') {
    params.set('include_content', options.include_content ? 'true' : 'false');
  }
  if (typeof options.limit === 'number') {
    params.set('limit', String(options.limit));
  }
  return request<any>('/notepad/search?' + params.toString());
};
