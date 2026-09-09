import { createHash } from 'node:crypto';
import { AtomicJsonDirectory } from '../storage/atomic-json-directory.js';
import {
  assertGenerationPlan,
  assertGenerationScopeMatches,
  type GenerationPlan,
  type GenerationScope
} from './generation-plan-schema.js';
import { transitionGenerationPlan, type GenerationPlanAction } from './generation-state-machine.js';

interface GenerationPlanStoreRecord {
  formatVersion: 1;
  plan: GenerationPlan;
}

export class GenerationPlanRevisionConflictError extends Error {
  constructor(public readonly actualRevision: number) {
    super(`Generation plan revision conflict. Current revision is ${actualRevision}.`);
  }
}

function fileNameFor(scope: GenerationScope): string {
  const digest = createHash('sha256').update(`${scope.projectId}\0${scope.documentId}`).digest('hex');
  return `generation-plan-${digest}.json`;
}

export class GenerationPlanStore {
  readonly files: AtomicJsonDirectory;

  constructor(rootDirectory: string) {
    this.files = new AtomicJsonDirectory(rootDirectory);
  }

  async create(source: GenerationPlan): Promise<GenerationPlan> {
    assertGenerationPlan(source);
    if (source.revision !== 0) throw new Error('A new generation plan must start at revision 0.');
    return this.files.withLock(async () => {
      const fileName = fileNameFor(source.scope);
      try {
        await this.files.read<GenerationPlanStoreRecord>(fileName);
        throw new Error(`Generation plan already exists for document ${source.scope.documentId}.`);
      } catch (error) {
        if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error;
      }
      const plan = structuredClone(source);
      plan.revision = 1;
      plan.updatedAt = new Date().toISOString();
      assertGenerationPlan(plan);
      await this.files.write(fileName, { formatVersion: 1, plan } satisfies GenerationPlanStoreRecord);
      return structuredClone(plan);
    });
  }

  async read(scope: GenerationScope): Promise<GenerationPlan> {
    const record = await this.readRecord(scope);
    return structuredClone(record.plan);
  }

  async apply(
    scope: GenerationScope,
    expectedRevision: number,
    action: GenerationPlanAction,
    timestamp = new Date().toISOString()
  ): Promise<{ plan: GenerationPlan; replayed: boolean }> {
    return this.files.withLock(async () => {
      const record = await this.readRecord(scope);
      if (record.plan.revision !== expectedRevision) throw new GenerationPlanRevisionConflictError(record.plan.revision);
      const transition = transitionGenerationPlan(record.plan, action, timestamp);
      if (!transition.changed) return { plan: structuredClone(transition.plan), replayed: transition.replayed };
      const plan = structuredClone(transition.plan);
      plan.revision = record.plan.revision + 1;
      assertGenerationPlan(plan);
      await this.files.write(fileNameFor(scope), { formatVersion: 1, plan } satisfies GenerationPlanStoreRecord);
      return { plan: structuredClone(plan), replayed: false };
    });
  }

  async remove(scope: GenerationScope): Promise<void> {
    await this.files.withLock(() => this.files.remove(fileNameFor(scope)));
  }

  private async readRecord(scope: GenerationScope): Promise<GenerationPlanStoreRecord> {
    const record = await this.files.read<GenerationPlanStoreRecord>(fileNameFor(scope));
    if (!record || record.formatVersion !== 1) throw new Error('Generation plan store record format is invalid.');
    assertGenerationPlan(record.plan);
    assertGenerationScopeMatches(record.plan.scope, scope);
    return record;
  }
}
