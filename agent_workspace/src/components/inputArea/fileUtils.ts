import type { FsEntry } from '../../types';

interface FsEntryLike {
  name?: string;
  path?: string;
  is_dir?: boolean;
  isDir?: boolean;
  size?: number | null;
  modified_at?: string | null;
  modifiedAt?: string | null;
}

export const MAX_ATTACHMENTS = 20; // 个
export const MAX_FILE_BYTES = 20 * 1024 * 1024; // 20MB
export const MAX_TOTAL_BYTES = 50 * 1024 * 1024; // 50MB

export const formatFileSize = (bytes: number) => {
  if (bytes === 0) return '0 Bytes';
  const k = 1024;
  const sizes = ['Bytes', 'KB', 'MB', 'GB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i];
};

export const normalizeFsEntry = (raw: FsEntryLike): FsEntry => ({
  name: raw?.name ?? '',
  path: raw?.path ?? '',
  isDir: raw?.is_dir ?? raw?.isDir ?? false,
  size: raw?.size ?? null,
  modifiedAt: raw?.modified_at ?? raw?.modifiedAt ?? null,
});

const CODE_FILE_EXTENSIONS = new Set([
  'c', 'cc', 'cpp', 'cs', 'css', 'go', 'h', 'hpp', 'html', 'htm', 'java', 'js', 'jsx', 'kt',
  'kts', 'less', 'lua', 'm', 'md', 'mm', 'php', 'proto', 'py', 'r', 'rb', 'rs', 'scala', 'scss',
  'sh', 'sql', 'svelte', 'swift', 'ts', 'tsx', 'vue', 'xml', 'yml', 'yaml', 'toml', 'ini', 'cfg',
  'conf', 'properties', 'gradle', 'env', 'graphql', 'bash', 'zsh', 'ps1', 'bat', 'make', 'cmake',
]);

const CODE_FILE_NAMES = new Set([
  'dockerfile', 'makefile', 'cmakelists.txt', '.gitignore', '.gitattributes', '.editorconfig',
  '.npmrc', '.yarnrc', '.yarnrc.yml', '.prettierrc', '.eslintrc', '.babelrc', '.env',
  '.env.local', '.env.development', '.env.production',
]);

export const isLikelyCodeFileName = (fileName: string) => {
  const normalized = String(fileName || '').trim().toLowerCase();
  if (!normalized) return false;
  if (CODE_FILE_NAMES.has(normalized)) return true;

  const parts = normalized.split('.');
  if (parts.length >= 2) {
    const ext = parts[parts.length - 1];
    if (CODE_FILE_EXTENSIONS.has(ext)) {
      return true;
    }
  }
  return false;
};

export const fuzzyMatch = (text: string, keyword: string) => {
  if (!keyword) return true;
  if (!text) return false;
  if (text.includes(keyword)) return true;

  let keyIndex = 0;
  for (let i = 0; i < text.length && keyIndex < keyword.length; i++) {
    if (text[i] === keyword[keyIndex]) {
      keyIndex += 1;
    }
  }
  return keyIndex === keyword.length;
};

export const compactSearchText = (value: string) => value.replace(/[\s._\-\/]+/g, '');
