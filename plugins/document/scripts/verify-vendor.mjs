import { readFile, stat } from 'node:fs/promises';
import path from 'node:path';
import {
  loadVendorManifest,
  projectRoot,
  selectedAssets,
  sha256File,
  vendorRoot,
  verifyAsset
} from './vendor-lib.mjs';

const mode = process.argv.includes('--all') ? 'all' : 'current';
const manifest = await loadVendorManifest();
const assets = selectedAssets(manifest, mode);
for (const asset of assets) {
  const result = await verifyAsset(asset);
  if (!result.ok) {
    throw new Error(`OfficeCLI verification failed (${result.reason}): ${asset.target}`);
  }
  process.stdout.write(`verified ${asset.platform}-${asset.arch} ${result.sha256}\n`);
}
const licensePath = path.join(vendorRoot, manifest.licenseAsset.target);
if (await sha256File(licensePath) !== manifest.licenseAsset.sha256) {
  throw new Error('OfficeCLI license verification failed');
}

const pdfiumManifest = JSON.parse(await readFile(path.join(vendorRoot, 'pdfium-v7243.json'), 'utf8'));
const pdfiumSource = path.join(projectRoot, pdfiumManifest.asset.source);
const pdfiumMetadata = await stat(pdfiumSource).catch(() => undefined);
if (!pdfiumMetadata?.isFile() || pdfiumMetadata.size !== pdfiumManifest.asset.size) {
  throw new Error('PDFium WebAssembly size verification failed');
}
if (await sha256File(pdfiumSource) !== pdfiumManifest.asset.sha256) {
  throw new Error('PDFium WebAssembly SHA-256 verification failed');
}
for (const asset of [
  pdfiumManifest.wrapper.licenseAsset,
  pdfiumManifest.builder.licenseAsset,
  ...pdfiumManifest.upstream.licenseAssets
]) {
  if (await sha256File(path.join(vendorRoot, asset.target)) !== asset.sha256) {
    throw new Error(`PDFium license verification failed: ${asset.target}`);
  }
}
if (pdfiumManifest.review?.status !== 'complete' || !pdfiumManifest.review.thirdPartyManifest) {
  throw new Error('PDFium third-party review is not complete');
}
const pdfiumThirdPartyManifest = JSON.parse(await readFile(
  path.join(vendorRoot, pdfiumManifest.review.thirdPartyManifest),
  'utf8'
));
if (pdfiumThirdPartyManifest.subject?.wasmSha256 !== pdfiumManifest.asset.sha256) {
  throw new Error('PDFium third-party manifest does not match the pinned WASM');
}
if (pdfiumThirdPartyManifest.subject?.pdfiumBranch !== pdfiumManifest.upstream.branch) {
  throw new Error('PDFium third-party manifest does not match the pinned branch');
}
const expectedPdfiumComponents = [
  'abseil',
  'agg',
  'emscripten',
  'fast-float',
  'foxit-font-data',
  'freetype',
  'icu',
  'libjpeg-turbo',
  'little-cms',
  'llvm-compiler-rt',
  'llvm-libcxx',
  'llvm-libcxxabi',
  'llvm-libunwind',
  'musl',
  'openjpeg',
  'zlib'
];
const actualPdfiumComponents = pdfiumThirdPartyManifest.components
  .map((component) => component.id)
  .sort();
if (JSON.stringify(actualPdfiumComponents) !== JSON.stringify(expectedPdfiumComponents)) {
  throw new Error('PDFium third-party component inventory is incomplete or unexpected');
}
for (const component of pdfiumThirdPartyManifest.components) {
  if (!component.name || !component.version || !component.revision || !component.source ||
      !component.license || !Array.isArray(component.evidence) || component.evidence.length === 0 ||
      !Array.isArray(component.licenseAssets) || component.licenseAssets.length === 0) {
    throw new Error(`PDFium third-party metadata is incomplete: ${component.id}`);
  }
  for (const asset of component.licenseAssets) {
    if (await sha256File(path.join(vendorRoot, asset.target)) !== asset.sha256) {
      throw new Error(`PDFium third-party license verification failed: ${asset.target}`);
    }
  }
}
const builtPdfium = path.join(projectRoot, pdfiumManifest.asset.target);
const builtMetadata = await stat(builtPdfium).catch(() => undefined);
if (builtMetadata && await sha256File(builtPdfium) !== pdfiumManifest.asset.sha256) {
  throw new Error('Built PDFium WebAssembly differs from the pinned source asset');
}
process.stdout.write(`verified pdfium-wasm ${pdfiumManifest.asset.sha256}\n`);
process.stdout.write(`verified pdfium-third-party ${actualPdfiumComponents.length} components\n`);
