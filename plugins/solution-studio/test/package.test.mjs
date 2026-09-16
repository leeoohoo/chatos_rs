import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { access, cp, mkdtemp, readFile, rm } from 'node:fs/promises';
import { createServer } from 'node:net';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';

test('package contains manifests, built UI, runtime entries, and finished skills', async () => {
  for (const file of ['.codex-plugin/plugin.json', '.mcp.json', 'chatos.plugin.json', 'ui/index.html', 'ui/assets/app.js', 'ui/assets/app.css', 'dist/mcp-server.mjs', 'dist/studio-server.mjs', 'dist/runtime-context.mjs']) await access(file);
  const manifest = JSON.parse(await readFile('.codex-plugin/plugin.json', 'utf8'));
  assert.equal(manifest.name, 'solution-studio');
  assert.equal(manifest.mcpServers, './.mcp.json');
  const chatosManifest = JSON.parse(await readFile('chatos.plugin.json', 'utf8'));
  assert.deepEqual(chatosManifest.runtimeContext.optional, ['project.id', 'workspace.id', 'workspace.root']);
  const files = ['skills/solution-studio/SKILL.md', 'skills/solution-discovery/SKILL.md', 'skills/solution-design/SKILL.md', 'skills/solution-execution-plan/SKILL.md', 'skills/solution-validation/SKILL.md'];
  for (const file of files) assert.doesNotMatch(await readFile(file, 'utf8'), /TODO|\[TODO:/);
  const app = await readFile('ui/assets/app.js', 'utf8');
  assert.doesNotMatch(app, /Diagram Studio|关联图表/);
  assert.doesNotMatch(app, /基于代码与文档证据|从目标与约束开始/);
  assert.doesNotMatch(app, /所有方案|最近方案/);
});

test('packaged runtime starts without node_modules', async () => {
  const packageRoot = await mkdtemp(path.join(os.tmpdir(), 'solution-studio-package-'));
  const dataRoot = await mkdtemp(path.join(os.tmpdir(), 'solution-studio-data-'));
  const port = await availablePort();
  await Promise.all([
    cp('bin', path.join(packageRoot, 'bin'), { recursive: true }),
    cp('dist', path.join(packageRoot, 'dist'), { recursive: true }),
    cp('ui', path.join(packageRoot, 'ui'), { recursive: true })
  ]);
  const child = spawn(process.execPath, ['bin/chatos-solution-studio', 'studio'], {
    cwd: packageRoot,
    env: {
      ...process.env,
      SOLUTION_STUDIO_HOST: '127.0.0.1',
      SOLUTION_STUDIO_PORT: String(port),
      SOLUTION_STUDIO_DATA_DIR: dataRoot
    },
    stdio: ['ignore', 'ignore', 'pipe']
  });
  try {
    await waitForHealth(child, port);
    const health = await fetch(`http://127.0.0.1:${port}/api/health`).then((response) => response.json());
    assert.equal(health.ok, true);
  } finally {
    if (child.exitCode === null) {
      child.kill('SIGTERM');
      await new Promise((resolve) => child.once('exit', resolve));
    }
    await Promise.all([
      rm(packageRoot, { recursive: true, force: true }),
      rm(dataRoot, { recursive: true, force: true })
    ]);
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

async function waitForHealth(child, port) {
  const deadline = Date.now() + 8_000;
  let stderr = '';
  child.stderr.on('data', (chunk) => { stderr += chunk.toString(); });
  while (Date.now() < deadline) {
    if (child.exitCode !== null) throw new Error(`Packaged runtime exited before startup: ${stderr}`);
    try {
      if ((await fetch(`http://127.0.0.1:${port}/api/health`)).ok) return;
    } catch {}
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  throw new Error(`Timed out waiting for packaged runtime: ${stderr}`);
}
