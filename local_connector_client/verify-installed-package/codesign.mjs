// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import path from 'node:path';
import process from 'node:process';
import { spawnSync } from 'node:child_process';

function codesignDetails(target, deep) {
  const verifyArgs = ['--verify', '--strict'];
  if (deep) verifyArgs.push('--deep');
  verifyArgs.push('--verbose=2', target);
  const verified = spawnSync('/usr/bin/codesign', verifyArgs, { encoding: 'utf8', maxBuffer: 1024 * 1024 });
  if (verified.error || verified.status !== 0) {
    throw new Error('macOS code signature verification failed');
  }
  const details = spawnSync('/usr/bin/codesign', ['-d', '--verbose=4', target], {
    encoding: 'utf8',
    maxBuffer: 1024 * 1024,
  });
  if (details.error || details.status !== 0) {
    throw new Error('Unable to inspect macOS code signature metadata');
  }
  const output = `${details.stdout || ''}\n${details.stderr || ''}`;
  const teamIdentifier = output.match(/^TeamIdentifier=(.+)$/m)?.[1]?.trim();
  if (!teamIdentifier || teamIdentifier === 'not set') {
    throw new Error('macOS signed package is missing a TeamIdentifier');
  }
  return teamIdentifier;
}

export function verifyMacCodeSigning(args, executablePaths) {
  if (!args.requireSigned) {
    return { required: false, verified: false, team_identifier: null };
  }
  if (!args.platform.startsWith('macos-') || process.platform !== 'darwin') {
    throw new Error('--require-signed is only supported for macOS packages on macOS');
  }
  const contents = path.dirname(args.resources);
  const appRoot = path.dirname(contents);
  if (path.basename(args.resources) !== 'Resources' || path.basename(contents) !== 'Contents' || !appRoot.endsWith('.app')) {
    throw new Error('Signed macOS verification requires an .app/Contents/Resources directory');
  }
  const teamIdentifiers = new Set([codesignDetails(appRoot, true)]);
  for (const executablePath of executablePaths) {
    teamIdentifiers.add(codesignDetails(executablePath, false));
  }
  if (teamIdentifiers.size !== 1) {
    throw new Error('macOS app/Core/helpers do not share one TeamIdentifier');
  }
  return { required: true, verified: true, team_identifier: [...teamIdentifiers][0] };
}
