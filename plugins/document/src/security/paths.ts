import { lstat, realpath } from 'node:fs/promises';
import path from 'node:path';
import { DocumentError } from '../errors.js';

const WINDOWS_ABSOLUTE = /^[A-Za-z]:[\\/]|^\\\\/;

function relativeComponents(input: string): string[] {
  if (!input || input.includes('\0')) {
    throw new DocumentError('INVALID_PATH', 'A non-empty relative workspace path is required.');
  }
  if (path.isAbsolute(input) || path.posix.isAbsolute(input) || path.win32.isAbsolute(input) || WINDOWS_ABSOLUTE.test(input)) {
    throw new DocumentError('INVALID_PATH', 'Absolute paths are not allowed.');
  }
  const components = input.split(/[\\/]+/);
  if (components.some((component) => !component || component === '.' || component === '..')) {
    throw new DocumentError('INVALID_PATH', 'Path traversal and empty path components are not allowed.');
  }
  return components;
}

function isWithin(root: string, target: string): boolean {
  const relative = path.relative(root, target);
  return relative === '' || (!relative.startsWith(`..${path.sep}`) && relative !== '..' && !path.isAbsolute(relative));
}

export interface ResolvedWorkspaceFile {
  absolutePath: string;
  relativePath: string;
  size: number;
}

async function resolveFileBelowRoot(
  configuredRoot: string,
  components: string[],
  invalidRoot: DocumentError
): Promise<ResolvedWorkspaceFile> {
  const rootMetadata = await lstat(configuredRoot).catch(() => undefined);
  if (!rootMetadata?.isDirectory() || rootMetadata.isSymbolicLink()) {
    throw invalidRoot;
  }
  const canonicalRoot = await realpath(configuredRoot);
  let current = canonicalRoot;
  for (const component of components) {
    current = path.join(current, component);
    const metadata = await lstat(current).catch((error: NodeJS.ErrnoException) => {
      if (error.code === 'ENOENT') {
        throw new DocumentError('FILE_NOT_FOUND', 'The requested workspace file does not exist.');
      }
      throw error;
    });
    if (metadata.isSymbolicLink()) {
      throw new DocumentError('INVALID_PATH', 'Symbolic links are not allowed in document paths.');
    }
  }

  const canonicalTarget = await realpath(current);
  if (!isWithin(canonicalRoot, canonicalTarget)) {
    throw new DocumentError('INVALID_PATH', 'The requested file is outside the bound workspace.');
  }
  const metadata = await lstat(canonicalTarget);
  if (!metadata.isFile() || metadata.isSymbolicLink()) {
    throw new DocumentError('INVALID_PATH', 'The requested path must resolve to a regular file.');
  }
  return {
    absolutePath: canonicalTarget,
    relativePath: components.join('/'),
    size: metadata.size
  };
}

export async function resolveWorkspaceFile(relativePath: string): Promise<ResolvedWorkspaceFile> {
  const configuredRoot = process.env.CHATOS_WORKSPACE?.trim();
  if (!configuredRoot) {
    throw new DocumentError(
      'WORKSPACE_NOT_CONFIGURED',
      'CHATOS_WORKSPACE is required to access project documents.'
    );
  }
  const components = relativeComponents(relativePath);
  try {
    return await resolveFileBelowRoot(
      configuredRoot,
      components,
      new DocumentError('WORKSPACE_NOT_CONFIGURED', 'CHATOS_WORKSPACE must be an existing non-symlink directory.')
    );
  } catch (error) {
    if (!(error instanceof DocumentError) || error.code !== 'FILE_NOT_FOUND') throw error;
  }

  const artifactRoot = process.env.CHATOS_PLUGIN_ARTIFACT_DIR?.trim();
  if (artifactRoot) {
    try {
      return await resolveFileBelowRoot(
        artifactRoot,
        components,
        new DocumentError('ARTIFACT_NOT_CONFIGURED', 'The artifact directory must be an existing non-symlink directory.')
      );
    } catch (error) {
      if (!(error instanceof DocumentError) || error.code !== 'FILE_NOT_FOUND') throw error;
    }
  }
  throw new DocumentError('FILE_NOT_FOUND', 'The requested workspace file or current-session artifact does not exist.');
}
