import { createHash } from 'node:crypto';
import { gzipSync, gunzipSync } from 'node:zlib';
import { AtomicJsonDirectory } from '../storage/atomic-json-directory.js';
import { assertSceneDocument, type SceneCreator, type SceneDocument } from './scene-schema.js';
import { applySceneTransaction, type SceneTransaction, type SceneTransactionSummary } from './scene-transaction.js';

export class SceneRevisionConflictError extends Error {
  constructor(public readonly actualRevision: number) {
    super(`Scene revision conflict. Current revision is ${actualRevision}.`);
  }
}

interface CompressedSceneSnapshot {
  encoding: 'gzip-base64';
  sha256: string;
  data: string;
}

interface SceneHistoryEntry {
  transaction: SceneTransaction;
  summary: SceneTransactionSummary;
  before: CompressedSceneSnapshot;
  after: CompressedSceneSnapshot;
}

interface SceneStoreRecord {
  formatVersion: 2;
  document: SceneDocument;
  past: SceneHistoryEntry[];
  future: SceneHistoryEntry[];
}

export interface SceneHistoryStatus {
  undoCount: number;
  redoCount: number;
  nextUndoTransactionId?: string;
  nextRedoTransactionId?: string;
}

function fileNameFor(documentId: string): string {
  const digest = createHash('sha256').update(documentId).digest('hex');
  return `scene-v2-${digest}.json`;
}

function restoredSnapshot(snapshot: SceneDocument, current: SceneDocument, timestamp: string): SceneDocument {
  const restored = structuredClone(snapshot);
  restored.revision = current.revision + 1;
  restored.updatedAt = timestamp;
  assertSceneDocument(restored);
  return restored;
}

function compressSnapshot(document: SceneDocument): CompressedSceneSnapshot {
  const source = Buffer.from(JSON.stringify(document), 'utf8');
  return {
    encoding: 'gzip-base64',
    sha256: createHash('sha256').update(source).digest('hex'),
    data: gzipSync(source, { level: 6 }).toString('base64')
  };
}

function decompressSnapshot(snapshot: CompressedSceneSnapshot): SceneDocument {
  if (!snapshot || snapshot.encoding !== 'gzip-base64' || typeof snapshot.sha256 !== 'string' || typeof snapshot.data !== 'string') {
    throw new Error('Scene history snapshot is invalid.');
  }
  let source: Buffer;
  try {
    source = gunzipSync(Buffer.from(snapshot.data, 'base64'));
  } catch {
    throw new Error('Scene history snapshot cannot be decompressed.');
  }
  if (createHash('sha256').update(source).digest('hex') !== snapshot.sha256) throw new Error('Scene history snapshot checksum mismatch.');
  const document = JSON.parse(source.toString('utf8')) as SceneDocument;
  assertSceneDocument(document);
  return document;
}

function historyEntryBytes(entry: SceneHistoryEntry): number {
  return Buffer.byteLength(entry.before.data, 'utf8') + Buffer.byteLength(entry.after.data, 'utf8')
    + Buffer.byteLength(JSON.stringify(entry.transaction), 'utf8') + Buffer.byteLength(JSON.stringify(entry.summary), 'utf8');
}

function trimHistory(
  pastSource: SceneHistoryEntry[],
  futureSource: SceneHistoryEntry[],
  historyLimit: number,
  historyByteLimit: number
): { past: SceneHistoryEntry[]; future: SceneHistoryEntry[] } {
  const past = pastSource.slice();
  const future = futureSource.slice();
  while (past.length + future.length > historyLimit) {
    if (past.length >= future.length && past.length > 0) past.shift();
    else future.shift();
  }
  let storedBytes = [...past, ...future].reduce((total, entry) => total + historyEntryBytes(entry), 0);
  while (storedBytes > historyByteLimit && past.length + future.length > 1) {
    const pastCandidate = past[0];
    const futureCandidate = future[0];
    if (pastCandidate && (!futureCandidate || historyEntryBytes(pastCandidate) >= historyEntryBytes(futureCandidate))) {
      storedBytes -= historyEntryBytes(past.shift()!);
    } else if (futureCandidate) {
      storedBytes -= historyEntryBytes(future.shift()!);
    }
  }
  return { past, future };
}

export class SceneDocumentStore {
  readonly files: AtomicJsonDirectory;

  constructor(rootDirectory: string, public readonly historyLimit = 60, public readonly historyByteLimit = 64 * 1024 * 1024) {
    if (!Number.isSafeInteger(historyLimit) || historyLimit < 1 || historyLimit > 500) throw new Error('Scene history limit is invalid.');
    if (!Number.isSafeInteger(historyByteLimit) || historyByteLimit < 1024 || historyByteLimit > 1024 * 1024 * 1024) throw new Error('Scene history byte limit is invalid.');
    this.files = new AtomicJsonDirectory(rootDirectory);
  }

