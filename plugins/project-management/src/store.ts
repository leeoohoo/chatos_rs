import { DatabaseSync } from 'node:sqlite';
import { createHash, randomUUID } from 'node:crypto';
import { chmodSync, lstatSync, mkdirSync } from 'node:fs';
import path from 'node:path';
import { readContext, type ProjectContext } from './context.js';
import {
  assert,
  assertAcyclic,
  commandSchema,
  DomainError,
  type Command,
  type Dependency,
  type DocumentRequirementLink,
  type DocumentVersion,
  type ExecutionIntent,
  type ExecutionReference,
  type Plan,
  type PlanningDocument,
  type Requirement,
  type ScopeGraph,
  type WorkItem
} from './domain.js';

export { readContext, commandSchema, DomainError };

function canonical(value: unknown): string {
  if (Array.isArray(value)) return `[${value.map(canonical).join(',')}]`;
  if (value && typeof value === 'object') {
    return `{${Object.entries(value)
      .sort(([left], [right]) => left.localeCompare(right))
      .map(([key, item]) => `${JSON.stringify(key)}:${canonical(item)}`)
      .join(',')}}`;
  }
  return JSON.stringify(value);
}

function rejectSymlink(file: string) {
  try {
    assert(!lstatSync(file).isSymbolicLink(), 'Plugin storage must not be a symbolic link');
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error;
  }
}

function sha256(value: string): string {
  return createHash('sha256').update(value).digest('hex');
}

type EntityTable = 'requirements' | 'work_items' | 'documents' | 'plans' | 'execution_intents' | 'execution_references';

/** Host-isolated business data only: no ProjectRegistry, Git client, HTTP token or execution status. */
export class PlanningStore {
  private db: DatabaseSync;
  readonly context: ProjectContext;

  constructor(env: NodeJS.ProcessEnv = process.env) {
    this.context = readContext(env);
    const directory = this.context.dataDir;
    rejectSymlink(directory);
    mkdirSync(directory, { recursive: true, mode: 0o700 });
    const file = path.join(directory, 'planning.sqlite3');
    for (const suffix of ['', '-wal', '-shm']) rejectSymlink(file + suffix);
    this.db = new DatabaseSync(file);
    try {
      chmodSync(file, 0o600);
      this.db.exec('PRAGMA busy_timeout=5000; PRAGMA foreign_keys=ON;');
      const version = Number(this.db.prepare('PRAGMA user_version').get()!.user_version);
      assert(version === 0 || version === 2, 'Unsupported planning database version; explicit import is required');
      this.db.exec('PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; BEGIN IMMEDIATE;');
      if (version === 0) {
        assert(!this.db.prepare("SELECT name FROM sqlite_master WHERE type='table'").get(), 'Unrecognized database; explicit import is required');
        this.db.exec(`
          CREATE TABLE scope_binding(
            singleton INTEGER PRIMARY KEY CHECK(singleton=1),
            identity TEXT NOT NULL,
            revision INTEGER NOT NULL
          );
          CREATE TABLE requirements(id TEXT PRIMARY KEY, body TEXT NOT NULL);
          CREATE TABLE work_items(id TEXT PRIMARY KEY, body TEXT NOT NULL);
          CREATE TABLE documents(id TEXT PRIMARY KEY, body TEXT NOT NULL);
          CREATE TABLE document_versions(
            id TEXT PRIMARY KEY,
            document_id TEXT NOT NULL REFERENCES documents(id),
            version INTEGER NOT NULL,
            body TEXT NOT NULL,
            UNIQUE(document_id, version)
          );
          CREATE TABLE document_requirement_links(
            document_id TEXT NOT NULL REFERENCES documents(id),
            requirement_id TEXT NOT NULL REFERENCES requirements(id),
            PRIMARY KEY(document_id, requirement_id)
          );
          CREATE TABLE dependencies(
            kind TEXT NOT NULL CHECK(kind IN ('requirement','work_item')),
            id TEXT NOT NULL,
            prerequisite_id TEXT NOT NULL,
            PRIMARY KEY(kind,id,prerequisite_id)
          );
          CREATE TABLE plans(id TEXT PRIMARY KEY, body TEXT NOT NULL);
          CREATE TABLE execution_intents(id TEXT PRIMARY KEY, body TEXT NOT NULL);
          CREATE TABLE execution_references(id TEXT PRIMARY KEY, body TEXT NOT NULL);
          CREATE TABLE mutation_receipts(
            request_id TEXT PRIMARY KEY,
            digest TEXT NOT NULL,
            response TEXT NOT NULL
          );
          PRAGMA user_version=2;
        `);
        this.db.prepare('INSERT INTO scope_binding VALUES(1,?,0)').run(this.identity());
      }
      assert(
        this.db.prepare('SELECT identity FROM scope_binding WHERE singleton=1').get()?.identity === this.identity(),
        'Plugin data belongs to another bound context'
      );
      this.db.exec('COMMIT;');
      assert(this.db.prepare('PRAGMA quick_check').get()?.quick_check === 'ok', 'Planning database failed integrity check');
    } catch (error) {
      try { this.db.exec('ROLLBACK'); } catch {}
      this.db.close();
      throw error;
    }
  }

