const quote = (value: string): string => {
  if (!value) return "''";
  return `'${value.replace(/'/g, `'\"'\"'`)}'`;
};

const extOf = (path: string): string => {
  const name = path.split('/').pop() || path.split('\\').pop() || '';
  const dot = name.lastIndexOf('.');
  if (dot < 0) return '';
  return name.slice(dot + 1).toLowerCase();
};

const basenameOf = (path: string): string => {
  const value = path.split('/').pop() || path.split('\\').pop() || path;
  return value.trim();
};

const parentOf = (path: string): string | null => {
  const normalized = path.replace(/\\/g, '/');
  const idx = normalized.lastIndexOf('/');
  if (idx <= 0) return null;
  return normalized.slice(0, idx);
};

export interface SingleFileRunProfile {
  cwd: string;
  command: string;
  label: string;
}

export const buildSingleFileRunProfile = (path: string): SingleFileRunProfile | null => {
  const cwd = parentOf(path);
  const base = basenameOf(path);
  if (!cwd || !base) return null;

  const ext = extOf(path);
  const q = quote(base);
  if (ext === 'py') {
    return { cwd, command: `python ${q}`, label: 'Python Run' };
  }
  if (ext === 'js' || ext === 'mjs' || ext === 'cjs') {
    return { cwd, command: `node ${q}`, label: 'Node Run' };
  }
  if (ext === 'ts') {
    return { cwd, command: `tsx ${q}`, label: 'TS Run' };
  }
  if (ext === 'rb') {
    return { cwd, command: `ruby ${q}`, label: 'Ruby Run' };
  }
  if (ext === 'php') {
    return { cwd, command: `php ${q}`, label: 'PHP Run' };
  }
  if (ext === 'sh' || ext === 'bash' || ext === 'zsh') {
    return { cwd, command: `bash ${q}`, label: 'Shell Run' };
  }
  return null;
};