  async create(source: SceneDocument): Promise<SceneDocument> {
    assertSceneDocument(source);
    if (source.revision !== 0) throw new Error('A new scene document must start at revision 0.');
    return this.files.withLock(async () => {
      const fileName = fileNameFor(source.documentId);
      try {
        await this.files.read<SceneStoreRecord>(fileName);
        throw new Error(`Scene document already exists: ${source.documentId}`);
      } catch (error) {
        if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error;
      }
      const document = structuredClone(source);
      document.revision = 1;
      document.updatedAt = new Date().toISOString();
      assertSceneDocument(document);
      await this.files.write(fileName, { formatVersion: 2, document, past: [], future: [] } satisfies SceneStoreRecord);
      return structuredClone(document);
    });
  }

  async read(documentId: string): Promise<SceneDocument> {
    return structuredClone((await this.readRecord(documentId)).document);
  }

  async apply(documentId: string, transaction: SceneTransaction): Promise<{ document: SceneDocument; summary: SceneTransactionSummary }> {
    return this.files.withLock(async () => {
      const record = await this.readRecord(documentId);
      if (transaction.baseRevision !== record.document.revision) throw new SceneRevisionConflictError(record.document.revision);
      const before = structuredClone(record.document);
      const result = applySceneTransaction(record.document, transaction);
      const entry: SceneHistoryEntry = {
        transaction: structuredClone(transaction),
        summary: structuredClone(result.summary),
        before: compressSnapshot(before),
        after: compressSnapshot(result.document)
      };
      const history = trimHistory([...record.past, entry], [], this.historyLimit, this.historyByteLimit);
      const next: SceneStoreRecord = {
        formatVersion: 2,
        document: result.document,
        ...history
      };
      await this.files.write(fileNameFor(documentId), next);
      return { document: structuredClone(result.document), summary: structuredClone(result.summary) };
    });
  }

  async undo(documentId: string, expectedRevision: number, _author: SceneCreator = 'human'): Promise<SceneDocument> {
    return this.files.withLock(async () => {
      const record = await this.readRecord(documentId);
      if (record.document.revision !== expectedRevision) throw new SceneRevisionConflictError(record.document.revision);
      const entry = record.past.at(-1);
      if (!entry) throw new Error('Scene history has nothing to undo.');
      const document = restoredSnapshot(decompressSnapshot(entry.before), record.document, new Date().toISOString());
      const history = trimHistory(record.past.slice(0, -1), [...record.future, entry], this.historyLimit, this.historyByteLimit);
      await this.files.write(fileNameFor(documentId), {
        formatVersion: 2,
        document,
        ...history
      } satisfies SceneStoreRecord);
      return structuredClone(document);
    });
  }

  async redo(documentId: string, expectedRevision: number, _author: SceneCreator = 'human'): Promise<SceneDocument> {
    return this.files.withLock(async () => {
      const record = await this.readRecord(documentId);
      if (record.document.revision !== expectedRevision) throw new SceneRevisionConflictError(record.document.revision);
      const entry = record.future.at(-1);
      if (!entry) throw new Error('Scene history has nothing to redo.');
      const document = restoredSnapshot(decompressSnapshot(entry.after), record.document, new Date().toISOString());
      const history = trimHistory([...record.past, entry], record.future.slice(0, -1), this.historyLimit, this.historyByteLimit);
      await this.files.write(fileNameFor(documentId), {
        formatVersion: 2,
        document,
        ...history
      } satisfies SceneStoreRecord);
      return structuredClone(document);
    });
  }

  async history(documentId: string): Promise<SceneHistoryStatus> {
    const record = await this.readRecord(documentId);
    return {
      undoCount: record.past.length,
      redoCount: record.future.length,
      nextUndoTransactionId: record.past.at(-1)?.transaction.transactionId,
      nextRedoTransactionId: record.future.at(-1)?.transaction.transactionId
    };
  }

  async remove(documentId: string): Promise<void> {
    await this.files.withLock(() => this.files.remove(fileNameFor(documentId)));
  }

  private async readRecord(documentId: string): Promise<SceneStoreRecord> {
    const record = await this.files.read<SceneStoreRecord>(fileNameFor(documentId));
    if (record.formatVersion !== 2) throw new Error('Scene store record format is invalid.');
    assertSceneDocument(record.document);
    if (record.document.documentId !== documentId) throw new Error('Scene store document identity mismatch.');
    if (!Array.isArray(record.past) || !Array.isArray(record.future)) throw new Error('Scene store history is invalid.');
    for (const entry of [...record.past, ...record.future]) {
      if (!entry || typeof entry !== 'object' || !entry.transaction || !entry.summary) throw new Error('Scene store history entry is invalid.');
      const before = decompressSnapshot(entry.before);
      const after = decompressSnapshot(entry.after);
      if (before.documentId !== documentId || after.documentId !== documentId) throw new Error('Scene store history identity mismatch.');
      if (entry.summary.transactionId !== entry.transaction.transactionId) throw new Error('Scene store history transaction identity mismatch.');
    }
    return record;
  }
}
