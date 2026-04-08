export interface NoteMeta {
  id: string;
  title: string;
  folder: string;
  tags: string[];
  updated_at: string;
}

export interface FolderNode {
  name: string;
  path: string;
  folders: FolderNode[];
  notes: NoteMeta[];
}

const createFolderNode = (name: string, path: string): FolderNode => ({
  name,
  path,
  folders: [],
  notes: [],
});

export const parseTags = (raw: string): string[] => (
  raw
    .split(',')
    .map((item) => item.trim())
    .filter((item) => item.length > 0)
);

export const formatTime = (raw: string | undefined): string => {
  if (!raw) return '';
  const date = new Date(raw);
  if (Number.isNaN(date.getTime())) return raw;
  return date.toLocaleString();
};

export const normalizeFolderPath = (raw: string | undefined | null): string => {
  const input = String(raw || '');
  return input.trim().replace(/\\/g, '/').replace(/^\/+|\/+$/g, '');
};

export const sanitizeFileName = (raw: string): string => {
  const cleaned = String(raw || '')
    .replace(/[\\/:*?"<>|]+/g, '_')
    .replace(/\s+/g, ' ')
    .trim();
  return cleaned || 'note';
};

const noteUpdatedAtTs = (note: NoteMeta): number => {
  const value = Date.parse(note.updated_at || '');
  return Number.isNaN(value) ? 0 : value;
};

export const buildFolderTree = (folders: string[], notes: NoteMeta[]): FolderNode => {
  const root = createFolderNode('', '');
  const nodeMap = new Map<string, FolderNode>();
  nodeMap.set('', root);

  const ensureNode = (rawPath: string): FolderNode => {
    const normalized = rawPath.trim().replace(/\\/g, '/').replace(/^\/+|\/+$/g, '');
    if (!normalized) {
      return root;
    }
    const cached = nodeMap.get(normalized);
    if (cached) {
      return cached;
    }

    const parts = normalized.split('/').filter((item) => item.trim().length > 0);
    let currentPath = '';
    let parentNode = root;
    for (const part of parts) {
      currentPath = currentPath ? `${currentPath}/${part}` : part;
      let currentNode = nodeMap.get(currentPath);
      if (!currentNode) {
        currentNode = createFolderNode(part, currentPath);
        parentNode.folders.push(currentNode);
        nodeMap.set(currentPath, currentNode);
      }
      parentNode = currentNode;
    }
    return parentNode;
  };

  for (const folder of folders) {
    ensureNode(folder);
  }

  for (const note of notes) {
    const folderNode = ensureNode(note.folder || '');
    folderNode.notes.push(note);
  }

  const sortNode = (node: FolderNode) => {
    node.folders.sort((left, right) => left.name.localeCompare(right.name, 'zh-Hans-CN'));
    node.notes.sort((left, right) => {
      const delta = noteUpdatedAtTs(right) - noteUpdatedAtTs(left);
      if (delta !== 0) {
        return delta;
      }
      return (left.title || '').localeCompare(right.title || '', 'zh-Hans-CN');
    });
    for (const child of node.folders) {
      sortNode(child);
    }
  };

  sortNode(root);
  return root;
};
