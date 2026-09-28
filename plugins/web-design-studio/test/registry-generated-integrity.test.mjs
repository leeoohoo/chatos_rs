import assert from 'node:assert/strict';
import { mkdtemp, mkdir, rm, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import {
  buildRegistryLock,
  verifyRegistryLock,
  writeRegistryLock
} from '../scripts/registry-generated-integrity.mjs';

test('registry integrity lock rejects hand-edited generated output', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'registry-generated-integrity-'));
  const groups = {
    demo: ['src/demo-registry.generated.ts', 'ui-src/library-runtime/vendor/demo']
  };

  try {
    await mkdir(path.join(root, 'src'), { recursive: true });
    await mkdir(path.join(root, 'ui-src/library-runtime/vendor/demo'), { recursive: true });
    await writeFile(path.join(root, 'src/demo-registry.generated.ts'), '// generated\n', 'utf8');
    await writeFile(path.join(root, 'ui-src/library-runtime/vendor/demo/button.tsx'), 'export const Button = 1;\n', 'utf8');

    const lockPath = path.join(root, 'registry-generated-lock.json');
    await writeRegistryLock(root, lockPath, groups);
    const lock = await buildRegistryLock(root, groups);
    assert.equal(lock.version, 1);
    await assert.doesNotReject(() => verifyRegistryLock(root, lockPath, groups));

    await writeFile(path.join(root, 'src/demo-registry.generated.ts'), '// hand edited\n', 'utf8');
    await assert.rejects(
      () => verifyRegistryLock(root, lockPath, groups),
      /demo registry generated output does not match its integrity lock/
    );
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
