'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');

const launcher = fs.readFileSync(path.join(__dirname, '..', 'bin', 'chatos-browser-cdp'), 'utf8');

test('launcher declares all six supported platform binaries', () => {
  for (const key of [
    'darwin-arm64',
    'darwin-x64',
    'linux-arm64',
    'linux-x64',
    'win32-arm64',
    'win32-x64'
  ]) {
    assert.match(launcher, new RegExp(`'${key}'`));
  }
});

test('launcher does not use a shell or install-time downloader', () => {
  assert.match(launcher, /shell: false/);
  assert.doesNotMatch(launcher, /exec\s*\(/);
  assert.doesNotMatch(launcher, /https?:\/\//);
});
