import { z } from "zod";

const safeText = (maximum: number) => z.string().min(1).max(maximum).refine(
  (value) => !/[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f]/u.test(value),
  "远程终端字段包含不安全的控制字符。",
);
const cursorSchema = z.number().int().nonnegative().safe();
export const terminalDimensionSchema = z.number().int().min(1).max(1_000);
export const remoteTerminalSessionIdSchema = safeText(128).refine(
  (value) => value.startsWith("remote-terminal-") && /^[A-Za-z0-9-]+$/u.test(value),
  "远程终端会话标识无效。",
);

export const remoteTerminalStartSchema = z.discriminatedUnion("status", [
  z.object({
    status: z.literal("started"),
    sessionId: remoteTerminalSessionIdSchema,
    initialWorkingDirectory: safeText(4_096),
  }).strict(),
  z.object({ status: z.literal("verification_required"), prompt: safeText(64 * 1_024) }).strict(),
]);

const base64Schema = z.string().min(1).max(384 * 1_024).regex(/^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/u);

export const remoteTerminalWireEventSchema = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("output"), cursor: cursorSchema, dataBase64: base64Schema }).strict(),
  z.object({ kind: z.literal("terminated"), cursor: cursorSchema, exitCode: z.number().int().nullable() }).strict(),
]);

export const remoteTerminalWirePollSchema = z.object({
  sessionId: remoteTerminalSessionIdSchema,
  running: z.boolean(),
  exitCode: z.number().int().nullable(),
  events: z.array(remoteTerminalWireEventSchema).max(128),
  nextCursor: cursorSchema,
  truncated: z.boolean(),
  timedOut: z.boolean(),
}).strict().superRefine((poll, context) => {
  let previous = -1;
  for (const event of poll.events) {
    if (event.cursor <= previous || event.cursor > poll.nextCursor) {
      context.addIssue({ code: "custom", message: "远程终端事件游标无效。" });
      break;
    }
    previous = event.cursor;
  }
});

export type RemoteTerminalStart = z.infer<typeof remoteTerminalStartSchema>;
export type RemoteTerminalWirePoll = z.infer<typeof remoteTerminalWirePollSchema>;

export type RemoteTerminalEvent =
  | { kind: "output"; cursor: number; data: Uint8Array }
  | { kind: "terminated"; cursor: number; exitCode: number | null };

export interface RemoteTerminalPoll {
  sessionId: string;
  running: boolean;
  exitCode: number | null;
  events: RemoteTerminalEvent[];
  nextCursor: number;
  truncated: boolean;
  timedOut: boolean;
}

export function decodeRemoteTerminalPoll(wire: RemoteTerminalWirePoll): RemoteTerminalPoll {
  return {
    ...wire,
    events: wire.events.map((event) => event.kind === "output"
      ? { kind: "output" as const, cursor: event.cursor, data: decodeBase64(event.dataBase64) }
      : event),
  };
}

export function encodeTerminalInput(data: Uint8Array) {
  let binary = "";
  const chunkSize = 8_192;
  for (let offset = 0; offset < data.length; offset += chunkSize) {
    binary += String.fromCharCode(...data.subarray(offset, offset + chunkSize));
  }
  return btoa(binary);
}

export function workingDirectoryFromOsc(value: string) {
  const normalized = value.trim();
  if (!normalized) return null;
  try {
    const url = new URL(normalized);
    return url.protocol === "file:" ? decodeURIComponent(url.pathname) : normalized;
  } catch {
    return normalized.startsWith("/") ? normalized : null;
  }
}

function decodeBase64(value: string) {
  const binary = atob(value);
  return Uint8Array.from(binary, (character) => character.charCodeAt(0));
}
