export function isSafeOoxmlArchiveEntryName(name: string): boolean {
  if (
    !name
    || name.includes('\0')
    || name.includes('\\')
    || name.startsWith('/')
    || /^[A-Za-z]:/.test(name)
  ) {
    return false;
  }

  // ZIP archives may contain explicit directory entries such as `word/`.
  // Validate the directory path itself while allowing its single trailing slash.
  const pathName = name.endsWith('/') ? name.slice(0, -1) : name;
  if (!pathName) return false;
  return !pathName.split('/').some((component) => !component || component === '.' || component === '..');
}
