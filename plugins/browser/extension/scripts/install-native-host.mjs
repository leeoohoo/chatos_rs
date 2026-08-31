import {copyFile, mkdir, readFile, writeFile} from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import {fileURLToPath} from 'node:url';

export const NATIVE_HOST_NAME = 'ai.chatos.browser_bridge';

export async function installNativeHost({binary, extensionId, browser = 'chrome'}) {
  validateExtensionId(extensionId);
  if (!path.isAbsolute(binary)) throw new Error('Native host binary path must be absolute');
  const manifest = {
    name: NATIVE_HOST_NAME,
    description: 'Chatos Browser MCP native messaging bootstrap',
    path: binary,
    type: 'stdio',
    allowed_origins: [`chrome-extension://${extensionId}/`]
  };
  const manifestPath = nativeManifestPath(browser);
  await mkdir(path.dirname(manifestPath), {recursive: true});
  try {
    const existing = await readFile(manifestPath, 'utf8');
    if (existing !== `${JSON.stringify(manifest, null, 2)}\n`) {
      await copyFile(manifestPath, `${manifestPath}.backup-${Date.now()}`);
    }
  } catch (error) {
    if (error.code !== 'ENOENT') throw error;
  }
  await writeFile(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`, {mode: 0o600});
  return manifestPath;
}

function nativeManifestPath(browser) {
  if (process.platform === 'darwin') {
    const roots = {
      chrome: 'Google/Chrome',
      chromium: 'Chromium',
      edge: 'Microsoft Edge'
    };
    const product = roots[browser];
    if (!product) throw new Error(`Unsupported browser: ${browser}`);
    return path.join(
      os.homedir(),
      'Library',
      'Application Support',
      product,
      'NativeMessagingHosts',
      `${NATIVE_HOST_NAME}.json`
    );
  }
  if (process.platform === 'linux') {
    const roots = {
      chrome: 'google-chrome',
      chromium: 'chromium',
      edge: 'microsoft-edge'
    };
    const product = roots[browser];
    if (!product) throw new Error(`Unsupported browser: ${browser}`);
    return path.join(os.homedir(), '.config', product, 'NativeMessagingHosts', `${NATIVE_HOST_NAME}.json`);
  }
  throw new Error('Development native-host installer currently supports macOS and Linux');
}

function validateExtensionId(extensionId) {
  if (!/^[a-p]{32}$/.test(extensionId)) {
    throw new Error('Extension ID must contain 32 characters in the range a-p');
  }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const options = parseArguments(process.argv.slice(2));
  const installed = await installNativeHost(options);
  process.stdout.write(`Installed native host manifest at ${installed}\n`);
}

function parseArguments(args) {
  const options = {browser: 'chrome'};
  for (let index = 0; index < args.length; index += 2) {
    const name = args[index];
    const value = args[index + 1];
    if (!value) throw new Error(`Missing value for ${name}`);
    if (name === '--binary') options.binary = path.resolve(value);
    else if (name === '--extension-id') options.extensionId = value;
    else if (name === '--browser') options.browser = value;
    else throw new Error(`Unknown argument: ${name}`);
  }
  if (!options.binary || !options.extensionId) {
    throw new Error('Usage: install-native-host.mjs --binary <path> --extension-id <id> [--browser chrome]');
  }
  return options;
}
