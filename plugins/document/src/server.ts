import { Server } from '@modelcontextprotocol/sdk/server/index.js';
import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js';
import { CallToolRequestSchema, ListToolsRequestSchema } from '@modelcontextprotocol/sdk/types.js';
import { createHash } from 'node:crypto';
import path from 'node:path';
import { SERVER_NAME, SERVER_VERSION } from './constants.js';
import { callTool, TOOL_DEFINITIONS } from './tools.js';
import { toPublicError } from './errors.js';

interface ArtifactCandidate {
  producer_artifact_id: string;
  relative_path: string;
  display_name: string;
  media_type: string;
  size_bytes: number;
  sha256: string;
}

function artifactCandidates(value: unknown): ArtifactCandidate[] {
  const candidates: ArtifactCandidate[] = [];
  const seen = new Set<string>();

  function visit(current: unknown): void {
    if (candidates.length >= 64 || !current || typeof current !== 'object') return;
    if (Array.isArray(current)) {
      for (const child of current) visit(child);
      return;
    }

    const item = current as Record<string, unknown>;
    const relativePath = item.relativePath;
    const mediaType = item.mimeType;
    const size = item.size;
    const sha256 = item.sha256;
    if (
      typeof relativePath === 'string'
      && path.basename(relativePath) === relativePath
      && typeof mediaType === 'string'
      && Number.isSafeInteger(size)
      && (size as number) >= 0
      && typeof sha256 === 'string'
      && /^[0-9a-f]{64}$/.test(sha256)
    ) {
      const identity = `${relativePath}\0${mediaType}\0${size}\0${sha256}`;
      if (!seen.has(identity)) {
        seen.add(identity);
        candidates.push({
          producer_artifact_id: `document_${createHash('sha256').update(identity).digest('hex')}`,
          relative_path: relativePath,
          display_name: relativePath,
          media_type: mediaType,
          size_bytes: size as number,
          sha256
        });
      }
      return;
    }

    for (const child of Object.values(item)) visit(child);
  }

  visit(value);
  return candidates;
}

function jsonResult(value: Record<string, unknown>, isError = false) {
  const result: Record<string, unknown> = {
    content: [{ type: 'text' as const, text: JSON.stringify(value, null, 2) }],
    structuredContent: value,
    isError
  };
  if (!isError) {
    const candidates = artifactCandidates(value);
    if (candidates.length > 0) {
      result._meta = { 'chatos/artifacts': candidates };
    }
  }
  return result;
}

async function runMcp(): Promise<void> {
  const server = new Server(
    { name: SERVER_NAME, version: SERVER_VERSION },
    { capabilities: { tools: {} } }
  );
  server.setRequestHandler(ListToolsRequestSchema, async () => ({ tools: [...TOOL_DEFINITIONS] }));
  server.setRequestHandler(CallToolRequestSchema, async (request) => {
    try {
      return jsonResult(await callTool(request.params.name, request.params.arguments ?? {}));
    } catch (error) {
      return jsonResult(toPublicError(error), true);
    }
  });
  const transport = new StdioServerTransport();
  await server.connect(transport);
}

function usage(): string {
  return [
    'Usage: chatos-document-mcp mcp',
    '       chatos-document-mcp --version',
    '',
    'CHATOS_WORKSPACE must point to the bound workspace before file tools are called.'
  ].join('\n');
}

async function main(): Promise<void> {
  const command = process.argv[2];
  if (command === '--version' || command === '-v') {
    process.stdout.write(`${SERVER_VERSION}\n`);
    return;
  }
  if (command === 'mcp') {
    await runMcp();
    return;
  }
  process.stderr.write(`${usage()}\n`);
  process.exitCode = 2;
}

await main().catch((error) => {
  const message = error instanceof Error ? error.message : String(error);
  process.stderr.write(`Document MCP failed: ${message}\n`);
  process.exitCode = 1;
});
