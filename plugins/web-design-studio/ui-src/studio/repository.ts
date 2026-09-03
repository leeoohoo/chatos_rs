import type { WebDesignDocument } from '../../src/schema';
import { createLandingPage } from '../../src/templates';

export interface DesignSummary {
  documentId: string;
  revision: number;
  title: string;
  componentCount: number;
  pageCount?: number;
  pendingRequestCount: number;
  updatedAt: string;
}

export interface DesignRepository {
  mode: 'server' | 'local';
  list(): Promise<DesignSummary[]>;
  read(documentId: string): Promise<WebDesignDocument>;
  create(title?: string): Promise<WebDesignDocument>;
  save(document: WebDesignDocument, expectedRevision: number): Promise<WebDesignDocument>;
  remove(documentId: string): Promise<void>;
}

const indexKey = 'chatos.web-design-studio.index.v1';
const documentPrefix = 'chatos.web-design-studio.document.v1.';

function summary(document: WebDesignDocument): DesignSummary {
  return {
    documentId: document.documentId,
    revision: document.revision,
    title: document.title,
    componentCount: document.components.length,
    pageCount: document.pages?.length ?? 1,
    pendingRequestCount: document.requests.filter((request) => request.status === 'pending').length,
    updatedAt: document.updatedAt
  };
}

class LocalRepository implements DesignRepository {
  readonly mode = 'local' as const;

  async list(): Promise<DesignSummary[]> {
    const ids = JSON.parse(localStorage.getItem(indexKey) ?? '[]') as string[];
    return ids.flatMap((id) => {
      const raw = localStorage.getItem(`${documentPrefix}${id}`);
      if (!raw) return [];
      try { return [summary(JSON.parse(raw) as WebDesignDocument)]; }
      catch { return []; }
    }).sort((left, right) => right.updatedAt.localeCompare(left.updatedAt));
  }

  async read(documentId: string): Promise<WebDesignDocument> {
    const raw = localStorage.getItem(`${documentPrefix}${documentId}`);
    if (!raw) throw new Error('没有找到这个网站设计。');
    return JSON.parse(raw) as WebDesignDocument;
  }

  async create(title?: string): Promise<WebDesignDocument> {
    const document = createLandingPage(title?.trim() || undefined);
    const now = new Date().toISOString();
    document.revision = 1;
    document.createdAt = now;
    document.updatedAt = now;
    await this.persist(document);
    return document;
  }

  async save(document: WebDesignDocument, expectedRevision: number): Promise<WebDesignDocument> {
    const current = await this.read(document.documentId);
    if (current.revision !== expectedRevision) throw new Error(`设计已经更新到版本 ${current.revision}，请刷新后再编辑。`);
    const next = structuredClone(document);
    next.revision = expectedRevision + 1;
    next.updatedAt = new Date().toISOString();
    await this.persist(next);
    return next;
  }

  async remove(documentId: string): Promise<void> {
    localStorage.removeItem(`${documentPrefix}${documentId}`);
    const ids = JSON.parse(localStorage.getItem(indexKey) ?? '[]') as string[];
    localStorage.setItem(indexKey, JSON.stringify(ids.filter((id) => id !== documentId)));
  }

  private async persist(document: WebDesignDocument): Promise<void> {
    localStorage.setItem(`${documentPrefix}${document.documentId}`, JSON.stringify(document));
    const ids = JSON.parse(localStorage.getItem(indexKey) ?? '[]') as string[];
    if (!ids.includes(document.documentId)) {
      ids.unshift(document.documentId);
      localStorage.setItem(indexKey, JSON.stringify(ids));
    }
  }
}

class ServerRepository implements DesignRepository {
  readonly mode = 'server' as const;

  async list(): Promise<DesignSummary[]> {
    const response = await fetch('/api/documents', { cache: 'no-store' });
    if (!response.ok) throw new Error('无法读取网站设计列表。');
    return (await response.json() as { items: DesignSummary[] }).items;
  }

  async read(documentId: string): Promise<WebDesignDocument> {
    const response = await fetch(`/api/documents/${encodeURIComponent(documentId)}`, { cache: 'no-store' });
    if (!response.ok) throw new Error('无法打开这个网站设计。');
    return response.json() as Promise<WebDesignDocument>;
  }

  async create(title?: string): Promise<WebDesignDocument> {
    const response = await fetch('/api/documents', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ title: title?.trim() || undefined })
    });
    if (!response.ok) throw new Error('无法创建网站设计。');
    return response.json() as Promise<WebDesignDocument>;
  }

  async save(document: WebDesignDocument, expectedRevision: number): Promise<WebDesignDocument> {
    const response = await fetch(`/api/documents/${encodeURIComponent(document.documentId)}`, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ document, expectedRevision })
    });
    if (!response.ok) {
      const body = await response.json().catch(() => ({ error: '保存失败。' })) as { error?: string };
      throw new Error(body.error ?? '保存失败。');
    }
    return response.json() as Promise<WebDesignDocument>;
  }

  async remove(documentId: string): Promise<void> {
    const response = await fetch(`/api/documents/${encodeURIComponent(documentId)}`, { method: 'DELETE' });
    if (!response.ok) throw new Error('删除失败。');
  }
}

export async function createRepository(): Promise<DesignRepository> {
  try {
    const response = await fetch('/api/health', { cache: 'no-store', signal: AbortSignal.timeout(900) });
    if (response.ok) return new ServerRepository();
  } catch {
    // Vite and a static plugin preview can use browser-local storage.
  }
  return new LocalRepository();
}
