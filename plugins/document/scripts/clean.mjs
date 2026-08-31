import { rm } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const projectRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
await rm(path.join(projectRoot, 'dist'), { recursive: true, force: true });
await rm(path.join(projectRoot, '.build'), { recursive: true, force: true });
