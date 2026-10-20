import { z } from "zod";

const safeText = (maximum: number) => z.string().min(1).max(maximum).refine(
  (value) => !/[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f]/u.test(value),
  "本机终端字段包含不安全的控制字符。",
);
const cursorSchema = z.number().int().nonnegative().safe();
export const localTerminalDimensionSchema = z.number().int().min(1).max(1_000);
export const localTerminalSessionIdSchema = safeText(128).refine(
  (value) => value.startsWith("local-terminal-") && /^[A-Za-z0-9-]+$/u.test(value),
  "本机终端会话标识无效。",
);

export const localTerminalStartSchema = z.object({
  sessionId: localTerminalSessionIdSchema,
  initialWorkingDirectory: safeText(4_096),
  shellName: safeText(128),
}).strict();

const base64Schema = z.string().min(1).max(384 * 1_024).regex(
  /^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/u,
);

export const localTerminalWireEventSchema = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("output"), cursor: cursorSchema, dataBase64: base64Schema }).strict(),
  z.object({
    kind: z.literal("terminated"),
    cursor: cursorSchema,
    exitCode: z.number().int().nullable(),
  }).strict(),
]);

export const localTerminalWirePollSchema = z.object({
  sessionId: localTerminalSessionIdSchema,
  running: z.boolean(),
  exitCode: z.number().int().nullable(),
  events: z.array(localTerminalWireEventSchema).max(128),
  nextCursor: cursorSchema,
  truncated: z.boolean(),
  timedOut: z.boolean(),
}).strict().superRefine((poll, context) => {
  let previous = -1;
  for (const event of poll.events) {
    if (event.cursor <= previous || event.cursor > poll.nextCursor) {
      context.addIssue({ code: "custom", message: "本机终端事件游标无效。" });
      break;
    }
    previous = event.cursor;
  }
});

export type LocalTerminalStart = z.infer<typeof localTerminalStartSchema>;
export type LocalTerminalWirePoll = z.infer<typeof localTerminalWirePollSchema>;

export type LocalTerminalEvent =
  | { kind: "output"; cursor: number; data: Uint8Array }
  | { kind: "terminated"; cursor: number; exitCode: number | null };

export interface LocalTerminalPoll {
  sessionId: string;
  running: boolean;
  exitCode: number | null;
  events: LocalTerminalEvent[];
  nextCursor: number;
  truncated: boolean;
  timedOut: boolean;
}

export function decodeLocalTerminalPoll(wire: LocalTerminalWirePoll): LocalTerminalPoll {
  return {
    ...wire,
    events: wire.events.map((event) => event.kind === "output"
      ? { kind: "output" as const, cursor: event.cursor, data: decodeBase64(event.dataBase64) }
      : event),
  };
}

export function encodeLocalTerminalInput(data: Uint8Array) {
  let binary = "";
  const chunkSize = 8_192;
  for (let offset = 0; offset < data.length; offset += chunkSize) {
    binary += String.fromCharCode(...data.subarray(offset, offset + chunkSize));
  }
  return btoa(binary);
}

export function localWorkingDirectoryFromOsc(value: string) {
  const normalized = value.trim();
  if (!normalized) return null;
  if (/^[A-Za-z]:[\\/]/u.test(normalized)) return normalized;
  try {
    const url = new URL(normalized);
    if (url.protocol !== "file:") return null;
    const path = decodeURIComponent(url.pathname);
    if (/^\/[A-Za-z]:\//u.test(path)) return path.slice(1);
    return /^[A-Za-z]:$/u.test(url.hostname) ? `${url.hostname}${path}` : path;
  } catch {
    return normalized.startsWith("/") ? normalized : null;
  }
}

export function localTerminalTitleForDirectory(directory: string) {
  const normalized = directory.replace(/[\\/]+$/u, "");
  const title = normalized.split(/[\\/]/u).filter(Boolean).at(-1);
  return title || "终端";
}

function decodeBase64(value: string) {
  const binary = atob(value);
  return Uint8Array.from(binary, (character) => character.charCodeAt(0));
}
