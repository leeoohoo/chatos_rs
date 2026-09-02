import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { createServer } from 'node:net';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';

test('studio command serves the packaged UI and persists blank project diagrams', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'diagram-studio-server-test-'));
  const port = await availablePort();
  const child = spawn(process.execPath, ['bin/chatos-diagram-studio', 'studio'], {
    cwd: process.cwd(),
    env: {
      ...process.env,
      DIAGRAM_STUDIO_HOST: '127.0.0.1',
      DIAGRAM_STUDIO_PORT: String(port),
      DIAGRAM_STUDIO_DATA_DIR: root
    },
    stdio: ['ignore', 'pipe', 'pipe']
  });

  try {
    await waitForReady(child, port);
    const base = `http://127.0.0.1:${port}`;
    const page = await fetch(base);
    assert.equal(page.status, 200);
    assert.match(await page.text(), /Diagram Studio/);

    const health = await fetch(`${base}/api/health`).then((response) => response.json());
    assert.equal(health.ok, true);
    assert.equal(health.dataDirectory, root);

    const project = await fetch(`${base}/api/projects`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ name: '本地客户端验证' })
    }).then((response) => response.json());
    const document = await fetch(`${base}/api/projects/${project.projectId}/documents`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ kind: 'sequence', title: '登录时序', blank: true })
    }).then((response) => response.json());

    assert.equal(document.kind, 'sequence');
    assert.deepEqual(document.nodes, []);
    assert.deepEqual(document.edges, []);
    const stored = JSON.parse(await readFile(path.join(root, `${document.documentId}.diagram.json`), 'utf8'));
    assert.equal(stored.title, '登录时序');
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
  throw new Error(`Timed out waiting for Diagram Studio: ${stderr}`);
}
