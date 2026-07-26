// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const clientDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const macosScript = fs.readFileSync(
  path.join(clientDir, 'prepare-document-runtime-macos.sh'),
  'utf8',
);
const windowsScript = fs.readFileSync(
  path.join(clientDir, 'prepare-document-runtime-windows.ps1'),
  'utf8',
);

const pinnedFontSha256 =
  '450625c8d46ab3df97b7904ded955ec2746d17ec76740cb1e91d1ba63a0f89af';

test('document runtime packagers pin the same font and runtime revision', () => {
  for (const source of [macosScript, windowsScript]) {
    assert.match(source, new RegExp(pinnedFontSha256));
    assert.match(source, /libreoffice-poppler-2026-07-25\.1/);
    assert.match(source, /NotoSansSC-OFL\.txt/);
    assert.match(source, /fonts\/NotoSansSC-Regular\.ttf|fonts\\NotoSansSC-Regular\.ttf/);
  }
});

test('Windows runtime manifest is written as UTF-8 without a BOM', () => {
  assert.match(windowsScript, /UTF8Encoding\(\$false\)/);
  assert.match(windowsScript, /\[System\.IO\.File\]::WriteAllText/);
  assert.doesNotMatch(windowsScript, /Set-Content[^\r\n]*runtime\.json/);
});
