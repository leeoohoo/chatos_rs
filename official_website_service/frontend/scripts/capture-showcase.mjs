#!/usr/bin/env node
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import fs from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const targets = [
  ['chatos-main', 'http://127.0.0.1:8088'],
  ['memory-engine', 'http://127.0.0.1:4178'],
  ['task-runner', 'http://127.0.0.1:39091'],
  ['sandbox-manager', 'http://127.0.0.1:8096'],
  ['project-management', 'http://127.0.0.1:39211'],
];

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const outputDir = path.resolve(__dirname, '../public/showcase');

async function main() {
  let chromium;
  try {
    ({ chromium } = await import('playwright'));
  } catch (error) {
    console.error(
      'Missing optional dependency "playwright". Install it temporarily with: npm install --no-save playwright',
    );
    throw error;
  }
  await fs.mkdir(outputDir, { recursive: true });
  const browser = await chromium.launch();
  const page = await browser.newPage({ viewport: { width: 1440, height: 960 } });

  for (const [name, url] of targets) {
    try {
      await page.goto(url, { waitUntil: 'networkidle', timeout: 12_000 });
      await page.screenshot({
        path: path.join(outputDir, `${name}.png`),
        fullPage: false,
      });
      console.log(`[ok] ${name} <- ${url}`);
    } catch (error) {
      console.warn(`[skip] ${name} <- ${url}: ${error.message}`);
    }
  }

  await browser.close();
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
