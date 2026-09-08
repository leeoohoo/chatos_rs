import { randomUUID } from 'node:crypto';
import { promises as fs } from 'node:fs';
import path from 'node:path';
import lockfile from 'proper-lockfile';

const safeFileName = /^[A-Za-z0-9][A-Za-z0-9._-]{0,239}$/;

export class AtomicJsonDirectory {
  constructor(public readonly rootDirectory: string) {}

  async initialize(): Promise<void> {
    await fs.mkdir(this.rootDirectory, { recursive: true });
  }

  resolve(fileName: string): string {
    if (!safeFileName.test(fileName)) throw new Error('Atomic JSON file name is invalid.');
    return path.join(this.rootDirectory, fileName);
  }

  async read<T>(fileName: string): Promise<T> {
    const raw = await fs.readFile(this.resolve(fileName), 'utf8');
    return JSON.parse(raw) as T;
  }

  async write(fileName: string, value: unknown): Promise<void> {
    await this.initialize();
    const destination = this.resolve(fileName);
    const temporary = path.join(this.rootDirectory, `.${fileName}.${process.pid}.${randomUUID()}.tmp`);
    const handle = await fs.open(temporary, 'wx', 0o600);
    try {
      await handle.writeFile(`${JSON.stringify(value, null, 2)}\n`, 'utf8');
      await handle.sync();
    } finally {
      await handle.close();
    }
    try {
      await fs.rename(temporary, destination);
      try {
        const directoryHandle = await fs.open(this.rootDirectory, 'r');
        try { await directoryHandle.sync(); } finally { await directoryHandle.close(); }
      } catch {
        // Directory fsync is unavailable on some platforms; the file itself was already synced and renamed.
      }
    } catch (error) {
      await fs.unlink(temporary).catch(() => undefined);
      throw error;
    }
  }

  async remove(fileName: string): Promise<void> {
    await fs.unlink(this.resolve(fileName));
  }

  async withLock<T>(task: () => Promise<T>): Promise<T> {
    await this.initialize();
    const release = await lockfile.lock(this.rootDirectory, {
      realpath: false,
      retries: { retries: 8, factor: 1.5, minTimeout: 20, maxTimeout: 400 }
    });
    try {
      return await task();
    } finally {
      await release();
    }
  }
}
