#!/usr/bin/env node

import { spawn } from 'node:child_process';

const scripts = process.argv.slice(2);
if (scripts.length === 0 || scripts.some((name) => !/^[a-z0-9:-]+$/i.test(name))) {
  process.stderr.write('usage: run-plugin-prepack.mjs NPM_SCRIPT [NPM_SCRIPT...]\n');
  process.exit(2);
}

const npm = process.platform === 'win32' ? 'npm.cmd' : 'npm';
for (const script of scripts) {
  const code = await new Promise((resolve, reject) => {
    const child = spawn(npm, ['run', '--silent', script], {
      cwd: process.cwd(),
      shell: process.platform === 'win32',
      stdio: ['inherit', process.stderr, process.stderr]
    });
    child.once('error', reject);
    child.once('exit', (exitCode, signal) => {
      if (signal) reject(new Error(`npm script ${script} terminated by ${signal}`));
      else resolve(exitCode ?? 1);
    });
  });
  if (code !== 0) process.exit(code);
}
