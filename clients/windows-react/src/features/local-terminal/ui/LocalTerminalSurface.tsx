import { forwardRef } from "react";
import { TerminalSurface, type TerminalSurfaceHandle } from "../../terminal/ui/TerminalSurface";
import { useLocalTerminalStore } from "../application/localTerminalStore";
import { localWorkingDirectoryFromOsc } from "../domain/localTerminal";
import type { LocalTerminalPort } from "../ports/LocalTerminalPort";

export type LocalTerminalSurfaceHandle = TerminalSurfaceHandle;

export const LocalTerminalSurface = forwardRef<
  LocalTerminalSurfaceHandle,
  { port: LocalTerminalPort; tabId: string }
>(function LocalTerminalSurface({ port, tabId }, ref) {
  const tab = useLocalTerminalStore((state) => state.tabs.find((candidate) => candidate.id === tabId));
  return (
    <TerminalSurface
      ariaLabel="本机终端输出"
      className="local-terminal-surface"
      events={tab?.events ?? []}
      lifecycleKey={tabId}
      onInput={(data) => useLocalTerminalStore.getState().sendInput(port, tabId, data)}
      onOsc7={(value) => {
        const directory = localWorkingDirectoryFromOsc(value);
        if (directory) useLocalTerminalStore.getState().updateWorkingDirectory(tabId, directory);
      }}
      onResize={(cols, rows) => { void useLocalTerminalStore.getState().resize(port, tabId, cols, rows); }}
      onTitleChange={(title) => useLocalTerminalStore.getState().updateTitle(tabId, title)}
      outputGeneration={tab?.outputGeneration ?? 0}
      ref={ref}
    />
  );
});
