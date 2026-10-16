// Opt-in local acceptance against immutable installed Releases, never a source overlay.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { spawn } from 'node:child_process';
import { createWriteStream, constants } from 'node:fs';
import { lstat, mkdir, mkdtemp, open, readFile, readdir, realpath } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';

const repository = path.resolve(import.meta.dirname, '../..');
const statePath = process.argv[2];
assert.ok(statePath && path.isAbsolute(statePath), 'Pass the original native state.json absolute path');
const { installedPluginRecords } = JSON.parse(await readFile(statePath, 'utf8'));
assert.ok(installedPluginRecords, 'Missing original installation integrity records');
const outputRoot = await mkdtemp(path.join(os.tmpdir(), 'chatos-installed-plugin-smoke-'));
const cases = [
  ['chatos-document-mcp', 'document', 'test/mcp.test.mjs'],
  ['chatos-diagram-studio', 'diagram-studio', 'test/mcp-smoke.test.mjs'],
  ['chatos-solution-studio', 'solution-studio', 'test/mcp-smoke.test.mjs'],
  ['chatos-web-design-studio', 'web-design-studio', 'test/mcp-smoke.test.mjs'],
  ['open-computer-use', 'computer-use', '../../clients/evals/computer-use-permissions-smoke.mjs'],
];
const results = [];

async function verifyInstallation(record) {
  const root = await realpath(record.installationPath);
  const expected = record.packageFileSHA256;
  assert.ok(expected && Object.keys(expected).length > 0, 'Missing trusted file hashes');
  const actual = new Map();
  let totalBytes = 0;
  async function visit(directory) {
    for (const entry of await readdir(directory)) {
      const file = path.join(directory, entry);
      const info = await lstat(file);
      assert.ok(!info.isSymbolicLink(), 'Installed package must not contain symlinks');
      if (info.isDirectory()) { await visit(file); continue; }
      assert.ok(info.isFile() && info.nlink === 1, 'Installed package must contain regular unlinked files');
      totalBytes += info.size;
      assert.ok(actual.size < 20_000 && totalBytes <= 512 * 1024 * 1024, 'Installation exceeds native integrity limits');
      const relative = path.relative(root, file);
      assert.ok(expected[relative], `Unrecorded package file: ${relative}`);
      const descriptor = await open(file, constants.O_RDONLY | constants.O_NOFOLLOW);
      try {
        const before = await descriptor.stat();
        assert.equal(before.ino, info.ino);
        const hash = createHash('sha256');
        for await (const chunk of descriptor.createReadStream({ autoClose: false })) hash.update(chunk);
        const after = await descriptor.stat();
        assert.equal(after.size, before.size);
        assert.equal(after.mtimeMs, before.mtimeMs);
        actual.set(relative, hash.digest('hex'));
        assert.equal(actual.get(relative), expected[relative], `Changed package file: ${relative}`);
      } finally { await descriptor.close(); }
    }
  }
  await visit(root);
  assert.equal(actual.size, Object.keys(expected).length, 'Missing installed package files');
  return root;
}

async function run(args, { cwd, env, log }) {
  const output = createWriteStream(log, { flags: 'wx' });
  const child = spawn(process.execPath, args, { cwd, env, stdio: ['ignore', 'pipe', 'pipe'] });
  child.stdout.pipe(output, { end: false });
  child.stderr.pipe(output, { end: false });
  let timedOut = false;
  const timeout = setTimeout(() => { timedOut = true; child.kill('SIGTERM'); }, 240_000);
  let force;
  child.once('exit', () => clearTimeout(force));
  timeout.unref();
  // Retain logs, not child processes, if a plugin fails to close.
  const watchdog = setTimeout(() => { force = setTimeout(() => child.kill('SIGKILL'), 5_000); }, 240_000);
  watchdog.unref();
  try {
    const code = await new Promise((resolve, reject) => {
      child.once('error', reject);
      child.once('close', resolve);
    });
    await new Promise(resolve => output.end(resolve));
    assert.ok(!timedOut, 'Installed plugin test timed out');
    assert.equal(code, 0, `Installed test failed; see ${log}`);
  } finally {
    clearTimeout(timeout); clearTimeout(watchdog); clearTimeout(force);
    output.end();
  }
}

for (const [name, directory, testFile] of cases) {
  const result = { plugin: name, integrity: false, catalog: false, businessMcp: testFile ? 'pending' : 'not_run' };
  try {
    const record = Object.values(installedPluginRecords).find(r => r.pluginKey.split('@')[0] === name);
    assert.ok(record, `Plugin is not installed: ${name}`);
    result.version = record.version;
    const root = await verifyInstallation(record);
    result.integrity = true;
    const manifest = JSON.parse(await readFile(path.join(root, 'chatos.plugin.json'), 'utf8'));
    const packageJSON = JSON.parse(await readFile(path.join(root, 'package.json'), 'utf8'));
    assert.equal(manifest.version, record.version);
    const server = Object.values(manifest.mcpServers)[0];
    const launcher = path.resolve(root, packageJSON.bin[server.bin]);
    assert.ok(launcher.startsWith(root + path.sep), 'Launcher escapes the package');
    const sandbox = path.join(outputRoot, directory);
    await mkdir(sandbox);
    const env = { ...process.env,
      CHATOS_PLUGIN_TEST_PACKAGE_ROOT: root,
      CHATOS_BROWSER_EXTENSION_ID: '',
      CHATOS_WORKSPACE: path.join(sandbox, 'workspace'),
      CHATOS_PLUGIN_DATA_DIR: path.join(sandbox, 'data'),
      CHATOS_PLUGIN_ARTIFACT_DIR: path.join(sandbox, 'artifacts'),
      VISUAL_COMPUTER_USE_MANAGED_APP_ROOT: path.join(sandbox, 'computer-use'),
      OPEN_COMPUTER_USE_MANAGED_APP_ROOT: path.join(sandbox, 'computer-use'),
    };
    delete env.CHATOS_BROWSER_BRIDGE_ENDPOINT;
    await Promise.all([mkdir(env.CHATOS_WORKSPACE), mkdir(env.CHATOS_PLUGIN_ARTIFACT_DIR)]);
    result.catalogLog = path.join(outputRoot, `${directory}-catalog.log`);
    await run([path.join(repository, 'scripts/audit-plugin-skill-gates.mjs'),
      '--manifest', path.join(root, 'chatos.plugin.json'), '--cwd', root, '--',
      process.execPath, launcher, ...(server.args || [])], { cwd: root, env, log: result.catalogLog });
    result.catalog = true;
    if (testFile) {
      result.businessLog = path.join(outputRoot, `${directory}-business.log`);
      await run(directory === 'computer-use' ? [testFile] : ['--test', testFile],
        { cwd: path.join(repository, 'plugins', directory), env, log: result.businessLog });
      result.businessMcp = 'passed';
    }
  } catch (error) {
    if (result.businessMcp === 'pending') result.businessMcp = 'failed';
    result.error = error.message;
    process.exitCode = 1;
  }
  results.push(result);
  console.log(JSON.stringify(result));
}
console.log(JSON.stringify({ outputRoot, allPassed: results.every(r => !r.error),
  note: 'Computer Use only checks permission status: no permission requests, clicks, typing, or desktop capture. Model/native UI acceptance is separate.' }));