  private identity() {
    return canonical({ projectId: this.context.projectId, scopeId: this.context.scopeId });
  }

  close() { this.db.close(); }

  private revision(): number {
    return Number(this.db.prepare('SELECT revision FROM scope_binding WHERE singleton=1').get()!.revision);
  }

  private entities<T>(table: EntityTable): T[] {
    return this.db.prepare(`SELECT body FROM ${table} ORDER BY rowid`).all().map(row => JSON.parse(String(row.body)) as T);
  }

  private save<T extends { id: string }>(table: EntityTable, value: T) {
    this.db.prepare(`INSERT INTO ${table}(id,body) VALUES(?,?) ON CONFLICT(id) DO UPDATE SET body=excluded.body`)
      .run(value.id, JSON.stringify(value));
  }

  private state() {
    return {
      revision: this.revision(),
      requirements: this.entities<Requirement>('requirements'),
      workItems: this.entities<WorkItem>('work_items'),
      documents: this.entities<PlanningDocument>('documents'),
      documentVersions: this.db.prepare('SELECT body FROM document_versions ORDER BY document_id,version').all()
        .map(row => JSON.parse(String(row.body)) as DocumentVersion),
      documentLinks: this.db.prepare(
        'SELECT document_id AS documentId, requirement_id AS requirementId FROM document_requirement_links ORDER BY document_id,requirement_id'
      ).all() as DocumentRequirementLink[],
      dependencies: this.db.prepare(
        'SELECT kind,id,prerequisite_id AS prerequisiteId FROM dependencies ORDER BY kind,id,prerequisite_id'
      ).all() as Dependency[],
      plans: this.entities<Plan>('plans'),
      executionIntents: this.entities<ExecutionIntent>('execution_intents'),
      executionReferences: this.entities<ExecutionReference>('execution_references')
    };
  }

  read() {
    this.db.exec('BEGIN');
    try {
      const state = this.state();
      this.db.exec('COMMIT');
      return state;
    } catch (error) {
      this.db.exec('ROLLBACK');
      throw error;
    }
  }

  getDocument(documentId: string) {
    const state = this.read();
    const document = state.documents.find(value => value.id === documentId);
    if (!document) return undefined;
    return {
      ...document,
      requirementIds: state.documentLinks.filter(value => value.documentId === documentId).map(value => value.requirementId),
      versions: state.documentVersions.filter(value => value.documentId === documentId).sort((left, right) => right.version - left.version)
    };
  }

