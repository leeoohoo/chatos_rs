import { createHash } from 'node:crypto';
import { createReadStream } from 'node:fs';

export async function sha256File(filePath: string): Promise<string> {
  return await new Promise((resolve, reject) => {
    const digest = createHash('sha256');
    const stream = createReadStream(filePath);
    stream.on('data', (chunk) => digest.update(chunk));
    stream.on('error', reject);
    stream.on('end', () => resolve(digest.digest('hex')));
  });
}
