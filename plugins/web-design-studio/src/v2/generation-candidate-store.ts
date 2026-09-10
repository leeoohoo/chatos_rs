import { createHash } from 'node:crypto';
import { AtomicJsonDirectory } from '../storage/atomic-json-directory.js';
import {
  assertGenerationScopeMatches,
  type GenerationArtifact,
  type GenerationScope
} from './generation-plan-schema.js';
import type { SceneTransaction } from './scene-transaction.js';

export interface GenerationSoftProtectionConflict {
  nodeId: string;
  protectedPath: string[];
  requestedPath: string[];
  protectedAtRevision: number;
  reason: string;
}

export interface GenerationCandidateRecord {
  schemaVersion: 1;
  candidateId: string;
  planId: string;
  scope: GenerationScope;
  pageId: string;
  stepId: string;
  attemptId: string;
  idempotencyKey: string;
  baseRevision: number;
  transaction: SceneTransaction;
  artifacts: GenerationArtifact[];
  protectionConflicts: GenerationSoftProtectionConflict[];
  qualitySummary: string;
  issueIds: string[];
  createdAt: string;
  updatedAt: string;
}

interface GenerationCandidateStoreRecord {
  formatVersion: 1;
  candidate: GenerationCandidateRecord;
}

const identifierPattern = /^[A-Za-z0-9][A-Za-z0-9._:-]{0,159}$/;

function assertIdentifier(value: unknown, label: string): asserts value is string {
  if (typeof value !== 'string' || !identifierPattern.test(value)) throw new Error(`${label} is invalid.`);
}

function assertTimestamp(value: unknown, label: string): asserts value is string {
  if (typeof value !== 'string' || !Number.isFinite(Date.parse(value))) throw new Error(`${label} is invalid.`);
}

function fileNameFor(scope: GenerationScope, attemptId: string): string {
  const digest = createHash('sha256').update(`${scope.projectId}\0${scope.documentId}\0${attemptId}`).digest('hex');
  return `generation-candidate-${digest}.json`;
}

export function assertGenerationCandidate(candidate: GenerationCandidateRecord): void {
  if (!candidate || typeof candidate !== 'object' || candidate.schemaVersion !== 1) throw new Error('Generation candidate is invalid.');
  assertIdentifier(candidate.candidateId, 'candidate.candidateId');
  assertIdentifier(candidate.planId, 'candidate.planId');
  assertIdentifier(candidate.scope.projectId, 'candidate.scope.projectId');
  assertIdentifier(candidate.scope.documentId, 'candidate.scope.documentId');
  assertIdentifier(candidate.pageId, 'candidate.pageId');
  assertIdentifier(candidate.stepId, 'candidate.stepId');
  assertIdentifier(candidate.attemptId, 'candidate.attemptId');
  assertIdentifier(candidate.idempotencyKey, 'candidate.idempotencyKey');
  if (!Number.isSafeInteger(candidate.baseRevision) || candidate.baseRevision < 0) throw new Error('candidate.baseRevision is invalid.');
  if (!candidate.transaction || candidate.transaction.author !== 'ai' || candidate.transaction.baseRevision !== candidate.baseRevision) {
    throw new Error('Generation candidate needs an AI transaction at its base revision.');
  }
  if (!Array.isArray(candidate.artifacts)) throw new Error('candidate.artifacts is invalid.');
  if (!Array.isArray(candidate.protectionConflicts)) throw new Error('candidate.protectionConflicts is invalid.');
  for (const [index, conflict] of candidate.protectionConflicts.entries()) {
    assertIdentifier(conflict.nodeId, `candidate.protectionConflicts[${index}].nodeId`);
    if (!Array.isArray(conflict.protectedPath) || conflict.protectedPath.some((segment) => typeof segment !== 'string' || !segment)) throw new Error('Candidate protected path is invalid.');
    if (!Array.isArray(conflict.requestedPath) || conflict.requestedPath.some((segment) => typeof segment !== 'string' || !segment)) throw new Error('Candidate requested path is invalid.');
    if (!Number.isSafeInteger(conflict.protectedAtRevision) || conflict.protectedAtRevision < 0) throw new Error('Candidate protection revision is invalid.');
    if (!conflict.reason?.trim()) throw new Error('Candidate protection reason is required.');
  }
  if (typeof candidate.qualitySummary !== 'string' || !candidate.qualitySummary.trim()) throw new Error('candidate.qualitySummary is required.');
  if (!Array.isArray(candidate.issueIds) || candidate.issueIds.some((issueId) => typeof issueId !== 'string' || !issueId.trim())) throw new Error('candidate.issueIds is invalid.');
  assertTimestamp(candidate.createdAt, 'candidate.createdAt');
  assertTimestamp(candidate.updatedAt, 'candidate.updatedAt');
}

export class GenerationCandidateStore {
  readonly files: AtomicJsonDirectory;

  constructor(rootDirectory: string) {
    this.files = new AtomicJsonDirectory(rootDirectory);
  }

  async create(source: GenerationCandidateRecord): Promise<GenerationCandidateRecord> {
    assertGenerationCandidate(source);
    return this.files.withLock(async () => {
      const fileName = fileNameFor(source.scope, source.attemptId);
      try {
        const existing = await this.files.read<GenerationCandidateStoreRecord>(fileName);
        assertGenerationCandidate(existing.candidate);
        if (existing.candidate.idempotencyKey === source.idempotencyKey
          && JSON.stringify(existing.candidate.transaction) === JSON.stringify(source.transaction)) {
          return structuredClone(existing.candidate);
        }
        throw new Error(`Generation candidate already exists for attempt ${source.attemptId}.`);
      } catch (error) {
        if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error;
      }
      const candidate = structuredClone(source);
      await this.files.write(fileName, { formatVersion: 1, candidate } satisfies GenerationCandidateStoreRecord);
      return structuredClone(candidate);
    });
  }

  async read(scope: GenerationScope, attemptId: string): Promise<GenerationCandidateRecord> {
    const record = await this.files.read<GenerationCandidateStoreRecord>(fileNameFor(scope, attemptId));
    if (!record || record.formatVersion !== 1) throw new Error('Generation candidate store record format is invalid.');
    assertGenerationCandidate(record.candidate);
    assertGenerationScopeMatches(record.candidate.scope, scope);
    if (record.candidate.attemptId !== attemptId) throw new Error('Generation candidate attempt identity mismatch.');
    return structuredClone(record.candidate);
  }

  async remove(scope: GenerationScope, attemptId: string): Promise<void> {
    await this.files.withLock(() => this.files.remove(fileNameFor(scope, attemptId)));
  }
}
