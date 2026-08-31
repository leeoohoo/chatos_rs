import { createHash } from 'node:crypto';
import { readdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const projectRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const outputPath = path.join(projectRoot, 'SBOM.cdx.json');
const licensesOutputPath = path.join(projectRoot, 'THIRD_PARTY_LICENSES.txt');
const pdfiumNoticesOutputPath = path.join(projectRoot, 'PDFIUM_THIRD_PARTY_NOTICES.txt');

async function json(relativePath) {
  return JSON.parse(await readFile(path.join(projectRoot, relativePath), 'utf8'));
}

function sha256(bytes) {
  return createHash('sha256').update(bytes).digest('hex');
}

function npmName(packagePath) {
  const marker = 'node_modules/';
  const index = packagePath.lastIndexOf(marker);
  const remainder = packagePath.slice(index + marker.length);
  const parts = remainder.split('/');
  return parts[0]?.startsWith('@') ? `${parts[0]}/${parts[1]}` : parts[0];
}

function npmPurl(name, version) {
  const encodedName = name.startsWith('@')
    ? `${encodeURIComponent(name.split('/')[0])}/${encodeURIComponent(name.split('/')[1])}`
    : encodeURIComponent(name);
  return `pkg:npm/${encodedName}@${version}`;
}

function integrityHash(integrity) {
  if (typeof integrity !== 'string') return undefined;
  const match = integrity.match(/^sha512-(.+)$/);
  if (!match?.[1]) return undefined;
  return { alg: 'SHA-512', content: Buffer.from(match[1], 'base64').toString('hex') };
}

const packageJson = await json('package.json');
const lock = await json('package-lock.json');
const metafile = await json('.build/server-metafile.json');
const office = await json('vendor/officecli-v1.0.144.json');
const pdfium = await json('vendor/pdfium-v7243.json');
if (pdfium.review?.status !== 'complete' || !pdfium.review.thirdPartyManifest) {
  throw new Error('PDFium third-party review is not complete');
}
const pdfiumThirdParty = await json(`vendor/${pdfium.review.thirdPartyManifest}`);
if (pdfiumThirdParty.subject?.wasmSha256 !== pdfium.asset.sha256) {
  throw new Error('PDFium third-party manifest does not match the packaged WASM');
}

const lockPaths = Object.keys(lock.packages)
  .filter((packagePath) => packagePath.startsWith('node_modules/'))
  .sort((left, right) => right.length - left.length);
const usedPaths = new Set();
for (const input of Object.keys(metafile.inputs)) {
  const matched = lockPaths.find((packagePath) => input === packagePath || input.startsWith(`${packagePath}/`));
  if (matched) usedPaths.add(matched);
}

const packageRows = [...usedPaths].map((packagePath) => {
  const details = lock.packages[packagePath];
  const name = npmName(packagePath);
  if (!details?.version || !details.license || !name) {
    throw new Error(`SBOM metadata is incomplete for ${packagePath}`);
  }
  return { packagePath, details, name, purl: npmPurl(name, details.version) };
});
const purlCounts = new Map();
for (const row of packageRows) purlCounts.set(row.purl, (purlCounts.get(row.purl) ?? 0) + 1);

const npmComponents = packageRows.map(({ packagePath, details, name, purl }) => {
  const bomRef = (purlCounts.get(purl) ?? 0) === 1
    ? purl
    : `${purl}?package_path=${encodeURIComponent(packagePath)}`;
  const hash = integrityHash(details.integrity);
  return {
    type: 'library',
    'bom-ref': bomRef,
    name,
    version: details.version,
    scope: 'required',
    purl,
    licenses: [{ expression: details.license }],
    ...(hash ? { hashes: [hash] } : {}),
    ...(details.resolved ? {
      externalReferences: [{ type: 'distribution', url: details.resolved }]
    } : {}),
    properties: [
      { name: 'chatos:bundle:packagePath', value: packagePath },
      { name: 'chatos:bundle:includedByEsbuild', value: 'true' }
    ]
  };
});

const officeRef = `pkg:github/iOfficeAI/OfficeCLI@${office.version}`;
const officeBinaryComponents = office.assets.map((asset) => ({
  type: 'file',
  'bom-ref': `file:vendor/${asset.target}`,
  name: asset.sourceName,
  version: office.version,
  hashes: [{ alg: 'SHA-256', content: asset.sha256 }],
  licenses: [{ expression: office.license }],
  externalReferences: [{ type: 'distribution', url: asset.url }],
  properties: [
    { name: 'chatos:platform', value: asset.platform },
    { name: 'chatos:architecture', value: asset.arch },
    { name: 'chatos:packagedPath', value: `vendor/${asset.target}` },
    { name: 'chatos:size', value: String(asset.size) }
  ]
}));
const officeComponent = {
  type: 'application',
  'bom-ref': officeRef,
  name: office.name,
  version: office.version,
  licenses: [{ expression: office.license }],
  externalReferences: [
    { type: 'vcs', url: `${office.repository}/tree/${office.tag}` },
    { type: 'website', url: office.repository }
  ],
  properties: [{ name: 'chatos:vendored', value: 'true' }]
};

const pdfiumRef = `file:${pdfium.asset.target}`;
const pdfiumComponent = {
  type: 'file',
  'bom-ref': pdfiumRef,
  name: pdfium.name,
  version: pdfium.builder.release,
  hashes: [{ alg: 'SHA-256', content: pdfium.asset.sha256 }],
  licenses: [{ expression: pdfium.upstream.license }],
  externalReferences: [
    { type: 'distribution', url: pdfium.builder.wasmArchiveUrl },
    { type: 'vcs', url: `${pdfium.upstream.repository}+/refs/heads/${pdfium.upstream.branch}` }
  ],
  properties: [
    { name: 'chatos:packagedPath', value: pdfium.asset.target },
    { name: 'chatos:size', value: String(pdfium.asset.size) },
    { name: 'chatos:builder', value: `${pdfium.builder.name} ${pdfium.builder.release}` },
    { name: 'chatos:wrapper', value: `${pdfium.wrapper.name} ${pdfium.wrapper.version}` },
    { name: 'chatos:licenseReviewStatus', value: pdfium.review.status },
    { name: 'chatos:licenseReviewManifest', value: `vendor/${pdfium.review.thirdPartyManifest}` },
    { name: 'chatos:licenseReviewScope', value: pdfium.review.scope }
  ]
};

const pdfiumThirdPartyComponents = pdfiumThirdParty.components.map((component) => {
  const purl = `pkg:generic/${encodeURIComponent(component.id)}@${encodeURIComponent(component.version)}`;
  return {
    type: component.id === 'foxit-font-data' ? 'data' : 'library',
    'bom-ref': purl,
    name: component.name,
    version: component.version,
    scope: 'required',
    purl,
    licenses: [{ expression: component.license }],
    externalReferences: [{ type: 'vcs', url: component.source }],
    properties: [
      { name: 'chatos:pdfium:componentId', value: component.id },
      { name: 'chatos:pdfium:revision', value: component.revision },
      { name: 'chatos:pdfium:evidence', value: component.evidence.join(' | ') },
      ...(component.upstreamRevision
        ? [{ name: 'chatos:pdfium:upstreamRevision', value: component.upstreamRevision }]
        : []),
      ...(component.notice
        ? [{ name: 'chatos:pdfium:notice', value: component.notice }]
        : [])
    ]
  };
});

const rootRef = npmPurl(packageJson.name, packageJson.version);
const allComponents = [
  ...npmComponents,
  officeComponent,
  ...officeBinaryComponents,
  pdfiumComponent,
  ...pdfiumThirdPartyComponents
].sort((left, right) => left['bom-ref'].localeCompare(right['bom-ref']));
const wrapper = npmComponents.find((component) => component.name === pdfium.wrapper.name);
const dependencies = [
  {
    ref: rootRef,
    dependsOn: allComponents
      .filter((component) => component['bom-ref'] !== rootRef)
      .map((component) => component['bom-ref'])
      .sort()
  },
  {
    ref: officeRef,
    dependsOn: officeBinaryComponents.map((component) => component['bom-ref']).sort()
  },
  {
    ref: pdfiumRef,
    dependsOn: pdfiumThirdPartyComponents.map((component) => component['bom-ref']).sort()
  },
  ...(wrapper ? [{ ref: wrapper['bom-ref'], dependsOn: [pdfiumRef] }] : []),
  {
    ref: `pkg:generic/emscripten@4.0.10`,
    dependsOn: ['musl', 'llvm-libcxx', 'llvm-libcxxabi', 'llvm-libunwind', 'llvm-compiler-rt']
      .map((id) => pdfiumThirdPartyComponents.find((component) =>
        component.properties.some((property) => property.name === 'chatos:pdfium:componentId' && property.value === id)
      )?.['bom-ref'])
      .filter(Boolean)
      .sort()
  }
].sort((left, right) => left.ref.localeCompare(right.ref));

const lockBytes = await readFile(path.join(projectRoot, 'package-lock.json'));
const metaBytes = await readFile(path.join(projectRoot, '.build', 'server-metafile.json'));
const sbom = {
  bomFormat: 'CycloneDX',
  specVersion: '1.5',
  version: 1,
  metadata: {
    tools: [{ vendor: 'Chatos', name: 'scripts/generate-sbom.mjs' }],
    component: {
      type: 'application',
      'bom-ref': rootRef,
      name: packageJson.name,
      version: packageJson.version,
      description: packageJson.description,
      purl: rootRef,
      licenses: [{ license: { name: packageJson.license } }]
    },
    properties: [
      { name: 'chatos:source:packageLockSha256', value: sha256(lockBytes) },
      { name: 'chatos:source:esbuildMetafileSha256', value: sha256(metaBytes) },
      { name: 'chatos:scope', value: 'Bundled production runtime and vendored binary assets' }
    ]
  },
  components: allComponents,
  dependencies
};
const serialized = `${JSON.stringify(sbom, null, 2)}\n`;

const licenseSections = [];
for (const row of [...packageRows].sort((left, right) => left.purl.localeCompare(right.purl))) {
  const directory = path.join(projectRoot, row.packagePath);
  const licenseFile = (await readdir(directory)).find((name) => /^(?:licen[cs]e|copying|notice)(?:[._-].*)?$/i.test(name));
  if (!licenseFile) throw new Error(`No license file was found for ${row.name}@${row.details.version}`);
  const licenseText = (await readFile(path.join(directory, licenseFile), 'utf8')).trim();
  licenseSections.push([
    `COMPONENT: ${row.name}@${row.details.version}`,
    `DECLARED LICENSE: ${row.details.license}`,
    `SOURCE: ${row.details.resolved ?? row.packagePath}`,
    '',
    licenseText
  ].join('\n'));
}
for (const item of [
  {
    component: `${office.name}@${office.version}`,
    license: office.license,
    source: `${office.repository}/tree/${office.tag}`,
    file: path.join(projectRoot, 'vendor', office.licenseAsset.target)
  },
  {
    component: `${pdfium.builder.name}@${pdfium.builder.release}`,
    license: pdfium.builder.license,
    source: `${pdfium.builder.repository}/tree/${pdfium.builder.release}`,
    file: path.join(projectRoot, 'vendor', pdfium.builder.licenseAsset.target)
  },
  {
    component: `${pdfium.upstream.name}@${pdfium.upstream.branch}`,
    license: pdfium.upstream.license,
    source: `${pdfium.upstream.repository}+/refs/heads/${pdfium.upstream.branch}`,
    file: path.join(projectRoot, 'vendor', pdfium.upstream.licenseAssets[0].target),
    note: 'The Apache-2.0 portion of the upstream license is reproduced in the OfficeCLI section above.'
  }
]) {
  const licenseText = (await readFile(item.file, 'utf8')).trim();
  licenseSections.push([
    `COMPONENT: ${item.component}`,
    `DECLARED LICENSE: ${item.license}`,
    `SOURCE: ${item.source}`,
    ...(item.note ? [`NOTE: ${item.note}`] : []),
    '',
    licenseText
  ].join('\n'));
}

const pdfiumNoticeSections = [];
for (const component of pdfiumThirdParty.components) {
  const assetSections = [];
  for (const asset of component.licenseAssets) {
    const licenseText = (await readFile(path.join(projectRoot, 'vendor', asset.target), 'utf8')).trim();
    licenseSections.push([
      `COMPONENT: ${component.name}@${component.version}`,
      `DECLARED LICENSE: ${component.license}`,
      `SOURCE: ${component.source}`,
      `REVISION: ${component.revision}`,
      `LICENSE FILE: vendor/${asset.target}`,
      ...(component.notice ? [`NOTICE: ${component.notice}`] : []),
      '',
      licenseText
    ].join('\n'));
    assetSections.push([
      `LICENSE FILE: vendor/${asset.target}`,
      `SHA-256: ${asset.sha256}`,
      '',
      licenseText
    ].join('\n'));
  }
  pdfiumNoticeSections.push([
    `COMPONENT: ${component.name}`,
    `VERSION: ${component.version}`,
    `REVISION: ${component.revision}`,
    `SOURCE: ${component.source}`,
    `LICENSE: ${component.license}`,
    ...(component.notice ? [`REQUIRED NOTICE: ${component.notice}`] : []),
    'BINARY/BUILD EVIDENCE:',
    ...component.evidence.map((evidence) => `- ${evidence}`),
    '',
    ...assetSections.map((section) => `${'-'.repeat(80)}\n${section}`)
  ].join('\n'));
}

const pdfiumNoticesSerialized = [
  'PDFIUM WEBASSEMBLY THIRD-PARTY NOTICES',
  '',
  `SUBJECT: PDFium ${pdfiumThirdParty.subject.pdfiumBranch} (${pdfiumThirdParty.subject.pdfiumCommit})`,
  `WASM SHA-256: ${pdfiumThirdParty.subject.wasmSha256}`,
  `AUDITED STATIC ARCHIVE SHA-256: ${pdfiumThirdParty.audit.sourceArchive.sha256}`,
  `AUDIT DATE: ${pdfiumThirdParty.audit.date}`,
  '',
  pdfiumThirdParty.audit.method,
  '',
  'This is a reproducible engineering inventory and redistribution-notice bundle for the fixed build, not legal advice.',
  '',
  'EXCLUDED FROM THIS BUILD:',
  ...pdfiumThirdParty.audit.excluded.map((item) => `- ${item.name}: ${item.reason}`),
  '',
  ...pdfiumNoticeSections.map((section) => `${'='.repeat(80)}\n${section}`),
  ''
].join('\n');

const licensesSerialized = [
  'THIRD-PARTY LICENSE TEXTS',
  '',
  'This file is generated from the production esbuild input graph and pinned vendor manifests.',
  'Do not edit it manually; run npm run sbom:generate.',
  '',
  ...licenseSections.map((section) => `${'='.repeat(80)}\n${section}`),
  ''
].join('\n');

if (process.argv.includes('--check')) {
  const existing = await readFile(outputPath, 'utf8').catch(() => undefined);
  const existingLicenses = await readFile(licensesOutputPath, 'utf8').catch(() => undefined);
  const existingPdfiumNotices = await readFile(pdfiumNoticesOutputPath, 'utf8').catch(() => undefined);
  if (existing !== serialized || existingLicenses !== licensesSerialized || existingPdfiumNotices !== pdfiumNoticesSerialized) {
    throw new Error('SBOM or third-party notice bundle is missing or stale; run npm run sbom:generate');
  }
  process.stdout.write(`verified SBOM.cdx.json and third-party notice bundles (${allComponents.length} components)\n`);
} else {
  await writeFile(outputPath, serialized);
  await writeFile(licensesOutputPath, licensesSerialized);
  await writeFile(pdfiumNoticesOutputPath, pdfiumNoticesSerialized);
  process.stdout.write(`generated SBOM.cdx.json and third-party notice bundles (${allComponents.length} components)\n`);
}
