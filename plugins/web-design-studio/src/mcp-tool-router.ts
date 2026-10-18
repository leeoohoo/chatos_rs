import { objectArguments } from './mcp-tool-helpers.js';
import { callDocumentTool } from './mcp-document-tool-router.js';
import { callProgressiveTool } from './mcp-progressive-tool-router.js';

export async function callTool(
  name: string,
  rawArguments: unknown
): Promise<Record<string, unknown>> {
  const argumentsValue = objectArguments(rawArguments);
  const progressiveResult = await callProgressiveTool(name, argumentsValue);
  if (progressiveResult !== undefined) return progressiveResult;
  const documentResult = await callDocumentTool(name, argumentsValue);
  if (documentResult !== undefined) return documentResult;
  throw new Error(`Unknown Web Design Studio tool: ${name}`);
}
