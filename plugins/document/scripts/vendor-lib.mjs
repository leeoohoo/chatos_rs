import { createHash } from 'node:crypto';
import { createReadStream, createWriteStream } from 'node:fs';
import { access, chmod, mkdir, readFile, rename, rm, stat, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { Readable } from 'node:stream';
import { pipeline } from 'node:stream/promises';
import { fileURLToPath } from 'node:url';

export const projectRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
export const vendorRoot = path.join(projectRoot, 'vendor');
export const manifestPath = path.join(vendorRoot, 'officecli-v1.0.144.json');

export async function loadVendorManifest() {
  return JSON.parse(await readFile(manifestPath, 'utf8'));
}

export async function sha256File(filePath) {
  return await new Promise((resolve, reject) => {
    const digest = createHash('sha256');
    const stream = createReadStream(filePath);
    stream.on('data', (chunk) => digest.update(chunk));
    stream.on('error', reject);
    stream.on('end', () => resolve(digest.digest('hex')));
  });
}

export async function verifyAsset(asset) {
  const target = path.join(vendorRoot, asset.target);
  const metadata = await stat(target).catch(() => undefined);
  if (!metadata?.isFile()) return { ok: false, target, reason: 'missing' };
  if (metadata.size !== asset.size) return { ok: false, target, reason: 'size' };
  const sha256 = await sha256File(target);
  if (sha256 !== asset.sha256) return { ok: false, target, reason: 'sha256' };
  return { ok: true, target, sha256, size: metadata.size };
}

export function selectedAssets(manifest, mode) {
  if (mode === 'all') return manifest.assets;
  return manifest.assets.filter(
    (asset) => asset.platform === process.platform && asset.arch === process.arch
  );
}

async function download(url, target) {
  const response = await fetch(url, { redirect: 'follow' });
  if (!response.ok || !response.body) {
    throw new Error(`download failed with HTTP ${response.status}: ${url}`);
  }
  await mkdir(path.dirname(target), { recursive: true });
  await pipeline(
    Readable.fromWeb(response.body),
    createWriteStream(target, { flags: 'wx', mode: 0o700 })
  );
}

export async function fetchAsset(asset) {
  const verified = await verifyAsset(asset);
  if (verified.ok) return { ...verified, downloaded: false };
  const target = path.join(vendorRoot, asset.target);
  const partial = `${target}.partial-${process.pid}-${Math.random().toString(16).slice(2)}`;
  await rm(partial, { force: true });
  try {
    await download(asset.url, partial);
    const metadata = await stat(partial);
    if (metadata.size !== asset.size) {
      throw new Error(`downloaded size mismatch for ${asset.sourceName}`);
    }
    const sha256 = await sha256File(partial);
    if (sha256 !== asset.sha256) {
      throw new Error(`downloaded SHA-256 mismatch for ${asset.sourceName}`);
    }
    await rm(target, { force: true });
    await rename(partial, target);
    if (asset.platform !== 'win32') await chmod(target, 0o755);
    return { ok: true, target, sha256, size: metadata.size, downloaded: true };
  } finally {
    await rm(partial, { force: true });
  }
}

export async function fetchLicense(manifest) {
  const asset = manifest.licenseAsset;
  const target = path.join(vendorRoot, asset.target);
  const existing = await access(target).then(() => true, () => false);
  if (existing && await sha256File(target) === asset.sha256) {
    return { target, downloaded: false };
  }
  const response = await fetch(asset.url);
  if (!response.ok) throw new Error(`license download failed with HTTP ${response.status}`);
  const bytes = new Uint8Array(await response.arrayBuffer());
  const sha256 = createHash('sha256').update(bytes).digest('hex');
  if (sha256 !== asset.sha256) throw new Error('OfficeCLI license SHA-256 mismatch');
  await mkdir(path.dirname(target), { recursive: true });
  await writeFile(target, bytes, { mode: 0o644 });
  return { target, downloaded: true };
}
