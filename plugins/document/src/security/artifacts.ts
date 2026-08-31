import { constants as fsConstants } from 'node:fs';
import { lstat, realpath } from 'node:fs/promises';
import path from 'node:path';
import { randomUUID } from 'node:crypto';
import { DocumentError } from '../errors.js';

const SAFE_OUTPUT_NAME = /^[\p{L}\p{N}][\p{L}\p{N}._ ()-]{0,199}$/u;

export interface ArtifactPaths {
  root: string;
  outputPath: string;
  outputName: string;
  temporaryPath: string;
}

export async function resolveArtifactPaths(outputName: string, extension: string): Promise<ArtifactPaths> {
  const configuredRoot = process.env.CHATOS_PLUGIN_ARTIFACT_DIR?.trim();
  if (!configuredRoot) {
    throw new DocumentError(
      'ARTIFACT_NOT_CONFIGURED',
      'CHATOS_PLUGIN_ARTIFACT_DIR is required for document creation and editing.'
    );
  }
  const rootMetadata = await lstat(configuredRoot).catch(() => undefined);
  if (!rootMetadata?.isDirectory() || rootMetadata.isSymbolicLink()) {
    throw new DocumentError('ARTIFACT_NOT_CONFIGURED', 'The artifact directory must be an existing non-symlink directory.');
  }
  if (!SAFE_OUTPUT_NAME.test(outputName) || path.basename(outputName) !== outputName) {
    throw new DocumentError('INVALID_PATH', 'outputName must be a safe file name without directory components.');
  }
  if (path.extname(outputName).toLowerCase() !== extension) {
    throw new DocumentError('INVALID_ARGUMENT', `outputName must end with ${extension}.`);
  }
  const root = await realpath(configuredRoot);
  const outputPath = path.join(root, outputName);
  const existing = await lstat(outputPath).catch((error: NodeJS.ErrnoException) => {
    if (error.code === 'ENOENT') return undefined;
    throw error;
  });
  if (existing) throw new DocumentError('OUTPUT_EXISTS', 'An artifact with the requested outputName already exists.');
  return {
    root,
    outputPath,
    outputName,
    temporaryPath: path.join(root, `.document-mcp-${randomUUID()}${extension}`)
  };
}

export const EXCLUSIVE_COPY_FLAG = fsConstants.COPYFILE_EXCL;
