import { Server } from '@modelcontextprotocol/sdk/server/index.js';
import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js';
import { CallToolRequestSchema, ListToolsRequestSchema } from '@modelcontextprotocol/sdk/types.js';
import { RevisionConflictError } from './document-store.js';
import { GenerationPlanRevisionConflictError } from './v2/generation-plan-store.js';
import { type ToolImagePayload } from './v2/generation-visual-service.js';
import { SceneRevisionConflictError } from './v2/scene-store.js';
import { store, TOOL_DEFINITIONS } from './mcp-runtime-context.js';
import { callTool } from './mcp-tool-router.js';

const SERVER_NAME = 'chatos-web-design-studio';
const SERVER_VERSION = '3.0.33';

interface HandoffArtifactFile {
  producerArtifactId: string;
  relativePath: string;
  displayName: string;
  mimeType: string;
  size: number;
  sha256: string;
}

function result(value: Record<string, unknown>, isError = false) {
  const sourceImages = Array.isArray(value.__images) ? value.__images as ToolImagePayload[] : [];
  const { __images: _discardedImages, ...structuredContent } = value;
  const response: Record<string, unknown> = {
    content: [
      { type: 'text' as const, text: JSON.stringify(structuredContent) },
      ...sourceImages.map((image) => ({ type: 'image' as const, data: image.data, mimeType: image.mimeType }))
    ],
    structuredContent,
    isError
  };
  if (!isError && Array.isArray(value.files)) {
    const candidates = value.files.flatMap((item) => {
      if (!item || typeof item !== 'object' || Array.isArray(item)) return [];
      const file = item as Partial<HandoffArtifactFile>;
      if (!file.producerArtifactId || !file.relativePath || !file.displayName || !file.mimeType
        || !Number.isSafeInteger(file.size) || !file.sha256) return [];
      return [{
        producer_artifact_id: file.producerArtifactId,
        relative_path: file.relativePath,
        display_name: file.displayName,
        media_type: file.mimeType,
        size_bytes: file.size,
        sha256: file.sha256
      }];
    });
    if (candidates.length > 0) response._meta = { 'chatos/artifacts': candidates };
  }
  return response;
}

async function runMcp(): Promise<void> {
  await store.initialize();
  await store.ensureLegacyProject();
  const server = new Server({ name: SERVER_NAME, version: SERVER_VERSION }, { capabilities: { tools: {} } });
  server.setRequestHandler(ListToolsRequestSchema, async () => ({ tools: [...TOOL_DEFINITIONS] }));
  server.setRequestHandler(CallToolRequestSchema, async (request) => {
    try {
      return result(await callTool(request.params.name, request.params.arguments ?? {}));
    } catch (error) {
      return result({
        error: error instanceof Error ? error.message : String(error),
        ...(error instanceof RevisionConflictError || error instanceof GenerationPlanRevisionConflictError || error instanceof SceneRevisionConflictError
          ? { actualRevision: error.actualRevision }
          : {})
      }, true);
    }
  });
  await server.connect(new StdioServerTransport());
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
  process.stderr.write('Usage: chatos-web-design-studio mcp\n');
  process.exitCode = 2;
}

await main().catch((error) => {
  process.stderr.write(`Web Design Studio failed: ${error instanceof Error ? error.message : String(error)}\n`);
  process.exitCode = 1;
});
