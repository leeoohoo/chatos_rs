import { createHash } from 'node:crypto';
import { AtomicJsonDirectory } from '../storage/atomic-json-directory.js';
import { assertGenerationScopeMatches, type GenerationScope } from './generation-plan-schema.js';
import type { SceneDocument } from './scene-schema.js';
import type { SceneTransaction } from './scene-transaction.js';
import {
  softProtectionsFromHumanTransaction,
  type GenerationSoftProtectedField
} from './generation-soft-protection.js';

interface GenerationSoftProtectionRecord {
  formatVersion: 1;
  scope: GenerationScope;
  appliedTransactionIds: string[];
  protections: GenerationSoftProtectedField[];
  updatedAt: string;
}

function fileNameFor(scope: GenerationScope): string {
  const digest = createHash('sha256').update(`${scope.projectId}\0${scope.documentId}`).digest('hex');
  return `generation-soft-protection-${digest}.json`;
}

function assertRecord(record: GenerationSoftProtectionRecord, scope: GenerationScope): void {
  if (!record || record.formatVersion !== 1) throw new Error('Soft protection store record format is invalid.');
  assertGenerationScopeMatches(record.scope, scope);
  if (!Array.isArray(record.appliedTransactionIds) || new Set(record.appliedTransactionIds).size !== record.appliedTransactionIds.length) {
    throw new Error('Soft protection transaction history is invalid.');
  }
  if (!Array.isArray(record.protections)) throw new Error('Soft protection entries are invalid.');
  if (!Number.isFinite(Date.parse(record.updatedAt))) throw new Error('Soft protection timestamp is invalid.');
}

export class GenerationSoftProtectionStore {
  readonly files: AtomicJsonDirectory;

  constructor(rootDirectory: string) {
    this.files = new AtomicJsonDirectory(rootDirectory);
  }

  async read(scope: GenerationScope, document: SceneDocument): Promise<GenerationSoftProtectedField[]> {
    if (document.documentId !== scope.documentId) throw new Error('Soft protection Scene document does not match the active scope.');
    let record: GenerationSoftProtectionRecord;
    try {
      record = await this.files.read<GenerationSoftProtectionRecord>(fileNameFor(scope));
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code === 'ENOENT') return [];
      throw error;
    }
    assertRecord(record, scope);
    const existing = new Set<string>();
    const visit = (node: import('./scene-schema.js').SceneNode): void => {
      existing.add(node.id);
      if ('children' in node) for (const child of node.children) visit(child);
      if ('slots' in node) for (const child of Object.values(node.slots).flat()) visit(child);
    };
    for (const page of document.pages) for (const node of page.children) visit(node);
    return structuredClone(record.protections.filter((protection) => existing.has(protection.nodeId) && protection.protectedAtRevision <= document.revision));
  }

  async recordHumanTransaction(
    scope: GenerationScope,
    transaction: SceneTransaction,
    committedRevision: number,
    reason?: string,
    timestamp = new Date().toISOString()
  ): Promise<GenerationSoftProtectedField[]> {
    if (scope.documentId === '' || transaction.baseRevision < 0) throw new Error('Soft protection scope or transaction is invalid.');
    const incoming = softProtectionsFromHumanTransaction(transaction, committedRevision, reason);
    return this.files.withLock(async () => {
      let record: GenerationSoftProtectionRecord;
      try {
        record = await this.files.read<GenerationSoftProtectionRecord>(fileNameFor(scope));
        assertRecord(record, scope);
      } catch (error) {
        if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error;
        record = { formatVersion: 1, scope: structuredClone(scope), appliedTransactionIds: [], protections: [], updatedAt: timestamp };
      }
      if (record.appliedTransactionIds.includes(transaction.transactionId)) return structuredClone(record.protections);
      const byField = new Map(record.protections.map((protection) => [`${protection.nodeId}\0${protection.path.join('.')}`, protection]));
      for (const protection of incoming) byField.set(`${protection.nodeId}\0${protection.path.join('.')}`, structuredClone(protection));
      record.appliedTransactionIds.push(transaction.transactionId);
      record.protections = [...byField.values()];
      record.updatedAt = timestamp;
      await this.files.write(fileNameFor(scope), record);
      return structuredClone(record.protections);
    });
  }
}
