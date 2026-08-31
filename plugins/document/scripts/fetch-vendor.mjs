import { fetchAsset, fetchLicense, loadVendorManifest, selectedAssets } from './vendor-lib.mjs';

const mode = process.argv.includes('--all') ? 'all' : 'current';
const manifest = await loadVendorManifest();
const assets = selectedAssets(manifest, mode);
if (assets.length === 0) {
  throw new Error(`OfficeCLI has no pinned asset for ${process.platform}-${process.arch}`);
}

await fetchLicense(manifest);
const results = await Promise.all(
  assets.map(async (asset) => {
    process.stdout.write(`OfficeCLI ${asset.platform}-${asset.arch}: checking ${asset.sourceName}\n`);
    return { asset, result: await fetchAsset(asset) };
  })
);
for (const { asset, result } of results) {
  process.stdout.write(
    `OfficeCLI ${asset.platform}-${asset.arch}: ${result.downloaded ? 'downloaded' : 'verified'} (${result.size} bytes)\n`
  );
}
