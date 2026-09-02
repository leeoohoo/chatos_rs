import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { spawn } from 'node:child_process';
import { mkdir, mkdtemp, readFile, rm } from 'node:fs/promises';
import { createServer } from 'node:net';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { promisify } from 'node:util';

const execFileAsync = promisify(execFile);

test('packed plugin starts its local backend without repository node_modules', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'diagram-studio-installed-test-'));
  const packDirectory = path.join(root, 'pack');
  const installDirectory = path.join(root, 'install');
  const dataDirectory = path.join(root, 'data');
  const port = await availablePort();
  let child;
  try {
    await Promise.all([
      mkdir(packDirectory, { recursive: true }),
      mkdir(installDirectory, { recursive: true })
    ]);
    const { stdout } = await execFileAsync('npm', [
      'pack', '--json', '--pack-destination', packDirectory
    ], { cwd: process.cwd(), maxBuffer: 4 * 1024 * 1024 });
    const [{ filename }] = JSON.parse(stdout);
    await execFileAsync('tar', [
      '-xzf', path.join(packDirectory, filename), '-C', installDirectory
    ]);
    const packageRoot = path.join(installDirectory, 'package');
    const packageJson = JSON.parse(await readFile(path.join(packageRoot, 'package.json'), 'utf8'));
    assert.equal(packageJson.bin['chatos-diagram-studio'], 'bin/chatos-diagram-studio');

    child = spawn(process.execPath, ['bin/chatos-diagram-studio', 'studio'], {
      cwd: packageRoot,
      env: {
        PATH: process.env.PATH,
        CHATOS_PLUGIN_APP_HOST: '127.0.0.1',
        CHATOS_PLUGIN_APP_PORT: String(port),
        CHATOS_PLUGIN_DATA_DIR: dataDirectory
      },
      stdio: ['ignore', 'pipe', 'pipe']
    });
    await waitForReady(child, port);

    const health = await fetch(`http://127.0.0.1:${port}/api/health`)
      .then((response) => response.json());
    assert.equal(health.ok, true);
    assert.equal(health.dataDirectory, dataDirectory);
  } finally {
    child?.kill('SIGTERM');
    if (child && child.exitCode === null) {
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
    if (child.exitCode !== null) throw new Error(`Installed Studio exited: ${stderr}`);
    try {
      const response = await fetch(`http://127.0.0.1:${port}/api/health`);
      if (response.ok) return;
    } catch {}
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  throw new Error(`Timed out waiting for installed Diagram Studio: ${stderr}`);
}