  scope(kind: 'requirement' | 'work_item', rootId: string): ScopeGraph {
    const state = this.read();
    const requirements = new Map(state.requirements.map(value => [value.id, value]));
    const workItems = new Map(state.workItems.map(value => [value.id, value]));
    const source = kind === 'requirement' ? requirements : workItems;
    assert(source.has(rootId), 'Planning item not found in the bound project');

    const dependencies = state.dependencies.filter(value => value.kind === kind);
    const walk = (start: string[], next: (id: string) => string[]) => {
      const seen = new Set<string>();
      const queue = [...start];
      while (queue.length) {
        const current = queue.shift()!;
        for (const value of next(current)) {
          if (value !== rootId && !seen.has(value)) { seen.add(value); queue.push(value); }
        }
      }
      return [...seen];
    };
    const prerequisites = walk([rootId], current => dependencies.filter(value => value.id === current).map(value => value.prerequisiteId));
    const dependants = walk([rootId], current => dependencies.filter(value => value.prerequisiteId === current).map(value => value.id));
    const ancestors = kind === 'requirement'
      ? walk([rootId], current => requirements.get(current)?.parentId ? [requirements.get(current)!.parentId!] : [])
      : [];
    const descendants = kind === 'requirement'
      ? walk([rootId], current => state.requirements.filter(value => value.parentId === current).map(value => value.id))
      : [];

    const requirementIds = new Set<string>();
    const workItemIds = new Set<string>();
    if (kind === 'requirement') {
      [rootId, ...descendants, ...prerequisites].forEach(value => requirementIds.add(value));
      state.workItems.filter(value => requirementIds.has(value.requirementId)).forEach(value => workItemIds.add(value.id));
    } else {
      [rootId, ...prerequisites].forEach(value => workItemIds.add(value));
      workItemIds.forEach(value => {
        const requirementId = workItems.get(value)?.requirementId;
        if (requirementId) requirementIds.add(requirementId);
      });
    }
    const documentIds = new Set(
      state.documentLinks.filter(value => requirementIds.has(value.requirementId)).map(value => value.documentId)
    );
    const nodes: ScopeGraph['nodes'] = [];
    for (const requirementId of requirementIds) {
      const item = requirements.get(requirementId);
      if (item) nodes.push({ id: item.id, kind: 'requirement', title: item.title, relation: item.id === rootId ? 'root' : 'scope' });
    }
    for (const workItemId of workItemIds) {
      const item = workItems.get(workItemId);
      if (item) nodes.push({ id: item.id, kind: 'work_item', title: item.title, relation: item.id === rootId ? 'root' : 'scope' });
    }
    const nodeIds = new Set(nodes.map(value => value.id));
    const edges: ScopeGraph['edges'] = [];
    for (const item of state.requirements) {
      if (item.parentId && nodeIds.has(item.id) && nodeIds.has(item.parentId)) edges.push({ from: item.parentId, to: item.id, relation: 'contains' });
    }
    for (const item of state.workItems) {
      if (nodeIds.has(item.id) && nodeIds.has(item.requirementId)) edges.push({ from: item.requirementId, to: item.id, relation: 'implements' });
    }
    for (const edge of state.dependencies) {
      if (nodeIds.has(edge.id) && nodeIds.has(edge.prerequisiteId)) edges.push({ from: edge.prerequisiteId, to: edge.id, relation: 'requires' });
    }
    return {
      kind,
      rootId,
      ancestors,
      descendants,
      prerequisites,
      dependants,
      requirementIds: [...requirementIds],
      workItemIds: [...workItemIds],
      documentIds: [...documentIds],
      nodes,
      edges
    };
  }

