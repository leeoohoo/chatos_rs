import { createHash, randomUUID } from 'node:crypto';
import { promises as fs } from 'node:fs';
import path from 'node:path';
import { AtomicJsonDirectory } from '../storage/atomic-json-directory.js';
import { assertGenerationScopeMatches, type GenerationArtifact, type GenerationScope } from './generation-plan-schema.js';
import type { SceneLayoutCalibrationReport } from './layout-calibration.js';
import type { SceneLayoutDiagnostic } from './layout-engine.js';
import type { SceneRect } from './scene-schema.js';

export interface VisualGroundingEntry {
  nodeId: string;
  parentId: string;
  pageId: string;
  rect: SceneRect;
  depth: number;
}

export interface GenerationVisualArtifactRecord {
  schemaVersion: 1;
  artifact: GenerationArtifact;
  scope: GenerationScope;
  pageId: string;
  rootNodeId: string;
  width: number;
  height: number;
  snapshotArtifactId?: string;
  crop?: SceneRect;
  grounding?: VisualGroundingEntry[];
  calibration?: SceneLayoutCalibrationReport;
  diagnostics?: SceneLayoutDiagnostic[];
  imageFileName?: string;
  mimeType?: 'image/png';
  createdAt: string;
}

function digestFor(scope: GenerationScope, artifactId: string): string {
  return createHash('sha256').update(`${scope.projectId}\0${scope.documentId}\0${artifactId}`).digest('hex');
}

function recordFileName(scope: GenerationScope, artifactId: string): string {
  return `visual-artifact-${digestFor(scope, artifactId)}.json`;
}

function imageFileName(scope: GenerationScope, artifactId: string): string {
  return `visual-artifact-${digestFor(scope, artifactId)}.png`;
}

function assertRecord(record: GenerationVisualArtifactRecord): void {
  if (!record || record.schemaVersion !== 1) throw new Error('Visual artifact record is invalid.');
  assertGenerationScopeMatches(record.scope, record.scope);
  if (record.artifact.revision < 0 || !Number.isSafeInteger(record.artifact.revision)) throw new Error('Visual artifact revision is invalid.');
  if (!record.artifact.artifactId?.trim() || !record.pageId?.trim() || !record.rootNodeId?.trim()) throw new Error('Visual artifact identity is invalid.');
  if (!Number.isFinite(record.width) || record.width <= 0 || !Number.isFinite(record.height) || record.height <= 0) throw new Error('Visual artifact dimensions are invalid.');
  if (!Number.isFinite(Date.parse(record.createdAt)) || record.artifact.createdAt !== record.createdAt) throw new Error('Visual artifact timestamp is invalid.');
  if (record.imageFileName !== undefined && !/^visual-artifact-[a-f0-9]{64}\.png$/.test(record.imageFileName)) throw new Error('Visual artifact image file is invalid.');
  if ((record.imageFileName === undefined) !== (record.mimeType === undefined)) throw new Error('Visual artifact image metadata is incomplete.');
}

async function atomicWriteBinary(directory: string, fileName: string, contents: Buffer): Promise<void> {
  const destination = path.join(directory, fileName);
  const temporary = path.join(directory, `.${fileName}.${process.pid}.${randomUUID()}.tmp`);
  const handle = await fs.open(temporary, 'wx', 0o600);
  try {
    await handle.writeFile(contents);
    await handle.sync();
  } finally {
    await handle.close();
  }
  try { await fs.rename(temporary, destination); }
  catch (error) {
    await fs.unlink(temporary).catch(() => undefined);
    throw error;
  }
}

export class GenerationVisualArtifactStore {
  readonly files: AtomicJsonDirectory;

  constructor(rootDirectory: string) {
    this.files = new AtomicJsonDirectory(path.join(rootDirectory, 'visual-artifacts-v3'));
  }

  async create(source: Omit<GenerationVisualArtifactRecord, 'imageFileName' | 'mimeType'>, image?: Buffer): Promise<GenerationVisualArtifactRecord> {
    return this.files.withLock(async () => {
      const fileName = recordFileName(source.scope, source.artifact.artifactId);
      try {
        const existing = await this.files.read<GenerationVisualArtifactRecord>(fileName);
        assertRecord(existing);
        throw new Error(`Visual artifact already exists: ${source.artifact.artifactId}`);
      } catch (error) {
        if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error;
      }
      const record: GenerationVisualArtifactRecord = {
        ...structuredClone(source),
        ...(image ? { imageFileName: imageFileName(source.scope, source.artifact.artifactId), mimeType: 'image/png' as const } : {})
      };
      assertRecord(record);
      if (image) await atomicWriteBinary(this.files.rootDirectory, record.imageFileName!, image);
      try {
        await this.files.write(fileName, record);
      } catch (error) {
        if (record.imageFileName) await fs.unlink(path.join(this.files.rootDirectory, record.imageFileName)).catch(() => undefined);
        throw error;
      }
      return structuredClone(record);
    });
  }

  async read(scope: GenerationScope, artifactId: string): Promise<GenerationVisualArtifactRecord> {
    const record = await this.files.read<GenerationVisualArtifactRecord>(recordFileName(scope, artifactId));
    assertRecord(record);
    assertGenerationScopeMatches(record.scope, scope);
    if (record.artifact.artifactId !== artifactId) throw new Error('Visual artifact identity does not match its storage key.');
    return structuredClone(record);
  }

  async readImage(scope: GenerationScope, artifactId: string): Promise<{ record: GenerationVisualArtifactRecord; data: Buffer; mimeType: 'image/png' }> {
    const record = await this.read(scope, artifactId);
    if (!record.imageFileName || record.mimeType !== 'image/png') throw new Error(`Visual artifact ${artifactId} does not contain an image.`);
    const data = await fs.readFile(path.join(this.files.rootDirectory, record.imageFileName));
    const expected = record.artifact.sha256;
    if (expected && createHash('sha256').update(data).digest('hex') !== expected) throw new Error(`Visual artifact ${artifactId} checksum mismatch.`);
    return { record, data, mimeType: 'image/png' };
  }
}
