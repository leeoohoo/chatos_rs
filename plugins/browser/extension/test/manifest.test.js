import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import path from 'node:path';
import test from 'node:test';
import {fileURLToPath} from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const manifest = JSON.parse(await readFile(path.join(root, 'manifest.json'), 'utf8'));
const background = await readFile(path.join(root, 'src', 'background.js'), 'utf8');
const popup = await readFile(path.join(root, 'popup', 'popup.html'), 'utf8');

test('manifest has the minimal explicit extension permissions', () => {
  assert.equal(manifest.manifest_version, 3);
  assert.deepEqual([...manifest.permissions].sort(), [
    'debugger',
    'nativeMessaging',
    'storage',
    'tabGroups',
    'tabs'
  ]);
  assert.equal(manifest.content_scripts, undefined);
  assert.equal(manifest.externally_connectable, undefined);
  assert.equal(manifest.host_permissions, undefined);
  assert.equal(manifest.background.type, 'module');
});

test('extension has no remote-code or webpage messaging path', async () => {
  assert.doesNotMatch(background, /onMessageExternal|onConnectExternal/);
  for (const file of ['src/background.js', 'src/bridge.js', 'src/controller.js', 'popup/popup.js']) {
    const source = await readFile(path.join(root, file), 'utf8');
    assert.doesNotMatch(source, /eval\s*\(|new\s+Function|https?:\/\//);
    assert.doesNotMatch(source, /console\s*\./);
  }
});

test('popup includes complete first-run and recovery guidance', () => {
  assert.match(popup, /安装插件/);
  assert.match(popup, /授权能力/);
  assert.match(popup, /启动任务/);
  assert.match(popup, /完成配对/);
  assert.match(popup, /连接不上怎么办/);
  assert.match(background, /AUTO_RECONNECT_DELAY_MS = 2000/);
  assert.match(background, /paired: pairingEnabled/);
});