  mutate(raw: unknown): { revision: number; result: { id: string } } {
    const command = commandSchema.parse(raw);
    const digest = sha256(canonical(command));
    this.db.exec('BEGIN IMMEDIATE');
    try {
      const receipt = this.db.prepare('SELECT digest,response FROM mutation_receipts WHERE request_id=?').get(command.requestId);
      if (receipt) {
        assert(receipt.digest === digest, 'requestId was already used for different content');
        this.db.exec('COMMIT');
        return JSON.parse(String(receipt.response));
      }
      if (this.revision() !== command.expectedRevision) {
        throw new DomainError('revision_conflict', 'Planning data changed; reload before editing');
      }
      const result = this.apply(command);
      assert(Buffer.byteLength(JSON.stringify(this.state())) <= 8 * 1024 * 1024, 'Planning data exceeds the 8 MiB scope limit');
      const response = { revision: command.expectedRevision + 1, result };
      this.db.prepare('UPDATE scope_binding SET revision=? WHERE singleton=1').run(response.revision);
      this.db.prepare('INSERT INTO mutation_receipts VALUES(?,?,?)').run(command.requestId, digest, JSON.stringify(response));
      this.db.exec('COMMIT');
      return response;
    } catch (error) {
      this.db.exec('ROLLBACK');
      throw error;
    }
  }

