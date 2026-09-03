import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { createServer } from 'node:net';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';

test('studio serves the packaged workbench and persists a design', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'web-design-studio-server-test-'));
  const port = await availablePort();
  const child = spawn(process.execPath, ['bin/chatos-web-design-studio', 'studio'], {
    cwd: process.cwd(),
    env: {
      ...process.env,
      WEB_DESIGN_STUDIO_HOST: '127.0.0.1',
      WEB_DESIGN_STUDIO_PORT: String(port),
      WEB_DESIGN_STUDIO_DATA_DIR: root
    },
    stdio: ['ignore', 'pipe', 'pipe']
  });
  try {
    await waitForReady(child, port);
    const base = `http://127.0.0.1:${port}`;
    const page = await fetch(base);
    assert.equal(page.status, 200);
    assert.match(await page.text(), /Web Design Studio/);

    const created = await fetch(`${base}/api/documents`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ title: '本地网站设计' })
    }).then((response) => response.json());
    assert.equal(created.revision, 1);
    const stored = JSON.parse(await readFile(path.join(root, `${created.documentId}.web-design.json`), 'utf8'));
    assert.equal(stored.title, '本地网站设计');
  } finally {
    if (child.exitCode === null) {
      child.kill('SIGTERM');
      await new Promise((resolve) => child.once('exit', resolve));
    }
    await rm(root, { recursive: true, force: true });
  }
});

async function availablePort() {
  const server = createServer();
  await new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', resolve);
  });
  const address = server.address();
  const port = typeof address === 'object' && address ? address.port : 0;
  await new Promise((resolve) => server.close(resolve));
  return port;
}

async function waitForReady(child, port) {
  const deadline = Date.now() + 8000;
  let stderr = '';
  child.stderr.on('data', (chunk) => { stderr += chunk.toString(); });
  while (Date.now() < deadline) {
    if (child.exitCode !== null) throw new Error(`Studio exited before startup: ${stderr}`);
    try {
      const response = await fetch(`http://127.0.0.1:${port}/api/health`);
      if (response.ok) return;
    } catch {}
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  throw new Error(`Timed out waiting for Web Design Studio: ${stderr}`);
}
