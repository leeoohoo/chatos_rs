import {execFileSync} from 'node:child_process';
import {mkdir, readFile, writeFile} from 'node:fs/promises';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {randomUUID} from 'node:crypto';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const metadata = JSON.parse(
  execFileSync('cargo', ['metadata', '--locked', '--format-version', '1'], {
    cwd: root,
    encoding: 'utf8'
  })
);
const packageJson = JSON.parse(await readFile(path.join(root, 'npm', 'package.json'), 'utf8'));
const packages = new Map(metadata.packages.map(pkg => [pkg.id, pkg]));
const nodes = new Map((metadata.resolve?.nodes ?? []).map(node => [node.id, node]));
const rootPackage = metadata.packages.find(pkg => pkg.name === 'browser-cdp-cli');
if (!rootPackage) throw new Error('browser-cdp-cli is missing from cargo metadata');

const reachable = new Set();
const pending = [rootPackage.id];
while (pending.length > 0) {
  const id = pending.pop();
  if (reachable.has(id)) continue;
  reachable.add(id);
  for (const dependency of nodes.get(id)?.deps ?? []) pending.push(dependency.pkg);
}

const componentFor = pkg => {
  const component = {
    type: metadata.workspace_members.includes(pkg.id) ? 'application' : 'library',
    'bom-ref': cargoRef(pkg),
    name: pkg.name,
    version: pkg.version,
    purl: cargoRef(pkg),
    properties: [
      {name: 'cargo:source', value: pkg.source ?? 'workspace'},
      {name: 'cargo:manifest_path', value: path.relative(root, pkg.manifest_path)}
    ]
  };
  if (pkg.license) component.licenses = [{expression: pkg.license}];
  if (pkg.checksum) component.hashes = [{alg: 'SHA-256', content: pkg.checksum}];
  if (pkg.repository) {
    component.externalReferences = [{type: 'vcs', url: pkg.repository}];
  }
  return component;
};

const selected = [...reachable]
  .map(id => packages.get(id))
  .filter(Boolean)
  .sort((left, right) => cargoRef(left).localeCompare(cargoRef(right)));
const dependencies = selected.map(pkg => ({
  ref: cargoRef(pkg),
  dependsOn: (nodes.get(pkg.id)?.deps ?? [])
    .map(dependency => packages.get(dependency.pkg))
    .filter(dependency => dependency && reachable.has(dependency.id))
    .map(cargoRef)
    .sort()
}));

const bom = {
  bomFormat: 'CycloneDX',
  specVersion: '1.5',
  serialNumber: `urn:uuid:${randomUUID()}`,
  version: 1,
  metadata: {
    timestamp: new Date().toISOString(),
    tools: {
      components: [{type: 'application', name: 'browser-cdp-sbom-generator', version: packageJson.version}]
    },
    component: {
      type: 'application',
      'bom-ref': `pkg:npm/${packageJson.name}@${packageJson.version}`,
      name: packageJson.name,
      version: packageJson.version,
      purl: `pkg:npm/${packageJson.name}@${packageJson.version}`
    }
  },
  components: selected.map(componentFor),
  dependencies
};

const output = path.join(root, 'npm', 'sbom.cdx.json');
await mkdir(path.dirname(output), {recursive: true});
await writeFile(output, `${JSON.stringify(bom, null, 2)}\n`);
process.stdout.write(`Generated ${output} with ${bom.components.length} components\n`);

function cargoRef(pkg) {
  return `pkg:cargo/${encodeURIComponent(pkg.name)}@${pkg.version}`;
}
