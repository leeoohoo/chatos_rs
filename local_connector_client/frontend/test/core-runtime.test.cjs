// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const test = require('node:test');
const { EventEmitter } = require('node:events');

const {
  coreRestartDelayMs,
  createCoreRuntime,
  discoverUserShellPath,
  readCachedUserShellPath,
  writeCachedUserShellPath,
} = require('../electron/core-runtime.cjs');

test('backs off unexpected Core restarts without growing past five seconds', () => {
  assert.deepEqual(
    [0, 1, 2, 3, 4, 5, 20].map(coreRestartDelayMs),
    [250, 500, 1_000, 2_000, 4_000, 5_000, 5_000],
  );
});

test('discovers and validates PATH entries from the interactive login shell', () => {
  const directories = new Set([
    '/Users/test/.docker/bin',
    '/Users/test/.local/bin',
    '/opt/homebrew/bin',
  ]);
  const discovered = discoverUserShellPath({
    platform: 'darwin',
    env: { SHELL: '/bin/zsh', PATH: '/usr/bin:/bin' },
    fileExists: (candidate) => candidate === '/bin/zsh',
    directoryExists: (candidate) => directories.has(candidate),
    spawnSyncImpl: (command, args, options) => {
      assert.equal(command, '/bin/zsh');
      assert.deepEqual(args, [
        '-ilc',
        'printf "__CHATOS_PATH_BEGIN__%s__CHATOS_PATH_END__" "$PATH"',
      ]);
      assert.equal(options.timeout, 10_000);
      return {
        status: 0,
        stdout: [
          'shell startup message',
          '__CHATOS_PATH_BEGIN__',
          '/Users/test/.docker/bin:relative:/missing:/Users/test/.local/bin:/opt/homebrew/bin',
          '__CHATOS_PATH_END__',
        ].join(''),
      };
    },
  });

  assert.deepEqual(discovered, [
    '/Users/test/.docker/bin',
    '/Users/test/.local/bin',
    '/opt/homebrew/bin',
  ]);
});

test('falls back cleanly when user shell PATH discovery fails', () => {
  let diagnostic = null;
  assert.deepEqual(discoverUserShellPath({
    platform: 'darwin',
    env: { SHELL: '/bin/zsh' },
    fileExists: () => true,
    spawnSyncImpl: () => ({ status: null, error: new Error('timeout') }),
    onDiagnostic: (value) => {
      diagnostic = value;
    },
  }), []);
  assert.equal(diagnostic.status, 'spawn-error');
  assert.equal(diagnostic.errorCode, 'Error');
});

test('persists and validates the last successful user shell PATH', () => {
  const tempRoot = fs.mkdtempSync(path.join(os.tmpdir(), 'chatos-shell-path-cache-'));
  try {
    const dockerBin = path.join(tempRoot, '.docker', 'bin');
    const localBin = path.join(tempRoot, '.local', 'bin');
    fs.mkdirSync(dockerBin, { recursive: true });
    fs.mkdirSync(localBin, { recursive: true });
    const cachePath = path.join(tempRoot, 'state', 'user-shell-path-cache.json');

    assert.equal(writeCachedUserShellPath(cachePath, [dockerBin, localBin, '/missing']), true);
    assert.deepEqual(readCachedUserShellPath(cachePath), [dockerBin, localBin]);
    assert.equal(fs.statSync(cachePath).mode & 0o777, 0o600);
  } finally {
    fs.rmSync(tempRoot, { recursive: true, force: true });
  }
});

test('passes cached user shell PATH to the spawned Core when discovery fails', () => {
  const tempRoot = fs.mkdtempSync(path.join(os.tmpdir(), 'chatos-core-path-fallback-'));
  const previousResourcesPath = process.resourcesPath;
  let spawned = null;
  try {
    process.resourcesPath = path.join(tempRoot, 'resources');
    fs.mkdirSync(process.resourcesPath, { recursive: true });
    fs.writeFileSync(path.join(process.resourcesPath, 'local_connector_client_core'), '');
    const cachedBin = path.join(tempRoot, 'user-bin');
    fs.mkdirSync(cachedBin, { recursive: true });
    const userData = path.join(tempRoot, 'user-data');
    assert.equal(
      writeCachedUserShellPath(
        path.join(userData, 'user-shell-path-cache.json'),
        [cachedBin],
      ),
      true,
    );

    const child = new EventEmitter();
    child.pid = 12345;
    child.exitCode = null;
    child.signalCode = null;
    child.kill = () => {};
    const runtime = createCoreRuntime({
      app: {
        getPath: (name) => ({
          userData,
          temp: path.join(tempRoot, 'temp'),
          logs: path.join(tempRoot, 'logs'),
        })[name],
        isPackaged: false,
      },
      desktopAuthToken: 'test-token',
      discoverUserShellPathImpl: ({ onDiagnostic }) => {
        onDiagnostic({ status: 'spawn-error', elapsedMs: 10_000, errorCode: 'ETIMEDOUT' });
        return [];
      },
      spawnImpl: (command, args, options) => {
        spawned = { command, args, options };
        return child;
      },
    });

    runtime.startCore();
    assert.ok(spawned.options.env.PATH.split(path.delimiter).includes(cachedBin));
    const log = fs.readFileSync(
      path.join(tempRoot, 'logs', 'local-connector-core.log'),
      'utf8',
    );
    assert.match(
      log,
      /core PATH source=cache .*shell_discovery=spawn-error .*shell_error_code=ETIMEDOUT/,
    );
    child.exitCode = 0;
    child.emit('exit', 0, null);
    runtime.cleanupIpcEndpoint();
  } finally {
    if (previousResourcesPath === undefined) {
      delete process.resourcesPath;
    } else {
      process.resourcesPath = previousResourcesPath;
    }
    fs.rmSync(tempRoot, { recursive: true, force: true });
  }
});

test('prepares Chrome extension in a visible user-home directory', () => {
  const tempRoot = fs.mkdtempSync(path.join(os.tmpdir(), 'chatos-core-runtime-'));
  const previousResourcesPath = process.resourcesPath;
  try {
    process.resourcesPath = path.join(tempRoot, 'resources');
    const source = path.join(process.resourcesPath, 'chrome-extension');
    fs.mkdirSync(source, { recursive: true });
    fs.writeFileSync(path.join(source, 'manifest.json'), '{"manifest_version":3}\n');
    fs.writeFileSync(path.join(source, 'background.js'), 'self;\n');

    const runtime = createCoreRuntime({
      app: { getPath: (name) => {
        assert.equal(name, 'home');
        return path.join(tempRoot, 'home');
      } },
      desktopAuthToken: 'test-token',
    });

    const destination = runtime.prepareChromeExtensionInstallDirectory();
    assert.equal(destination, path.join(tempRoot, 'home', 'ChatOS Chrome Extension'));
    assert.equal(fs.readFileSync(path.join(destination, 'manifest.json'), 'utf8'), '{"manifest_version":3}\n');
    assert.equal(fs.readFileSync(path.join(destination, 'background.js'), 'utf8'), 'self;\n');
  } finally {
    if (previousResourcesPath === undefined) {
      delete process.resourcesPath;
    } else {
      process.resourcesPath = previousResourcesPath;
    }
    fs.rmSync(tempRoot, { recursive: true, force: true });
  }
});
