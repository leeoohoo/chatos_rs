import assert from 'node:assert/strict';
import { access, readFile } from 'node:fs/promises';
import path from 'node:path';

const root = path.resolve(import.meta.dirname, '..');
const packageJson = JSON.parse(await readFile(path.join(root, 'package.json'), 'utf8'));
const manifest = JSON.parse(await readFile(path.join(root, 'chatos.plugin.json'), 'utf8'));
assert.equal(packageJson.name, '@chatos/project-management');
assert.equal(manifest.name, 'chatos-project-management');
assert.equal(packageJson.version, manifest.version);
assert.equal(packageJson.bin['chatos-project-management'], 'bin/chatos-project-management');
assert.equal(manifest.mcpServers['project-management-mcp'].bin, 'chatos-project-management');
assert.equal(manifest.ui[0].runtime.bin, 'chatos-project-management');
assert.equal(manifest.runtimeContext.scope, 'project');
assert.equal(manifest.runtimeContext.missingContext, 'reject');
for (const file of packageJson.files) await access(path.join(root, file));
process.stdout.write(`verified ${manifest.name} ${manifest.version}\n`);
