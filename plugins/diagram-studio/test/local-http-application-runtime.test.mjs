import assert from 'node:assert/strict';
import { promises as fs } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import {
  pluginDirectoryWatchMode,
  readPluginLocalHttpEndpoint,
  startPluginDirectoryMonitor
} from '../dist/local-http-application-runtime.test.mjs';

const delay = (milliseconds) => new Promise((resolve) => setTimeout(resolve, milliseconds));

test('the host file-watch contract selects polling only when requested', () => {
  assert.equal(pluginDirectoryWatchMode({}), 'native');
  assert.equal(pluginDirectoryWatchMode({ CHATOS_PLUGIN_FILE_WATCH_MODE: 'polling' }), 'polling');
});

test('the shared runtime resolves the host-owned local_http endpoint', () => {
  assert.deepEqual(readPluginLocalHttpEndpoint({
    defaultPort: 4178,
    legacyHostEnvironmentKey: 'LEGACY_HOST',
    legacyPortEnvironmentKey: 'LEGACY_PORT',
    environment: {
      CHATOS_PLUGIN_APP_HOST: '127.0.0.1',
      CHATOS_PLUGIN_APP_PORT: '54321',
      LEGACY_HOST: '0.0.0.0',
      LEGACY_PORT: '1234'
    }
  }), { host: '127.0.0.1', port: 54321 });
});

test('the shared runtime rejects an invalid local_http port', () => {
  assert.throws(() => readPluginLocalHttpEndpoint({
    defaultPort: 4178,
    environment: { CHATOS_PLUGIN_APP_PORT: 'not-a-port' }
  }), /Invalid local_http application port/);
});

test('the shared polling monitor observes atomic directory changes', async () => {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), 'chatos-plugin-monitor-'));
  let resolveChange;
  const changed = new Promise((resolve) => { resolveChange = resolve; });
  const monitor = startPluginDirectoryMonitor({
    directory: root,
    accepts: (fileName) => fileName.endsWith('.diagram.json'),
    onChange: () => resolveChange(),
    mode: 'polling',
    pollIntervalMs: 250
  });

  try {
    await delay(350);
    const temporary = path.join(root, '.document.tmp');
    await fs.writeFile(temporary, '{}\n');
    await fs.rename(temporary, path.join(root, 'document.diagram.json'));
    await Promise.race([
      changed,
      delay(2_000).then(() => { throw new Error('directory change was not observed'); })
    ]);
  } finally {
    monitor.close();
    await fs.rm(root, { recursive: true, force: true });
  }
});
