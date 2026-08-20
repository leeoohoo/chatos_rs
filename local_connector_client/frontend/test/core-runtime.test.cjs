// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const test = require('node:test');

const {
  coreRestartDelayMs,
  createCoreRuntime,
  discoverUserShellPath,
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
      assert.equal(options.timeout, 2_000);
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
  assert.deepEqual(discoverUserShellPath({
    platform: 'darwin',
    env: { SHELL: '/bin/zsh' },
    fileExists: () => true,
    spawnSyncImpl: () => ({ status: null, error: new Error('timeout') }),
  }), []);
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
