import {access, mkdir, readFile, rm} from 'node:fs/promises';
import path from 'node:path';
import {spawn} from 'node:child_process';
import {fileURLToPath} from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const manifest = JSON.parse(await readFile(path.join(root, 'manifest.json'), 'utf8'));
const outputDirectory = path.join(root, 'release');
const output = path.join(outputDirectory, `chatos-browser-bridge-${manifest.version}.zip`);

await import('./build.mjs');
await mkdir(outputDirectory, {recursive: true});
await rm(output, {force: true});
await run('zip', ['-qr', output, '.'], path.join(root, 'dist'));
await access(output);
process.stdout.write(`Packaged Chrome Web Store upload at ${output}\n`);

function run(command, args, cwd) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, {cwd, shell: false, stdio: ['ignore', 'pipe', 'pipe']});
    let stderr = '';
    child.stderr.setEncoding('utf8');
    child.stderr.on('data', chunk => {
      stderr = `${stderr}${chunk}`.slice(-16 * 1024);
    });
    child.on('error', error => reject(new Error(`Could not run ${command}: ${error.message}`)));
    child.on('exit', code => {
      if (code === 0) resolve();
      else reject(new Error(`${command} exited with status ${code}: ${stderr}`));
    });
  });
}
