// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const test = require('node:test');

const { coreRestartDelayMs, createCoreRuntime } = require('../electron/core-runtime.cjs');

test('backs off unexpected Core restarts without growing past five seconds', () => {
  assert.deepEqual(
    [0, 1, 2, 3, 4, 5, 20].map(coreRestartDelayMs),
    [250, 500, 1_000, 2_000, 4_000, 5_000, 5_000],
  );
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
