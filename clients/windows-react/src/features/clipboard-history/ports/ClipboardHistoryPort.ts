import type { ClipboardHistory } from "../domain/clipboardHistory";

export interface ClipboardHistoryPort {
  list(): Promise<ClipboardHistory>;
  setPinned(id: string, pinned: boolean): Promise<ClipboardHistory>;
  delete(id: string): Promise<ClipboardHistory>;
  clear(): Promise<ClipboardHistory>;
  restore(id: string): Promise<void>;
  close(): Promise<void>;
}
