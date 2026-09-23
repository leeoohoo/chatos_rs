import { createHash, randomUUID } from 'node:crypto';
import { promises as fs } from 'node:fs';
import path from 'node:path';
import lockfile from 'proper-lockfile';
import { assertIdentifier, assertSolutionWorkspace, createSolutionWorkspace, workspaceSummary, type ChatosProjectBinding, type SolutionWorkspace, type SolutionWorkspaceSummary, type SourceMode } from './schema.js';
import { projectBindingFromContext } from './runtime-context.js';

export class RevisionConflictError extends Error {
  constructor(public readonly actualRevision: number) {
    super(`Solution workspace revision conflict. Current revision is ${actualRevision}.`);
  }
}

export function resolveDataDirectory(): string {
  return path.resolve(process.env.SOLUTION_STUDIO_DATA_DIR ?? process.env.CHATOS_PLUGIN_DATA_DIR ?? path.join(process.cwd(), '.solution-studio-data'));
}

export function dataScopeFingerprint(rootDirectory: string): string {
  return createHash('sha256').update(path.resolve(rootDirectory)).digest('hex');
}

export class SolutionWorkspaceStore {
  constructor(public readonly rootDirectory = resolveDataDirectory(), public readonly hostProject = projectBindingFromContext()) {}

  async initialize(): Promise<void> {
    await fs.mkdir(this.rootDirectory, { recursive: true });
  }

  async list(): Promise<SolutionWorkspaceSummary[]> {
    await this.initialize();
    const entries = await fs.readdir(this.rootDirectory, { withFileTypes: true });
    const workspaces: SolutionWorkspace[] = [];
    for (const entry of entries) {
      if (!entry.isFile() || !entry.name.endsWith('.solution.json')) continue;
      try { workspaces.push(await this.read(entry.name.slice(0, -'.solution.json'.length))); }
      catch { /* A malformed workspace remains visible through its direct read error. */ }
    }
    return workspaces
      .sort((left, right) => right.updatedAt.localeCompare(left.updatedAt))
      .slice(0, 1)
      .map(workspaceSummary);
  }

  async getCurrent(): Promise<SolutionWorkspace | undefined> {
    const current = (await this.list())[0];
    return current ? this.read(current.workspaceId) : undefined;
  }

  async findByArtifactKey(artifactKey: string): Promise<SolutionWorkspace | undefined> {
    assertIdentifier(artifactKey, 'artifactKey');
    const summaries = await this.list();
    const match = summaries.find((item) => item.artifactKey === artifactKey);
    return match ? this.read(match.workspaceId) : undefined;
  }

  async read(workspaceId: string): Promise<SolutionWorkspace> {
    assertIdentifier(workspaceId, 'workspaceId');
    const value: unknown = JSON.parse(await fs.readFile(this.workspacePath(workspaceId), 'utf8'));
    assertSolutionWorkspace(value);
    if (value.workspaceId !== workspaceId) throw new Error('Solution workspace identity does not match its filename.');
    return this.bindToHostProject(value);
  }

  async create(title: string, sourceMode: SourceMode = 'existing-project', artifactKey?: string): Promise<SolutionWorkspace> {
    return this.withLock(async () => {
      const current = await this.getCurrent();
      if (current) return current;
      const key = artifactKey?.trim();
      const workspace = createSolutionWorkspace(title, sourceMode, key, this.hostProject);
      const now = new Date().toISOString();
      workspace.revision = 1;
      workspace.createdAt = now;
      workspace.updatedAt = now;
      await this.atomicWrite(this.workspacePath(workspace.workspaceId), workspace, false);
      return workspace;
    });
  }

  async replace(workspace: SolutionWorkspace, expectedRevision: number): Promise<SolutionWorkspace> {
    assertSolutionWorkspace(workspace);
    return this.withLock(async () => {
      const current = await this.read(workspace.workspaceId);
      if (current.revision !== expectedRevision) throw new RevisionConflictError(current.revision);
      const next = structuredClone(workspace);
      this.assertHostProjectMatches(next.hostProject);
      next.hostProject = current.hostProject ? structuredClone(current.hostProject) : undefined;
      next.revision = expectedRevision + 1;
      next.createdAt = current.createdAt;
      next.updatedAt = new Date().toISOString();
      assertSolutionWorkspace(next);
      await this.atomicWrite(this.workspacePath(next.workspaceId), next, true);
      return next;
    });
  }

  async upsert(artifactKey: string, title: string, sourceMode: SourceMode, updater: (workspace: SolutionWorkspace) => SolutionWorkspace): Promise<{ workspace: SolutionWorkspace; created: boolean }> {
    assertIdentifier(artifactKey, 'artifactKey');
    return this.withLock(async () => {
      const existing = await this.getCurrent();
      const base = existing ?? createSolutionWorkspace(title, sourceMode, artifactKey, this.hostProject);
      const next = updater(structuredClone(base));
      next.title = title.trim().slice(0, 240) || base.title;
      next.sourceMode = sourceMode;
      next.revision = existing ? existing.revision + 1 : 1;
      next.createdAt = existing?.createdAt ?? new Date().toISOString();
      next.updatedAt = new Date().toISOString();
      assertSolutionWorkspace(next);
      await this.atomicWrite(this.workspacePath(next.workspaceId), next, Boolean(existing));
      return { workspace: next, created: !existing };
    });
  }

  async remove(workspaceId: string): Promise<void> {
    assertIdentifier(workspaceId, 'workspaceId');
    await this.withLock(async () => { await fs.unlink(this.workspacePath(workspaceId)); });
  }

  private workspacePath(workspaceId: string): string {
    assertIdentifier(workspaceId, 'workspaceId');
    return path.join(this.rootDirectory, `${workspaceId}.solution.json`);
  }

  private assertHostProjectMatches(binding: ChatosProjectBinding | undefined): void {
    if (!binding) return;
    if (!this.hostProject) throw new Error(`Solution workspace belongs to ChatOS project ${binding.projectId}, but no project context is active.`);
    if (binding.projectId !== this.hostProject.projectId) throw new Error(`Solution workspace belongs to a different ChatOS project (${binding.projectId}).`);
  }

  private bindToHostProject(workspace: SolutionWorkspace): SolutionWorkspace {
    this.assertHostProjectMatches(workspace.hostProject);
    if (!this.hostProject) return workspace;
    return {
      ...workspace,
      hostProject: {
        ...structuredClone(workspace.hostProject ?? this.hostProject),
        ...structuredClone(this.hostProject),
        projectId: this.hostProject.projectId
      }
    };
  }

  private async withLock<T>(action: () => Promise<T>): Promise<T> {
    await this.initialize();
    const lockPath = path.join(this.rootDirectory, '.store-lock');
    await fs.mkdir(lockPath, { recursive: true });
    const release = await lockfile.lock(lockPath, { realpath: false, retries: { retries: 8, factor: 1.5, minTimeout: 20, maxTimeout: 250 }, stale: 15_000 });
    try { return await action(); }
    finally { await release(); }
  }

  private async atomicWrite(destination: string, workspace: SolutionWorkspace, overwrite: boolean): Promise<void> {
    await this.initialize();
    if (!overwrite) {
      try { await fs.access(destination); throw new Error(`Workspace already exists: ${workspace.workspaceId}`); }
      catch (error) { if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error; }
    }
    const temporary = `${destination}.${process.pid}.${randomUUID()}.tmp`;
    await fs.writeFile(temporary, `${JSON.stringify(workspace, null, 2)}\n`, { encoding: 'utf8', mode: 0o600 });
    await fs.rename(temporary, destination);
  }
}
