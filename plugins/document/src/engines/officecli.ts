import { createHash } from 'node:crypto';
import { execFile } from 'node:child_process';
import { readFile, stat } from 'node:fs/promises';
import path from 'node:path';
import { promisify } from 'node:util';
import { fileURLToPath } from 'node:url';
import { DocumentError } from '../errors.js';

const execFileAsync = promisify(execFile);
const EXPECTED_VERSION = '1.0.144';
const packageRoot = process.env.CHATOS_PLUGIN_ROOT?.trim()
  ? path.resolve(process.env.CHATOS_PLUGIN_ROOT)
  : path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');

interface VendorAsset {
  platform: string;
  arch: string;
  target: string;
  size: number;
  sha256: string;
}

interface VendorManifest {
  version: string;
  assets: VendorAsset[];
}

let verifiedBinaryPromise: Promise<string> | undefined;

function sha256(bytes: Uint8Array): string {
  return createHash('sha256').update(bytes).digest('hex');
}

async function verifyCurrentBinary(): Promise<string> {
  const manifestPath = path.join(packageRoot, 'vendor', 'officecli-v1.0.144.json');
  const manifest = JSON.parse(await readFile(manifestPath, 'utf8')) as VendorManifest;
  if (manifest.version !== EXPECTED_VERSION) {
    throw new DocumentError('ENGINE_UNAVAILABLE', 'The bundled OfficeCLI manifest version is invalid.');
  }
  const asset = manifest.assets.find(
    (candidate) => candidate.platform === process.platform && candidate.arch === process.arch
  );
  if (!asset) {
    throw new DocumentError('ENGINE_UNAVAILABLE', `OfficeCLI is not bundled for ${process.platform}-${process.arch}.`);
  }
  const binaryPath = path.join(packageRoot, 'vendor', asset.target);
  const metadata = await stat(binaryPath).catch(() => undefined);
  if (!metadata?.isFile() || metadata.size !== asset.size) {
    throw new DocumentError('ENGINE_UNAVAILABLE', 'The bundled OfficeCLI binary is missing or has the wrong size.');
  }
  const bytes = await readFile(binaryPath);
  if (sha256(bytes) !== asset.sha256) {
    throw new DocumentError('ENGINE_UNAVAILABLE', 'The bundled OfficeCLI binary failed SHA-256 verification.');
  }
  const version = await runRaw(binaryPath, ['--version'], process.cwd(), 15_000);
  if (version.trim() !== EXPECTED_VERSION) {
    throw new DocumentError('ENGINE_UNAVAILABLE', 'The bundled OfficeCLI executable reported an unexpected version.');
  }
  return binaryPath;
}

async function runRaw(binaryPath: string, args: string[], cwd: string, timeout: number): Promise<string> {
  try {
    const result = await execFileAsync(binaryPath, args, {
      cwd,
      env: {
        ...process.env,
        OFFICECLI_SKIP_UPDATE: '1',
        OFFICECLI_NO_AUTO_INSTALL: '1',
        OFFICECLI_NO_AUTO_RESIDENT: '1',
        OFFICECLI_RESIDENT_FLUSH: 'each'
      },
      timeout,
      maxBuffer: 2 * 1024 * 1024,
      windowsHide: true,
      encoding: 'utf8'
    });
    return result.stdout;
  } catch (error) {
    const value = error as NodeJS.ErrnoException & { killed?: boolean; signal?: string };
    if (value.killed || value.signal === 'SIGTERM') {
      throw new DocumentError('ENGINE_TIMEOUT', 'OfficeCLI exceeded the operation timeout.');
    }
    throw new DocumentError('ENGINE_ERROR', 'OfficeCLI could not complete the requested document operation.');
  }
}

export async function runOfficeCli(args: string[], cwd: string, timeout = 120_000): Promise<void> {
  verifiedBinaryPromise ??= verifyCurrentBinary();
  const binaryPath = await verifiedBinaryPromise;
  await runRaw(binaryPath, args, cwd, timeout);
}

export const OFFICECLI_VERSION = EXPECTED_VERSION;
