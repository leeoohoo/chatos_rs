// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const clientDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const tool = path.join(clientDir, 'prepare-plugin-bundles.mjs');
const pluginCatalog = path.join(
  clientDir,
  'plugin_bundles',
  'catalog',
  'bundled-plugin-catalog.json',
);
const skillCatalog = path.join(
  clientDir,
  'skill_bundles',
  'catalog',
  'internal-skill-catalog.json',
);
const skillRoot = path.join(clientDir, 'skill_bundles', 'internal');

function runTool(output, extra = [], catalog = pluginCatalog) {
  return spawnSync(process.execPath, [
    tool,
    ...extra,
    '--plugin-catalog', catalog,
    '--skill-catalog', skillCatalog,
    '--skill-root', skillRoot,
    '--output', output,
    '--platform', 'windows-x64',
  ], {
    cwd: clientDir,
    encoding: 'utf8',
  });
}

test('stages complete Plugin Bundles and rejects staged file tampering', () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), 'chatos-plugin-bundles-'));
  try {
    const output = path.join(temp, 'output');
    const staged = runTool(output);
    assert.equal(staged.status, 0, staged.stderr);
    const index = JSON.parse(
      fs.readFileSync(path.join(output, 'plugin-bundle-index.json'), 'utf8'),
    );
    assert.equal(index.plugins.length, 12);
    assert.equal(index.plugins.flatMap((plugin) => plugin.skills).length, 28);
    const documents = index.plugins.find((plugin) => plugin.name === 'documents');
    assert.equal(documents.release_id, 'bundled-release-documents-1-22-0');
    assert.equal(documents.version, '1.22.0');
    assert.equal(documents.published_at, '2026-07-25T16:00:00Z');
    assert.equal(
      documents.skills[0].bundle_hash,
      'c52035d10e378ef850ef56818e15ea90c6711285f04f166656c934566671bd7e',
    );
    assert.equal(
      fs.existsSync(path.join(
        output,
        'internal',
        'documents',
        '1.22.0',
        'skills',
        'documents',
        'SKILL.md',
      )),
      true,
    );
    const pdf = index.plugins.find((plugin) => plugin.name === 'pdf');
    assert.equal(pdf.release_id, 'bundled-release-pdf-1-9-0');
    assert.equal(pdf.version, '1.9.0');
    assert.equal(pdf.published_at, '2026-07-25T17:00:00Z');
    assert.equal(
      pdf.skills[0].bundle_hash,
      'f387766f7fe6f39550ec65e65a0f9f3729252b79a672c0692c3b88f764dd8e52',
    );
    assert.equal(
      fs.existsSync(path.join(
        output,
        'internal',
        'pdf',
        '1.9.0',
        'skills',
        'pdf',
        'SKILL.md',
      )),
      true,
    );
    const spreadsheets = index.plugins.find((plugin) => plugin.name === 'spreadsheets');
    assert.equal(spreadsheets.release_id, 'bundled-release-spreadsheets-1-2-0');
    assert.equal(spreadsheets.version, '1.2.0');
    assert.equal(spreadsheets.published_at, '2026-07-25T18:00:00Z');
    assert.equal(
      spreadsheets.skills.find((skill) => skill.skill_id === 'internal_skill_spreadsheets').bundle_hash,
      '3ee6b5a2c4473a0b70612697db8e35185f434662b9967d2ff8022ecf7d869246',
    );
    assert.equal(
      fs.existsSync(path.join(
        output,
        'internal',
        'spreadsheets',
        '1.2.0',
        'skills',
        'spreadsheets',
        'SKILL.md',
      )),
      true,
    );
    const presentations = index.plugins.find((plugin) => plugin.name === 'presentations');
    assert.equal(presentations.release_id, 'bundled-release-presentations-1-24-0');
    assert.equal(presentations.version, '1.24.0');
    assert.equal(presentations.published_at, '2026-07-26T13:00:00Z');
    assert.equal(
      presentations.skills[0].bundle_hash,
      'bdcc9542b49a5efc6b49d5b801552813e587596b693134715b95fe66dab8043d',
    );
    assert.equal(
      fs.existsSync(path.join(
        output,
        'internal',
        'presentations',
        '1.24.0',
        'skills',
        'presentations',
        'SKILL.md',
      )),
      true,
    );
    const templateCreator = index.plugins.find((plugin) => plugin.name === 'template-creator');
    assert.equal(templateCreator.release_id, 'bundled-release-template-creator-1-2-0');
    assert.equal(templateCreator.version, '1.2.0');
    assert.equal(templateCreator.published_at, '2026-07-25T20:00:00Z');
    assert.equal(
      templateCreator.skills[0].bundle_hash,
      '5d4544f1ebbd0b45d38fa1ffccda495cd6a137879089d00c1a987d0044a425b8',
    );
    assert.equal(
      fs.existsSync(path.join(
        output,
        'internal',
        'template-creator',
        '1.2.0',
        'skills',
        'template-creator',
        'SKILL.md',
      )),
      true,
    );
    const browser = index.plugins.find((plugin) => plugin.name === 'browser');
    assert.equal(browser.release_id, 'bundled-release-browser-1-8-0');
    assert.equal(browser.version, '1.8.0');
    assert.equal(browser.published_at, '2026-07-24T02:00:00Z');
    assert.equal(
      browser.skills[0].bundle_hash,
      'c4469e9afb44281fe86867a16328d5c809f82c32114ac82809e2bf552e771c49',
    );
    assert.equal(
      fs.existsSync(path.join(
        output,
        'internal',
        'browser',
        '1.8.0',
        'skills',
        'control-in-app-browser',
        'SKILL.md',
      )),
      true,
    );
    const computerUse = index.plugins.find((plugin) => plugin.name === 'computer-use');
    assert.equal(computerUse.release_id, 'bundled-release-computer-use-1-15-0');
    assert.equal(computerUse.version, '1.15.0');
    assert.equal(computerUse.published_at, '2026-07-25T15:00:00Z');
    assert.equal(
      computerUse.skills[0].bundle_hash,
      'a829b8ba08fd0f00edfe23424d14727824775a2e27ef29205fb0e63e13176424',
    );
    assert.equal(
      fs.existsSync(path.join(
        output,
        'internal',
        'computer-use',
        '1.15.0',
        'skills',
        'computer-use',
        'SKILL.md',
      )),
      true,
    );
    const chrome = index.plugins.find((plugin) => plugin.name === 'chrome');
    assert.equal(chrome.release_id, 'bundled-release-chrome-1-4-0');
    assert.equal(chrome.version, '1.4.0');
    assert.equal(chrome.published_at, '2026-07-25T07:00:00Z');
    assert.equal(
      chrome.skills[0].bundle_hash,
      '726cc47cd74273024422adfecc8770e023729464e8c74bcf9fc1bd04d1eef676',
    );
    assert.equal(
      fs.existsSync(path.join(
        output,
        'internal',
        'chrome',
        '1.4.0',
        'skills',
        'control-chrome',
        'SKILL.md',
      )),
      true,
    );

    const verified = runTool(output, ['--verify-only']);
    assert.equal(verified.status, 0, verified.stderr);
    fs.appendFileSync(
      path.join(
        output,
        'internal',
        'documents',
        '1.22.0',
        'skills',
        'documents',
        'instructions.md',
      ),
      '\ntampered\n',
    );
    const rejected = runTool(output, ['--verify-only']);
    assert.notEqual(rejected.status, 0);
    assert.match(rejected.stderr, /checksum mismatch/i);
  } finally {
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test('rejects Plugin catalog names that could escape staging', () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), 'chatos-plugin-catalog-'));
  try {
    const catalog = JSON.parse(fs.readFileSync(pluginCatalog, 'utf8'));
    catalog.plugins[0].name = '../escape';
    const unsafeCatalog = path.join(temp, 'catalog.json');
    fs.writeFileSync(unsafeCatalog, JSON.stringify(catalog));
    const rejected = runTool(path.join(temp, 'output'), [], unsafeCatalog);
    assert.notEqual(rejected.status, 0);
    assert.match(rejected.stderr, /invalid entry/i);
    assert.equal(fs.existsSync(path.join(temp, 'escape')), false);
  } finally {
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test('stages one independent Plugin Release without rewriting unchanged Plugin identities', () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), 'chatos-plugin-release-'));
  try {
    const baselineOutput = path.join(temp, 'baseline');
    const baseline = runTool(baselineOutput);
    assert.equal(baseline.status, 0, baseline.stderr);
    const baselineIndex = JSON.parse(
      fs.readFileSync(path.join(baselineOutput, 'plugin-bundle-index.json'), 'utf8'),
    );

    const catalog = JSON.parse(fs.readFileSync(pluginCatalog, 'utf8'));
    const documents = catalog.plugins.find((plugin) => plugin.name === 'documents');
    documents.release_version = '1.23.0';
    documents.release_epoch = '2026-07-25T17:00:00Z';
    documents.artifact_revision = 'documents-1.23.0';
    const upgradedCatalog = path.join(temp, 'catalog.json');
    fs.writeFileSync(upgradedCatalog, JSON.stringify(catalog));

    const upgradedOutput = path.join(temp, 'upgraded');
    const upgraded = runTool(upgradedOutput, [], upgradedCatalog);
    assert.equal(upgraded.status, 0, upgraded.stderr);
    const upgradedIndex = JSON.parse(
      fs.readFileSync(path.join(upgradedOutput, 'plugin-bundle-index.json'), 'utf8'),
    );
    const baselineDocuments = baselineIndex.plugins.find((plugin) => plugin.name === 'documents');
    const upgradedDocuments = upgradedIndex.plugins.find((plugin) => plugin.name === 'documents');
    const baselinePdf = baselineIndex.plugins.find((plugin) => plugin.name === 'pdf');
    const upgradedPdf = upgradedIndex.plugins.find((plugin) => plugin.name === 'pdf');

    assert.equal(upgradedDocuments.release_id, 'bundled-release-documents-1-23-0');
    assert.equal(upgradedDocuments.version, '1.23.0');
    assert.equal(upgradedDocuments.published_at, '2026-07-25T17:00:00Z');
    assert.notEqual(upgradedDocuments.artifact_sha256, baselineDocuments.artifact_sha256);
    assert.equal(upgradedPdf.release_id, baselinePdf.release_id);
    assert.equal(upgradedPdf.artifact_sha256, baselinePdf.artifact_sha256);
  } finally {
    fs.rmSync(temp, { recursive: true, force: true });
  }
});

test('rejects ambiguous or unsafe per-Plugin Release metadata', () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), 'chatos-plugin-release-invalid-'));
  try {
    const catalog = JSON.parse(fs.readFileSync(pluginCatalog, 'utf8'));
    catalog.plugins[0].release_version = '1.1.0-beta';
    catalog.plugins[0].artifact_revision = 'unsafe\nrevision';
    const invalidCatalog = path.join(temp, 'catalog.json');
    fs.writeFileSync(invalidCatalog, JSON.stringify(catalog));
    const rejected = runTool(path.join(temp, 'output'), [], invalidCatalog);
    assert.notEqual(rejected.status, 0);
    assert.match(rejected.stderr, /release version is invalid|artifact revision is invalid/i);
  } finally {
    fs.rmSync(temp, { recursive: true, force: true });
  }
});