  private apply(command: Command): { id: string } {
    const state = this.state();
    const requirement = (id: string) => {
      const item = state.requirements.find(value => value.id === id);
      assert(item, 'Requirement not found in the bound project');
      return item;
    };
    const workItem = (id: string) => {
      const item = state.workItems.find(value => value.id === id);
      assert(item, 'Work item not found in the bound project');
      return item;
    };
    const document = (id: string) => {
      const item = state.documents.find(value => value.id === id);
      assert(item, 'Document not found in the bound project');
      return item;
    };
    const currentDocumentContent = (id: string) => state.documentVersions
      .filter(value => value.documentId === id)
      .sort((left, right) => right.version - left.version)[0]?.content ?? '';
    const linkedPublishedDocuments = (requirementId: string) => state.documentLinks
      .filter(value => value.requirementId === requirementId)
      .map(value => state.documents.find(document => document.id === value.documentId))
      .filter((value): value is PlanningDocument => Boolean(value && value.status === 'published' && value.kind !== 'notes'));

    switch (command.operation) {
      case 'requirement.create':
      case 'requirement.update': {
        const previous = command.operation === 'requirement.update' ? requirement(command.id) : undefined;
        assert(!previous || previous.status !== 'archived', 'Archived requirements are immutable');
        assert(previous || command.status === 'draft', 'Requirements must be created as drafts');
        assert(previous || state.requirements.length < 500, 'Requirement limit reached');
        const id = previous?.id ?? randomUUID();
        if (command.parentId) assert(requirement(command.parentId).status !== 'archived', 'Parent requirement is archived');
        const item: Requirement = {
          id,
          title: command.title,
          detail: command.detail,
          acceptanceCriteria: command.acceptanceCriteria,
          parentId: command.parentId,
          status: command.status
        };
        const updated = [...state.requirements.filter(value => value.id !== id), item];
        assertAcyclic(updated.map(value => value.id), updated.filter(value => value.parentId).map(value => [value.id, value.parentId!]));
        if (['approved', 'accepted'].includes(command.status)) {
          assert(command.acceptanceCriteria.trim(), 'Acceptance criteria are required');
          assert(linkedPublishedDocuments(id).length > 0, 'A published planning document is required');
          if (previous && ['approved', 'accepted'].includes(previous.status)) {
            assert(
              previous.detail === command.detail
                && previous.acceptanceCriteria === command.acceptanceCriteria
                && previous.title === command.title
                && previous.parentId === command.parentId,
              'Reopen the requirement as draft before revising approved content'
            );
            if (command.operation === 'requirement.update' && command.prerequisiteIds !== undefined) {
              const current = state.dependencies.filter(value => value.kind === 'requirement' && value.id === id).map(value => value.prerequisiteId).sort();
              assert(canonical(current) === canonical([...command.prerequisiteIds].sort()), 'Reopen the requirement as draft before changing dependencies');
            }
          }
        }
        if (command.status === 'archived') {
          assert(
            !state.requirements.some(value => value.parentId === id && value.status !== 'archived')
              && !state.workItems.some(value => value.requirementId === id && value.status !== 'archived'),
            'Archive child requirements and work items first'
          );
          assert(!state.dependencies.some(value => value.kind === 'requirement' && value.prerequisiteId === id), 'Requirement is still a prerequisite');
        }
        if (command.operation === 'requirement.update' && command.prerequisiteIds !== undefined) {
          this.setDependencies('requirement', id, command.prerequisiteIds, state);
        }
        this.save('requirements', item);
        return { id };
      }

      case 'work_item.create':
      case 'work_item.update': {
        const previous = command.operation === 'work_item.update' ? workItem(command.id) : undefined;
        const parent = requirement(command.requirementId);
        assert(parent.status !== 'archived', 'Requirement is archived');
        assert(!previous || previous.status !== 'archived', 'Archived work items are immutable');
        assert(!previous || previous.requirementId === command.requirementId, 'Work items cannot move between requirements');
        assert(previous || command.status === 'todo', 'Work items must be created as todo');
        assert(previous || state.workItems.length < 500, 'Work item limit reached');
        const id = previous?.id ?? randomUUID();
        if (['ready', 'accepted'].includes(command.status)) {
          assert(command.acceptanceCriteria.trim(), 'Acceptance criteria are required');
          assert(linkedPublishedDocuments(parent.id).length > 0, 'A published planning document is required before readiness');
        }
        if (command.status === 'archived') {
          assert(!state.dependencies.some(value => value.kind === 'work_item' && value.prerequisiteId === id), 'Work item is still a prerequisite');
        }
        const item: WorkItem = {
          id,
          title: command.title,
          detail: command.detail,
          acceptanceCriteria: command.acceptanceCriteria,
          requirementId: command.requirementId,
          status: command.status
        };
        if (command.operation === 'work_item.update' && command.prerequisiteIds !== undefined) {
          this.setDependencies('work_item', id, command.prerequisiteIds, state);
        }
        this.save('work_items', item);
        return { id };
      }

      case 'document.create':
      case 'document.revise': {
        const previous = command.operation === 'document.revise' ? document(command.id) : undefined;
        assert(!previous || previous.status !== 'archived', 'Archived documents are immutable');
        assert(previous || state.documents.length < 500, 'Document limit reached');
        for (const requirementId of command.requirementIds) {
          assert(requirement(requirementId).status !== 'archived', 'Linked requirement is archived');
        }
        if (previous) {
          const protectedRequirementIds = new Set([
            ...state.documentLinks.filter(value => value.documentId === previous.id).map(value => value.requirementId),
            ...command.requirementIds
          ]);
          for (const requirementId of protectedRequirementIds) {
            assert(!['approved', 'accepted'].includes(requirement(requirementId).status), 'Reopen approved linked requirements before revising their document');
          }
          const oldLinks = state.documentLinks.filter(value => value.documentId === previous.id).map(value => value.requirementId).sort();
          assert(
            previous.title !== command.title
              || previous.kind !== command.kind
              || previous.format !== command.format
              || previous.status !== command.status
              || currentDocumentContent(previous.id) !== command.content
              || canonical(oldLinks) !== canonical([...command.requirementIds].sort()),
            'Document revision does not contain any changes'
          );
        } else {
          assert(command.status !== 'archived', 'Documents cannot be created as archived');
        }
        const now = new Date().toISOString();
        const id = previous?.id ?? randomUUID();
        const version = (previous?.currentVersion ?? 0) + 1;
        const item: PlanningDocument = {
          id,
          title: command.title,
          kind: command.kind,
          format: command.format,
          status: command.status,
          currentVersion: version,
          createdAt: previous?.createdAt ?? now,
          updatedAt: now
        };
        const documentVersion: DocumentVersion = {
          id: randomUUID(),
          documentId: id,
          version,
          content: command.content,
          contentSha256: sha256(command.content),
          createdAt: now
        };
        this.save('documents', item);
        this.db.prepare('INSERT INTO document_versions(id,document_id,version,body) VALUES(?,?,?,?)')
          .run(documentVersion.id, id, version, JSON.stringify(documentVersion));
        this.db.prepare('DELETE FROM document_requirement_links WHERE document_id=?').run(id);
        for (const requirementId of command.requirementIds) {
          this.db.prepare('INSERT INTO document_requirement_links VALUES(?,?)').run(id, requirementId);
        }
        return { id };
      }

      case 'dependencies.set': {
        this.setDependencies(command.kind, command.id, command.prerequisiteIds, state);
        return { id: command.id };
      }

      case 'plan.create': {
        assert(state.plans.length < 100, 'Plan version limit reached');
        const workItemIds = new Set(command.workItemIds);
        const requirementIds = new Set<string>();
        let changed = true;
        while (changed) {
          changed = false;
          for (const workItemId of [...workItemIds]) {
            const item = workItem(workItemId);
            if (!requirementIds.has(item.requirementId)) { requirementIds.add(item.requirementId); changed = true; }
            for (const edge of state.dependencies.filter(value => value.kind === 'work_item' && value.id === workItemId)) {
              if (!workItemIds.has(edge.prerequisiteId)) { workItemIds.add(edge.prerequisiteId); changed = true; }
            }
          }
          for (const requirementId of [...requirementIds]) {
            for (const edge of state.dependencies.filter(value => value.kind === 'requirement' && value.id === requirementId)) {
              if (!requirementIds.has(edge.prerequisiteId)) { requirementIds.add(edge.prerequisiteId); changed = true; }
            }
          }
        }
        const items = [...workItemIds].map(workItem);
        const requirements = [...requirementIds].map(requirement);
        for (const item of items) assert(item.status === 'ready', 'Plan work items and their prerequisites must be ready');
        for (const item of requirements) {
          assert(item.status === 'approved', 'Plan requirements and their prerequisites must be approved');
          assert(linkedPublishedDocuments(item.id).length > 0, 'Plan is missing a published planning document');
        }
        const documentIds = new Set(state.documentLinks.filter(value => requirementIds.has(value.requirementId)).map(value => value.documentId));
        const documents = [...documentIds].map(documentId => {
          const item = document(documentId);
          const pinnedVersion = state.documentVersions.find(value => value.documentId === item.id && value.version === item.currentVersion);
          assert(pinnedVersion, 'Current document version is missing');
          return {
            ...item,
            pinnedVersion,
            requirementIds: state.documentLinks.filter(value => value.documentId === item.id && requirementIds.has(value.requirementId)).map(value => value.requirementId)
          };
        });
        const dependencies = state.dependencies.filter(value => {
          return value.kind === 'work_item'
            ? workItemIds.has(value.id) && workItemIds.has(value.prerequisiteId)
            : requirementIds.has(value.id) && requirementIds.has(value.prerequisiteId);
        });
        const plan: Plan = {
          id: randomUUID(),
          title: command.title,
          version: state.plans.length + 1,
          status: 'draft',
          createdAt: new Date().toISOString(),
          requirements,
          documents,
          workItems: items,
          dependencies
        };
        this.save('plans', plan);
        return { id: plan.id };
      }

      case 'plan.approve': {
        const plan = state.plans.find(value => value.id === command.id);
        assert(plan && plan.status === 'draft', 'Only a draft plan can be approved');
        this.save('plans', { ...plan, status: 'approved' });
        return { id: plan.id };
      }

      case 'execution.prepare': {
        const plan = state.plans.find(value => value.id === command.planId);
        assert(plan?.status === 'approved', 'Only an approved frozen plan can prepare execution');
        assert(!state.executionIntents.some(value => value.planId === plan.id), 'This plan already has an execution intent');
        const intent: ExecutionIntent = {
          id: randomUUID(),
          planId: plan.id,
          title: command.title,
          status: 'prepared',
          createdAt: new Date().toISOString(),
          approvedAt: null
        };
        this.save('execution_intents', intent);
        return { id: intent.id };
      }

      case 'execution.approve': {
        const intent = state.executionIntents.find(value => value.id === command.id);
        assert(intent?.status === 'prepared', 'Only a prepared execution intent can be approved');
        this.save('execution_intents', { ...intent, status: 'approved', approvedAt: new Date().toISOString() });
        return { id: intent.id };
      }

      case 'execution.link': {
        const intent = state.executionIntents.find(value => value.id === command.intentId);
        assert(intent?.status === 'approved', 'Execution references require an approved intent');
        assert(
          !state.executionReferences.some(value => value.intentId === intent.id && value.referenceType === command.referenceType && value.externalId === command.externalId),
          'Execution reference already exists'
        );
        const reference: ExecutionReference = {
          id: randomUUID(),
          intentId: intent.id,
          provider: 'task-runner',
          referenceType: command.referenceType,
          externalId: command.externalId,
          url: command.url,
          revision: command.revision,
          createdAt: new Date().toISOString()
        };
        this.save('execution_references', reference);
        return { id: reference.id };
      }

      case 'execution.link_batch': {
        const intent = state.executionIntents.find(value => value.id === command.intentId);
        assert(intent?.status === 'approved', 'Execution references require an approved intent');
        const desired = [
          { referenceType: 'batch' as const, externalId: command.batchId },
          ...command.tasks.map(task => ({ referenceType: 'task' as const, externalId: task.taskId }))
        ];
        const foreign = state.executionReferences.find(reference => desired.some(value =>
          value.referenceType === reference.referenceType && value.externalId === reference.externalId
        ) && reference.intentId !== intent.id);
        assert(!foreign, 'Execution reference belongs to another intent');
        let batchReference = state.executionReferences.find(value =>
          value.intentId === intent.id && value.referenceType === 'batch' && value.externalId === command.batchId
        );
        for (const item of desired) {
          if (state.executionReferences.some(value =>
            value.intentId === intent.id && value.referenceType === item.referenceType && value.externalId === item.externalId
          )) continue;
          const reference: ExecutionReference = {
            id: randomUUID(), intentId: intent.id, provider: 'task-runner',
            referenceType: item.referenceType, externalId: item.externalId,
            url: null, revision: command.revision, createdAt: new Date().toISOString()
          };
          this.save('execution_references', reference);
          if (item.referenceType === 'batch') batchReference = reference;
        }
        assert(batchReference, 'Task batch reference was not stored');
        return { id: batchReference.id };
      }
    }
  }

  private setDependencies(
    kind: 'requirement' | 'work_item',
    id: string,
    prerequisiteIds: string[],
    state: ReturnType<PlanningStore['state']>
  ) {
    const items = kind === 'requirement' ? state.requirements : state.workItems;
    const item = items.find(value => value.id === id);
    assert(item && item.status !== 'archived', 'Dependency source not found or archived');
    for (const prerequisiteId of prerequisiteIds) {
      assert(items.some(value => value.id === prerequisiteId && value.status !== 'archived'), 'Dependency target not found or archived');
    }
    const edges: [string, string][] = state.dependencies
      .filter(value => value.kind === kind && value.id !== id)
      .map(value => [value.id, value.prerequisiteId]);
    edges.push(...prerequisiteIds.map(value => [id, value] as [string, string]));
    assertAcyclic(items.map(value => value.id), edges);
    this.db.prepare('DELETE FROM dependencies WHERE kind=? AND id=?').run(kind, id);
    for (const prerequisiteId of prerequisiteIds) {
      this.db.prepare('INSERT INTO dependencies VALUES(?,?,?)').run(kind, id, prerequisiteId);
    }
  }
}
