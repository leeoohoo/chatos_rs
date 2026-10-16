import { copyFile, lstat, readFile, rm, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { randomUUID } from 'node:crypto';
import { EXCLUSIVE_COPY_FLAG, resolveArtifactPaths } from '../security/artifacts.js';
import { runOfficeCli, OFFICECLI_VERSION } from '../engines/officecli.js';
import { inspectOoxml } from '../inspect/ooxml.js';
import { sha256File } from '../inspect/hash.js';
import { resolveWorkspaceFile } from '../security/paths.js';
import { DocumentError } from '../errors.js';
import { translateOperations, type OfficeFormat } from './operations.js';

const EXTENSIONS: Record<OfficeFormat, string> = { docx: '.docx', xlsx: '.xlsx', pptx: '.pptx' };

function validateLocale(locale: unknown, operations: unknown): string {
  if (locale === undefined || locale === 'auto') {
    const content = JSON.stringify(operations ?? []);
    if (/\p{Script=Hiragana}|\p{Script=Katakana}/u.test(content)) return 'ja';
    if (/\p{Script=Hangul}/u.test(content)) return 'ko';
    if (/\p{Script=Han}/u.test(content)) return 'zh-CN';
    if (/\p{Script=Arabic}/u.test(content)) return 'ar';
    if (/\p{Script=Hebrew}/u.test(content)) return 'he';
    return 'en-US';
  }
  if (typeof locale !== 'string' || !/^[A-Za-z]{2,3}(?:-[A-Za-z0-9]{2,8}){0,2}$/.test(locale)) {
    throw new DocumentError('INVALID_ARGUMENT', 'locale must be a short BCP-47 language tag.');
  }
  return locale;
}

async function applyBatch(
  filePath: string,
  format: OfficeFormat,
  operations: unknown,
  root: string,
  operationLimit = 500
): Promise<void> {
  const commands = translateOperations(format, operations, operationLimit);
  if (commands.length === 0) return;
  const phases = format === 'docx'
    ? [
        commands.filter((command) => command.command === 'set' && command.path === '/'),
        commands.filter((command) => command.command === 'add' && command.parent === '/styles'),
        commands.filter((command) => !(
          (command.command === 'set' && command.path === '/')
          || (command.command === 'add' && command.parent === '/styles')
          || (command.command === 'add' && command.parent === '/' && (command.type === 'header' || command.type === 'footer'))
        )),
        commands.filter((command) => command.command === 'add' && command.parent === '/' && (command.type === 'header' || command.type === 'footer'))
      ].filter((phase) => phase.length > 0)
    : [commands];
  for (const phase of phases) for (let offset = 0; offset < phase.length; offset += 500) {
    const commandFile = path.join(root, `.document-mcp-commands-${randomUUID()}.json`);
    try {
      await writeFile(commandFile, JSON.stringify(phase.slice(offset, offset + 500)), { flag: 'wx', mode: 0o600 });
      await runOfficeCli(
        ['batch', filePath, '--input', commandFile, '--stop-on-error', '--json'],
        root
      );
    } finally {
      await rm(commandFile, { force: true });
    }
  }
}

async function publishValidatedArtifact(
  temporaryPath: string,
  outputPath: string,
  outputName: string,
  format: OfficeFormat
): Promise<Record<string, unknown>> {
  try {
    const data = await readFile(temporaryPath);
    const validation = inspectOoxml(data, EXTENSIONS[format]);
    await copyFile(temporaryPath, outputPath, EXCLUSIVE_COPY_FLAG).catch((error: NodeJS.ErrnoException) => {
      if (error.code === 'EEXIST') throw new DocumentError('OUTPUT_EXISTS', 'The output artifact was created by another operation.');
      throw error;
    });
    const metadata = await lstat(outputPath);
    return {
      relativePath: outputName,
      size: metadata.size,
      sha256: await sha256File(outputPath),
      mimeType: validation.mimeType
    };
  } catch (error) {
    if (error instanceof DocumentError) throw error;
    throw new DocumentError('VALIDATION_FAILED', 'The generated Office document failed validation.');
  } finally {
    await rm(temporaryPath, { force: true });
  }
}

export async function createOfficeArtifact(args: Record<string, unknown>): Promise<Record<string, unknown>> {
  const format = args.format;
  if (format !== 'docx' && format !== 'xlsx' && format !== 'pptx') {
    throw new DocumentError('INVALID_ARGUMENT', 'format must be docx, xlsx, or pptx.');
  }
  if (typeof args.outputName !== 'string') throw new DocumentError('INVALID_ARGUMENT', 'outputName is required.');
  const paths = await resolveArtifactPaths(args.outputName, EXTENSIONS[format]);
  try {
    await runOfficeCli(
      ['create', paths.temporaryPath, '--type', format, '--locale', validateLocale(args.locale, args.operations), '--json'],
      paths.root
    );
    await applyBatch(paths.temporaryPath, format, args.operations ?? [], paths.root);
    const artifact = await publishValidatedArtifact(
      paths.temporaryPath,
      paths.outputPath,
      paths.outputName,
      format
    );
    return {
      ok: true,
      operation: 'office_create',
      format,
      artifact,
      engine: { name: 'OfficeCLI', version: OFFICECLI_VERSION },
      validation: { status: 'structural_passed', visualReviewStatus: 'required', warnings: [] },
      requiredNextAction: {
        tool: 'document_render',
        inputPath: artifact.relativePath,
        reason: 'Inspect the returned page images before delivering the document.'
      }
    };
  } catch (error) {
    await rm(paths.temporaryPath, { force: true });
    throw error;
  }
}

export async function editOfficeArtifact(
  args: Record<string, unknown>,
  internalOperationLimit = 500
): Promise<Record<string, unknown>> {
  if (typeof args.inputPath !== 'string' || typeof args.outputName !== 'string') {
    throw new DocumentError('INVALID_ARGUMENT', 'inputPath and outputName are required.');
  }
  const source = await resolveWorkspaceFile(args.inputPath);
  const extension = path.extname(source.relativePath).toLowerCase();
  const format = extension.slice(1) as OfficeFormat;
  if (!(format in EXTENSIONS)) {
    throw new DocumentError('UNSUPPORTED_FORMAT', 'office_edit_batch supports .docx, .xlsx, and .pptx.');
  }
  const paths = await resolveArtifactPaths(args.outputName, extension);
  try {
    await copyFile(source.absolutePath, paths.temporaryPath, EXCLUSIVE_COPY_FLAG);
    await applyBatch(paths.temporaryPath, format, args.operations ?? [], paths.root, internalOperationLimit);
    const artifact = await publishValidatedArtifact(
      paths.temporaryPath,
      paths.outputPath,
      paths.outputName,
      format
    );
    return {
      ok: true,
      operation: 'office_edit_batch',
      format,
      source: { relativePath: source.relativePath, size: source.size, sha256: await sha256File(source.absolutePath) },
      artifact,
      engine: { name: 'OfficeCLI', version: OFFICECLI_VERSION },
      validation: { status: 'structural_passed', visualReviewStatus: 'required', warnings: [] },
      requiredNextAction: {
        tool: 'document_render',
        inputPath: artifact.relativePath,
        reason: 'Inspect every visually changed page before delivering the document.'
      }
    };
  } catch (error) {
    await rm(paths.temporaryPath, { force: true });
    throw error;
  }
}
