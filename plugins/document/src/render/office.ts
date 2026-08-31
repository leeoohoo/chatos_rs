import { runOfficeCli, OFFICECLI_VERSION } from '../engines/officecli.js';

export interface OfficeRenderTarget {
  page: number;
  outputPath: string;
}

export async function openOfficeWithEngine(absolutePath: string, cwd: string): Promise<void> {
  await runOfficeCli(['view', absolutePath, 'stats', '--json'], cwd, 60_000);
}

export async function renderOfficePages(
  absolutePath: string,
  cwd: string,
  targets: OfficeRenderTarget[],
  viewportWidth: number,
  viewportHeight: number
): Promise<void> {
  for (const target of targets) {
    await runOfficeCli(
      [
        'view',
        absolutePath,
        'screenshot',
        '--page',
        String(target.page),
        '--out',
        target.outputPath,
        '--screenshot-width',
        String(viewportWidth),
        '--screenshot-height',
        String(viewportHeight),
        '--render',
        'html',
        '--json'
      ],
      cwd
    );
  }
}

export async function renderOfficeDocumentStack(
  absolutePath: string,
  cwd: string,
  outputPath: string,
  maximumPages: number,
  viewportWidth: number,
  viewportHeight: number
): Promise<void> {
  await runOfficeCli(
    [
      'view',
      absolutePath,
      'screenshot',
      '--page',
      `1-${maximumPages}`,
      '--out',
      outputPath,
      '--screenshot-width',
      String(viewportWidth),
      '--screenshot-height',
      String(viewportHeight),
      '--render',
      'html',
      '--json'
    ],
    cwd
  );
}

export async function renderOfficeRange(
  absolutePath: string,
  cwd: string,
  outputPath: string,
  range: string,
  viewportWidth: number,
  viewportHeight: number
): Promise<void> {
  await runOfficeCli(
    [
      'view',
      absolutePath,
      'screenshot',
      '--range',
      range,
      '--out',
      outputPath,
      '--screenshot-width',
      String(viewportWidth),
      '--screenshot-height',
      String(viewportHeight),
      '--render',
      'html',
      '--json'
    ],
    cwd
  );
}

export { OFFICECLI_VERSION };
