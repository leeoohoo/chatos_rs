import { getCurrentWindow } from "@tauri-apps/api/window";
import type { WindowChromePort } from "../ports/WindowChromePort";

export class TauriWindowChromeAdapter implements WindowChromePort {
  minimize(): Promise<void> {
    return getCurrentWindow().minimize();
  }

  toggleMaximize(): Promise<void> {
    return getCurrentWindow().toggleMaximize();
  }

  close(): Promise<void> {
    return getCurrentWindow().close();
  }
}
