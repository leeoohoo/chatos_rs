import { invoke } from "@tauri-apps/api/core";
import { clipboardHistorySchema } from "../domain/clipboardHistory";
import type { ClipboardHistoryPort } from "../ports/ClipboardHistoryPort";

export class TauriClipboardHistoryAdapter implements ClipboardHistoryPort {
  async list() { return clipboardHistorySchema.parse(await invoke("list_clipboard_history")); }
  async setPinned(id: string, pinned: boolean) { return clipboardHistorySchema.parse(await invoke("set_clipboard_history_entry_pinned", { id, pinned })); }
  async delete(id: string) { return clipboardHistorySchema.parse(await invoke("delete_clipboard_history_entry", { id })); }
  async clear() { return clipboardHistorySchema.parse(await invoke("clear_clipboard_history")); }
  async restore(id: string) { await invoke("restore_clipboard_history_entry", { id }); }
  async close() { await invoke("close_clipboard_history"); }
}
