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
      WEB_DESIGN_STUDIO_DATA_DIR: root,
      CHATOS_CONTEXT_SCOPE: 'project',
      CHATOS_PROJECT_ID: 'host-project-through-123',
      CHATOS_PROJECT_NAME: '宿主产品项目',
      CHATOS_WORKSPACE_ID: 'workspace-through-456'
    },
    stdio: ['ignore', 'pipe', 'pipe']
  });
  try {
    await waitForReady(child, port);
    const base = `http://127.0.0.1:${port}`;
    const page = await fetch(base);
    assert.equal(page.status, 200);
    assert.match(await page.text(), /Web Design Studio/);

    const context = await fetch(`${base}/api/context`).then((response) => response.json());
    assert.equal(context.kind, 'project');
    assert.equal(context.chatosProjectId, 'host-project-through-123');
    assert.equal(context.chatosProjectName, '宿主产品项目');
    assert.equal(context.workspaceId, 'workspace-through-456');
    assert.ok(context.defaultProjectId);
    assert.notEqual(context.defaultProjectId, context.chatosProjectId);

    const projects = await fetch(`${base}/api/projects`).then((response) => response.json());
    assert.equal(projects.items.length, 1);
    assert.equal(projects.items[0].projectId, context.defaultProjectId);
    assert.equal(projects.items[0].name, '宿主产品项目');
    assert.equal(projects.items.some((project) => project.projectId === context.chatosProjectId), false);

    const projectDesign = await fetch(`${base}/api/projects/${context.defaultProjectId}/documents`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ title: '项目内空白网站', blank: true })
    }).then((response) => response.json());
    assert.equal(projectDesign.components.length, 0);
    const projectAfterCreate = await fetch(`${base}/api/projects/${context.defaultProjectId}`).then((response) => response.json());
    assert.deepEqual(projectAfterCreate.designIds, [projectDesign.documentId]);

    const created = await fetch(`${base}/api/documents`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ title: '本地网站设计' })
    }).then((response) => response.json());
    assert.equal(created.revision, 1);
    const stored = JSON.parse(await readFile(path.join(root, `${created.documentId}.web-design.json`), 'utf8'));
    assert.equal(stored.title, '本地网站设计');

    const exactDesign = structuredClone(created);
    exactDesign.viewport.width = 2560;
    exactDesign.viewport.height = 2300;
    exactDesign.breakpoints.desktop = { width: 2560, height: 2300, preview: { presetId: 'desktop-qhd', orientation: 'default', viewportHeight: 1440 } };
    const form = exactDesign.components[0];
    form.x = 928.25;
    form.y = 1478.75;
    form.width = 480.5;
    form.height = 720.25;
    form.style = { ...form.style, background: 'rgba(255,255,255,.83)', borderRadius: 17.5 };
    form.responsive = { mobile: { x: 21.5, y: 812.25, width: 347.5, height: 508.75 } };
    const saved = await fetch(`${base}/api/documents/${created.documentId}`, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ document: exactDesign, expectedRevision: created.revision })
    }).then((response) => response.json());
    const reopened = await fetch(`${base}/api/documents/${created.documentId}`).then((response) => response.json());
    assert.deepEqual(reopened.components, exactDesign.components);
    assert.deepEqual(reopened.viewport, exactDesign.viewport);
    assert.deepEqual(reopened.breakpoints, exactDesign.breakpoints);
    assert.deepEqual(saved.components, exactDesign.components);
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
